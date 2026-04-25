use std::path::{Path, PathBuf};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility},
    store::GraphStore,
};
use kuzu::{Connection, Database, SystemConfig, Value};

use crate::{branch, schema as db_schema};

// ── KuzuGraphStore ────────────────────────────────────────────────────────────

/// Local KuzuDB-backed implementation of [`GraphStore`].
///
/// One database file per repo (`graph.kuzu`), with per-branch node/edge tables
/// inside it. A fresh `Connection` is created for each operation so we avoid
/// the self-referential lifetime that `Mutex<Connection<'db>>` would require.
pub struct KuzuGraphStore {
    db: Database,
    repo_id: String,
}

impl KuzuGraphStore {
    /// Open (or create) the graph database for the repo at `repo_root`.
    pub fn open(repo_root: &Path) -> Result<Self> {
        let repo_id = branch::repo_id(repo_root);
        let db_path = branch::db_path(&repo_id);

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Database::new(&db_path, SystemConfig::default())
            .map_err(|e| GitCortexError::Store(format!("open db: {e}")))?;

        Ok(Self { db, repo_id })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn conn(&self) -> Result<Connection<'_>> {
        Connection::new(&self.db)
            .map_err(|e| GitCortexError::Store(format!("open connection: {e}")))
    }

    fn ensure_branch(&self, branch: &str) -> Result<()> {
        let mut conn = self.conn()?;
        db_schema::ensure_branch(&mut conn, branch)
    }
}

// ── GraphStore impl ───────────────────────────────────────────────────────────

impl GraphStore for KuzuGraphStore {
    // ── Write path ────────────────────────────────────────────────────────────

    fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()> {
        if diff.is_empty() {
            return Ok(());
        }

        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let conn = self.conn()?;

        // 1. Remove all nodes (and their edges) for deleted/replaced files.
        for file in &diff.removed_files {
            let file_str = esc(file.to_string_lossy().as_ref());
            conn.query(&format!(
                "MATCH (n:{nt}) WHERE n.file = '{file_str}' DETACH DELETE n"
            ))
            .map_err(|e| GitCortexError::Store(format!("delete file nodes: {e}")))?;
        }

        // 2. Remove explicit node IDs.
        for id in &diff.removed_node_ids {
            let id_str = esc(&id.as_str());
            conn.query(&format!(
                "MATCH (n:{nt}) WHERE n.id = '{id_str}' DETACH DELETE n"
            ))
            .map_err(|e| GitCortexError::Store(format!("delete node: {e}")))?;
        }

        // 3. Remove explicit edges.
        for (src, dst, kind) in &diff.removed_edges {
            let s = esc(&src.as_str());
            let d = esc(&dst.as_str());
            let k = esc(&kind.to_string());
            conn.query(&format!(
                "MATCH (s:{nt})-[e:{et}]->(d:{nt}) \
                 WHERE s.id = '{s}' AND d.id = '{d}' AND e.kind = '{k}' \
                 DELETE e"
            ))
            .map_err(|e| GitCortexError::Store(format!("delete edge: {e}")))?;
        }

        // 4. Insert new nodes.
        for node in &diff.added_nodes {
            let id = esc(&node.id.as_str());
            let kind = esc(&node.kind.to_string());
            let name = esc(&node.name);
            let qname = esc(&node.qualified_name);
            let file = esc(node.file.to_string_lossy().as_ref());
            let sl = node.span.start_line as i64;
            let el = node.span.end_line as i64;
            let loc = node.metadata.loc as i64;
            let vis = esc(&vis_str(&node.metadata.visibility));
            let is_async = node.metadata.is_async;
            let is_unsafe = node.metadata.is_unsafe;

            conn.query(&format!(
                "CREATE (:{nt} {{\
                    id: '{id}', kind: '{kind}', name: '{name}', \
                    qualified_name: '{qname}', file: '{file}', \
                    start_line: {sl}, end_line: {el}, loc: {loc}, \
                    visibility: '{vis}', is_async: {is_async}, is_unsafe: {is_unsafe}\
                }})"
            ))
            .map_err(|e| GitCortexError::Store(format!("insert node '{name}': {e}")))?;
        }

        // 5. Insert new edges. If either endpoint is absent (cross-file), MATCH
        //    yields no rows and the CREATE is silently skipped — correct behaviour.
        for edge in &diff.added_edges {
            let s = esc(&edge.src.as_str());
            let d = esc(&edge.dst.as_str());
            let k = esc(&edge.kind.to_string());

            conn.query(&format!(
                "MATCH (s:{nt} {{id: '{s}'}}), (d:{nt} {{id: '{d}'}}) \
                 CREATE (s)-[:{et} {{kind: '{k}'}}]->(d)"
            ))
            .map_err(|e| GitCortexError::Store(format!("insert edge: {e}")))?;
        }

        Ok(())
    }

