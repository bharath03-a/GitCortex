use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility},
    store::{CallersDeep, GraphStore, SymbolContext},
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

        // Use explicit transactions so Phase 1 (node inserts) is committed and
        // visible before Phase 2 (edge MATCHes) begins — required for KuzuDB's
        // MVCC snapshot isolation to work correctly.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin transaction: {e}")))?;

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

        // Commit node inserts so the edge MATCH queries in steps 5–6 see them.
        conn.query("COMMIT")
            .map_err(|e| GitCortexError::Store(format!("commit nodes: {e}")))?;

        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin edge transaction: {e}")))?;

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

        // 6. Resolve cross-file deferred edges against the full store.
        //    The diff-local pass couldn't find these callees/types because they
        //    live in unchanged files. We match by name here — best-effort without
        //    full type inference, filtered to the correct node kinds to reduce noise.

        for (caller_id, callee_name) in &diff.deferred_calls {
            let caller = esc(&caller_id.as_str());
            let callee = esc(callee_name);
            conn.query(&format!(
                "MATCH (caller:{nt} {{id: '{caller}'}}), (callee:{nt}) \
                 WHERE callee.name = '{callee}' \
                 AND (callee.kind = 'function' OR callee.kind = 'method') \
                 CREATE (caller)-[:{et} {{kind: 'calls'}}]->(callee)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred call '{callee_name}': {e}")))?;
        }

        for (fn_id, type_name) in &diff.deferred_uses {
            let fn_esc = esc(&fn_id.as_str());
            let ty = esc(type_name);
            conn.query(&format!(
                "MATCH (fn_node:{nt} {{id: '{fn_esc}'}}), (ty:{nt}) \
                 WHERE ty.name = '{ty}' \
                 AND (ty.kind = 'struct' OR ty.kind = 'enum' \
                      OR ty.kind = 'trait' OR ty.kind = 'type_alias') \
                 CREATE (fn_node)-[:{et} {{kind: 'uses'}}]->(ty)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred use '{type_name}': {e}")))?;
        }

        for (struct_id, trait_name) in &diff.deferred_implements {
            let s = esc(&struct_id.as_str());
            let t = esc(trait_name);
            conn.query(&format!(
                "MATCH (st:{nt} {{id: '{s}'}}), (tr:{nt}) \
                 WHERE tr.name = '{t}' AND tr.kind = 'trait' \
                 CREATE (st)-[:{et} {{kind: 'implements'}}]->(tr)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred impl '{trait_name}': {e}")))?;
        }

        conn.query("COMMIT")
            .map_err(|e| GitCortexError::Store(format!("commit edges: {e}")))?;

        Ok(())
    }

    // ── Read path ─────────────────────────────────────────────────────────────

    fn lookup_symbol(&self, branch: &str, name: &str, fuzzy: bool) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let name_esc = esc(name);
        let conn = self.conn()?;

        let condition = if fuzzy {
            format!("contains(n.name, '{name_esc}')")
        } else {
            format!("n.name = '{name_esc}'")
        };

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE {condition} RETURN {NODE_COLS}"
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

    fn find_callers_deep(
        &self,
        branch: &str,
        function_name: &str,
        depth: u8,
    ) -> Result<CallersDeep> {
        let depth = depth.min(5);
        let mut hops: Vec<Vec<Node>> = Vec::new();
        // Track seen node IDs to avoid cycles.
        let mut seen: HashSet<String> = HashSet::new();
        // The frontier holds the *names* of nodes whose callers we search next.
        let mut frontier: Vec<String> = vec![function_name.to_owned()];
        seen.insert(function_name.to_owned());

        for _ in 0..depth {
            if frontier.is_empty() {
                break;
            }
            let mut hop_nodes: Vec<Node> = Vec::new();
            let mut next_frontier: Vec<String> = Vec::new();
            for target in &frontier {
                for caller in self.find_callers(branch, target)? {
                    let id = caller.id.as_str().to_owned();
                    if seen.insert(id) {
                        next_frontier.push(caller.name.clone());
                        hop_nodes.push(caller);
                    }
                }
            }
            hops.push(hop_nodes);
            frontier = next_frontier;
        }

        let total_affected: usize = hops.iter().map(|h| h.len()).sum();
        let risk_level = match total_affected {
            0..=2 => "LOW",
            3..=10 => "MEDIUM",
            11..=30 => "HIGH",
            _ => "CRITICAL",
        };

        Ok(CallersDeep { hops, risk_level })
    }

    fn symbol_context(&self, branch: &str, name: &str) -> Result<SymbolContext> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(name);
        let conn = self.conn()?;

        // Definition — first match.
        let mut def_result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.name = '{name_esc}' RETURN {NODE_COLS} LIMIT 1"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let mut defs = rows_to_nodes(&mut def_result)?;
        if defs.is_empty() {
            return Err(GitCortexError::Store(format!(
                "symbol '{name}' not found on branch '{branch}'"
            )));
        }
        let definition = defs.remove(0);

        // Callers — who calls this symbol.
        let callers = self.find_callers(branch, name)?;

        // Callees — what this symbol calls.
        let mut callee_result = conn
            .query(&format!(
                "MATCH (caller:{nt})-[:{et} {{kind: 'calls'}}]->(callee:{nt}) \
                 WHERE caller.name = '{name_esc}' \
                 RETURN callee.id, callee.kind, callee.name, callee.qualified_name, \
                        callee.file, callee.start_line, callee.end_line, callee.loc, \
                        callee.visibility, callee.is_async, callee.is_unsafe"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let callees = rows_to_nodes(&mut callee_result)?;

        // Used-by — who references this symbol via Uses edges.
        let mut used_result = conn
            .query(&format!(
                "MATCH (fn:{nt})-[:{et} {{kind: 'uses'}}]->(ty:{nt}) \
                 WHERE ty.name = '{name_esc}' \
                 RETURN fn.id, fn.kind, fn.name, fn.qualified_name, \
                        fn.file, fn.start_line, fn.end_line, fn.loc, \
                        fn.visibility, fn.is_async, fn.is_unsafe"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let used_by = rows_to_nodes(&mut used_result)?;

        Ok(SymbolContext {
            definition,
            callers,
            callees,
            used_by,
        })
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
        let added_ids: Vec<&String> = to_ids.iter().filter(|id| !from_ids.contains(*id)).collect();

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
        let result = conn
            .query(&format!(
                "MATCH (s:{nt})-[e:{et}]->(d:{nt}) RETURN s.id, d.id, e.kind"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        let mut out = Vec::new();
        for row in result {
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
const NODE_COLS: &str = "n.id, n.kind, n.name, n.qualified_name, n.file, \
     n.start_line, n.end_line, n.loc, n.visibility, n.is_async, n.is_unsafe";

fn rows_to_nodes(result: &mut kuzu::QueryResult) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for row in result.by_ref() {
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
        span: Span {
            start_line,
            end_line,
        },
        metadata: NodeMetadata {
            loc,
            visibility,
            is_async,
            is_unsafe,
            ..Default::default()
        },
    })
}

fn collect_ids(conn: &mut Connection, table: &str) -> Result<Vec<String>> {
    let result = conn
        .query(&format!("MATCH (n:{table}) RETURN n.id"))
        .map_err(|e| GitCortexError::Store(e.to_string()))?;

    let mut ids = Vec::new();
    for row in result {
        ids.push(str_val(&row[0])?);
    }
    Ok(ids)
}

// ── Value extraction ──────────────────────────────────────────────────────────

fn str_val(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        other => Err(GitCortexError::Store(format!(
            "expected String, got {other:?}"
        ))),
    }
}

fn i64_val(v: &Value) -> Result<i64> {
    match v {
        Value::Int64(n) => Ok(*n),
        Value::Int32(n) => Ok(*n as i64),
        other => Err(GitCortexError::Store(format!(
            "expected Int64, got {other:?}"
        ))),
    }
}

fn bool_val(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(GitCortexError::Store(format!(
            "expected Bool, got {other:?}"
        ))),
    }
}

// ── Enum conversions ──────────────────────────────────────────────────────────

fn kind_from_str(s: &str) -> NodeKind {
    match s {
        "folder" => NodeKind::Folder,
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