    // ── Read path ─────────────────────────────────────────────────────────────

    fn lookup_symbol(&self, branch: &str, name: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let name_esc = esc(name);
        let conn = self.conn()?;

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.name = '{name_esc}' \
                 RETURN {NODE_COLS}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        rows_to_nodes(&mut result)
    }

    fn find_callers(&self, branch: &str, function_name: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(function_name);
        let conn = self.conn()?;

        let mut result = conn
            .query(&format!(
                "MATCH (caller:{nt})-[e:{et} {{kind: 'calls'}}]->(callee:{nt}) \
                 WHERE callee.name = '{name_esc}' \
                 RETURN caller.id, caller.kind, caller.name, caller.qualified_name, \
                        caller.file, caller.start_line, caller.end_line, caller.loc, \
                        caller.visibility, caller.is_async, caller.is_unsafe"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        rows_to_nodes(&mut result)
    }

    fn list_definitions(&self, branch: &str, file: &Path) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let file_esc = esc(file.to_string_lossy().as_ref());
        let conn = self.conn()?;

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.file = '{file_esc}' \
                 RETURN {NODE_COLS} ORDER BY n.start_line"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        rows_to_nodes(&mut result)
    }

    fn branch_diff(&self, from: &str, to: &str) -> Result<GraphDiff> {
        self.ensure_branch(from)?;
        self.ensure_branch(to)?;

        let from_nt = db_schema::node_table(from);
        let to_nt = db_schema::node_table(to);
        let mut conn = self.conn()?;

        // Collect node IDs from each branch.
        let from_ids = collect_ids(&mut conn, &from_nt)?;
        let to_ids = collect_ids(&mut conn, &to_nt)?;

        // Nodes in `to` but not in `from` → added.
        let added_ids: Vec<&String> =
            to_ids.iter().filter(|id| !from_ids.contains(*id)).collect();

        // Nodes in `from` but not in `to` → removed.
        let removed_ids: Vec<&String> =
            from_ids.iter().filter(|id| !to_ids.contains(*id)).collect();

        let mut diff = GraphDiff::default();

        for id in added_ids {
            let id_esc = esc(id);
            let mut r = conn
                .query(&format!(
                    "MATCH (n:{to_nt}) WHERE n.id = '{id_esc}' RETURN {NODE_COLS}"
                ))
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            diff.added_nodes.extend(rows_to_nodes(&mut r)?);
        }

        for id in removed_ids {
            if let Ok(node_id) = NodeId::try_from(id.as_str()) {
                diff.removed_node_ids.push(node_id);
            }
        }

        Ok(diff)
    }

    fn list_all_nodes(&self, branch: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let conn = self.conn()?;
        let mut result = conn
            .query(&format!("MATCH (n:{nt}) RETURN {NODE_COLS}"))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut result)
    }

    fn list_all_edges(&self, branch: &str) -> Result<Vec<Edge>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let conn = self.conn()?;
        let mut result = conn
            .query(&format!(
                "MATCH (s:{nt})-[e:{et}]->(d:{nt}) RETURN s.id, d.id, e.kind"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        let mut out = Vec::new();
        while let Some(row) = result.next() {
            let src_str = str_val(&row[0])?;
            let dst_str = str_val(&row[1])?;
            let kind_str = str_val(&row[2])?;
            out.push(Edge {
                src: NodeId::try_from(src_str.as_str())
                    .map_err(|e| GitCortexError::Store(format!("bad src id: {e}")))?,
                dst: NodeId::try_from(dst_str.as_str())
                    .map_err(|e| GitCortexError::Store(format!("bad dst id: {e}")))?,
                kind: edge_kind_from_str(&kind_str),
            });
        }
        Ok(out)
    }

    // ── Indexing state ────────────────────────────────────────────────────────

    fn last_indexed_sha(&self, branch_name: &str) -> Result<Option<String>> {
        branch::read_last_sha(&self.repo_id, branch_name)
    }

    fn set_last_indexed_sha(&mut self, branch_name: &str, sha: &str) -> Result<()> {
        branch::write_last_sha(&self.repo_id, branch_name, sha)
    }
}

// ── Query helpers ─────────────────────────────────────────────────────────────

/// Fixed column projection used in all node-returning queries.
/// Order must match `row_to_node()`.
const NODE_COLS: &str =
    "n.id, n.kind, n.name, n.qualified_name, n.file, \
     n.start_line, n.end_line, n.loc, n.visibility, n.is_async, n.is_unsafe";

fn rows_to_nodes(result: &mut kuzu::QueryResult) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    while let Some(row) = result.next() {
        nodes.push(row_to_node(row)?);
    }
    Ok(nodes)
}

fn row_to_node(row: Vec<Value>) -> Result<Node> {
    if row.len() < 11 {
        return Err(GitCortexError::Store(format!(
            "expected 11 columns, got {}",
            row.len()
        )));
    }
    let id_str = str_val(&row[0])?;
    let kind = kind_from_str(&str_val(&row[1])?);
    let name = str_val(&row[2])?;
    let qualified_name = str_val(&row[3])?;
    let file = PathBuf::from(str_val(&row[4])?);
    let start_line = i64_val(&row[5])? as u32;
    let end_line = i64_val(&row[6])? as u32;
    let loc = i64_val(&row[7])? as u32;
    let visibility = vis_from_str(&str_val(&row[8])?);
    let is_async = bool_val(&row[9])?;
    let is_unsafe = bool_val(&row[10])?;

    Ok(Node {
        id: NodeId::try_from(id_str.as_str())
            .map_err(|e| GitCortexError::Store(format!("bad node id: {e}")))?,
        kind,
        name,
        qualified_name,
        file,
        span: Span { start_line, end_line },
        metadata: NodeMetadata { loc, visibility, is_async, is_unsafe, ..Default::default() },
    })
}

fn collect_ids(conn: &mut Connection, table: &str) -> Result<Vec<String>> {
    let mut result = conn
        .query(&format!("MATCH (n:{table}) RETURN n.id"))
        .map_err(|e| GitCortexError::Store(e.to_string()))?;

    let mut ids = Vec::new();
    while let Some(row) = result.next() {
        ids.push(str_val(&row[0])?);
    }
    Ok(ids)
}

// ── Value extraction ──────────────────────────────────────────────────────────

fn str_val(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        other => Err(GitCortexError::Store(format!("expected String, got {other:?}"))),
    }
}

fn i64_val(v: &Value) -> Result<i64> {
    match v {
        Value::Int64(n) => Ok(*n),
        Value::Int32(n) => Ok(*n as i64),
        other => Err(GitCortexError::Store(format!("expected Int64, got {other:?}"))),
    }
}

fn bool_val(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(GitCortexError::Store(format!("expected Bool, got {other:?}"))),
    }
}

// ── Enum conversions ──────────────────────────────────────────────────────────

fn kind_from_str(s: &str) -> NodeKind {
    match s {
        "file" => NodeKind::File,
        "module" => NodeKind::Module,
        "struct" => NodeKind::Struct,
        "enum" => NodeKind::Enum,
        "trait" => NodeKind::Trait,
        "type_alias" => NodeKind::TypeAlias,
        "function" => NodeKind::Function,
        "method" => NodeKind::Method,
        "constant" => NodeKind::Constant,
        "macro" => NodeKind::Macro,
        _ => NodeKind::Function,
    }
}

fn edge_kind_from_str(s: &str) -> EdgeKind {
    match s {
        "calls" => EdgeKind::Calls,
        "implements" => EdgeKind::Implements,
        "uses" => EdgeKind::Uses,
        "imports" => EdgeKind::Imports,
        _ => EdgeKind::Contains,
    }
}

fn vis_str(v: &Visibility) -> String {
    match v {
        Visibility::Pub => "pub".into(),
        Visibility::PubCrate => "pub_crate".into(),
        Visibility::Private => "private".into(),
    }
}

fn vis_from_str(s: &str) -> Visibility {
    match s {
        "pub" => Visibility::Pub,
        "pub_crate" => Visibility::PubCrate,
        _ => Visibility::Private,
    }
}

// ── String escaping ───────────────────────────────────────────────────────────

/// Escape a string for inline use in a Cypher query.
/// Replaces `\` → `\\` and `'` → `\'`.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}
