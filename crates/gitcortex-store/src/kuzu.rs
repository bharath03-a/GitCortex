use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility, SCHEMA_VERSION},
    store::{CallersDeep, GraphStore, SubGraph, SymbolContext},
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
    ///
    /// If the persisted schema version doesn't match [`SCHEMA_VERSION`], the
    /// entire repo data directory is wiped so a fresh full index runs on next
    /// hook invocation.
    pub fn open(repo_root: &Path) -> Result<Self> {
        let repo_id = branch::repo_id(repo_root);

        if branch::read_schema_version(&repo_id) != SCHEMA_VERSION {
            eprintln!(
                "gitcortex: schema version mismatch (expected {}); wiping graph store for re-index",
                SCHEMA_VERSION
            );
            branch::wipe_repo_data(&repo_id);
            branch::write_schema_version(&repo_id, SCHEMA_VERSION)?;
        }

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

        // Transaction 1: commit all deletes first.
        // KuzuDB has a quirk where DETACH DELETE + CREATE in the same transaction
        // can produce NULL for the last STRING column in newly created nodes.
        // Splitting into separate transactions avoids this.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin delete transaction: {e}")))?;

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

        conn.query("COMMIT")
            .map_err(|e| GitCortexError::Store(format!("commit deletes: {e}")))?;

        // Transaction 2: insert new nodes. Deduplicate by ID first so a rename
        // delta (or any other case producing the same NodeId twice) never hits a
        // PK violation.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin node insert transaction: {e}")))?;

        let mut seen_node_ids: HashSet<String> = HashSet::new();
        for node in diff.added_nodes.iter().filter(|n| seen_node_ids.insert(n.id.as_str().to_owned())) {
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
            let is_static = node.metadata.is_static;
            let is_abstract = node.metadata.is_abstract;
            let is_final = node.metadata.is_final;
            let is_property = node.metadata.is_property;
            let is_generator = node.metadata.is_generator;
            let is_const = node.metadata.is_const;
            let generic_bounds = esc(&node.metadata.generic_bounds.join("|"));

            conn.query(&format!(
                "CREATE (:{nt} {{\
                    id: '{id}', kind: '{kind}', name: '{name}', \
                    qualified_name: '{qname}', file: '{file}', \
                    start_line: {sl}, end_line: {el}, loc: {loc}, \
                    visibility: '{vis}', is_async: {is_async}, is_unsafe: {is_unsafe}, \
                    is_static: {is_static}, is_abstract: {is_abstract}, is_final: {is_final}, \
                    is_property: {is_property}, is_generator: {is_generator}, is_const: {is_const}, \
                    generic_bounds: '{generic_bounds}'\
                }})"
            ))
            .map_err(|e| GitCortexError::Store(format!("insert node '{name}': {e}")))?;
        }

        // Commit node inserts so the edge MATCH queries in step 3 see them.
        conn.query("COMMIT")
            .map_err(|e| GitCortexError::Store(format!("commit nodes: {e}")))?;

        // Transaction 3: insert edges and resolve deferred references.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin edge transaction: {e}")))?;

        // 5. Insert new edges. Deduplicate by (src,dst,kind) to avoid creating
        //    parallel edges. MATCH yields nothing for missing endpoints → skip.
        let mut seen_edges: HashSet<(String, String, String)> = HashSet::new();
        for edge in diff.added_edges.iter().filter(|e| {
            seen_edges.insert((e.src.as_str().to_owned(), e.dst.as_str().to_owned(), e.kind.to_string()))
        }) {
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
                 WHERE tr.name = '{t}' AND (tr.kind = 'trait' OR tr.kind = 'interface') \
                 CREATE (st)-[:{et} {{kind: 'implements'}}]->(tr)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred impl '{trait_name}': {e}")))?;
        }

        for (subtype_id, supertype_name) in &diff.deferred_inherits {
            let s = esc(&subtype_id.as_str());
            let t = esc(supertype_name);
            conn.query(&format!(
                "MATCH (sub:{nt} {{id: '{s}'}}), (sup:{nt}) \
                 WHERE sup.name = '{t}' \
                 AND (sup.kind = 'struct' OR sup.kind = 'interface' OR sup.kind = 'trait') \
                 CREATE (sub)-[:{et} {{kind: 'inherits'}}]->(sup)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred inherits '{supertype_name}': {e}")))?;
        }

        for (method_id, exception_name) in &diff.deferred_throws {
            let m = esc(&method_id.as_str());
            let e_name = esc(exception_name);
            conn.query(&format!(
                "MATCH (m:{nt} {{id: '{m}'}}), (ex:{nt}) \
                 WHERE ex.name = '{e_name}' \
                 CREATE (m)-[:{et} {{kind: 'throws'}}]->(ex)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred throws '{exception_name}': {e}")))?;
        }

        for (target_id, annotation_name) in &diff.deferred_annotated {
            let t = esc(&target_id.as_str());
            let a = esc(annotation_name);
            conn.query(&format!(
                "MATCH (target:{nt} {{id: '{t}'}}), (ann:{nt}) \
                 WHERE ann.name = '{a}' \
                 AND (ann.kind = 'annotation' OR ann.kind = 'macro' OR ann.kind = 'function') \
                 CREATE (target)-[:{et} {{kind: 'annotated'}}]->(ann)"
            ))
            .map_err(|e| GitCortexError::Store(format!("deferred annotated '{annotation_name}': {e}")))?;
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
                "MATCH (n:{nt})-[:{et} {{kind: 'calls'}}]->(callee:{nt}) \
                 WHERE callee.name = '{name_esc}' \
                 RETURN DISTINCT {NODE_COLS}"
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
                "MATCH (caller:{nt})-[:{et} {{kind: 'calls'}}]->(n:{nt}) \
                 WHERE caller.name = '{name_esc}' \
                 RETURN {NODE_COLS}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let callees = rows_to_nodes(&mut callee_result)?;

        // Used-by — who references this symbol via Uses edges.
        let mut used_result = conn
            .query(&format!(
                "MATCH (n:{nt})-[:{et} {{kind: 'uses'}}]->(ty:{nt}) \
                 WHERE ty.name = '{name_esc}' \
                 RETURN {NODE_COLS}"
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

    fn find_callees(&self, branch: &str, function_name: &str, depth: u8) -> Result<CallersDeep> {
        let depth = depth.min(5);
        let mut hops: Vec<Vec<Node>> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut frontier: Vec<String> = vec![function_name.to_owned()];
        seen.insert(function_name.to_owned());

        for _ in 0..depth {
            if frontier.is_empty() {
                break;
            }
            let mut hop_nodes: Vec<Node> = Vec::new();
            let mut next_frontier: Vec<String> = Vec::new();
            for caller_name in &frontier {
                let nt = db_schema::node_table(branch);
                let et = db_schema::edge_table(branch);
                let name_esc = esc(caller_name);
                let conn = self.conn()?;
                let mut result = conn
                    .query(&format!(
                        "MATCH (caller:{nt})-[:{et} {{kind: 'calls'}}]->(n:{nt}) \
                         WHERE caller.name = '{name_esc}' \
                         RETURN {NODE_COLS}"
                    ))
                    .map_err(|e| GitCortexError::Store(e.to_string()))?;
                for node in rows_to_nodes(&mut result)? {
                    let id = node.id.as_str().to_owned();
                    if seen.insert(id) {
                        next_frontier.push(node.name.clone());
                        hop_nodes.push(node);
                    }
                }
            }
            hops.push(hop_nodes);
            frontier = next_frontier;
        }

        let total: usize = hops.iter().map(|h| h.len()).sum();
        let risk_level = match total {
            0..=2 => "LOW",
            3..=10 => "MEDIUM",
            11..=30 => "HIGH",
            _ => "CRITICAL",
        };
        Ok(CallersDeep { hops, risk_level })
    }

    fn find_implementors(&self, branch: &str, trait_or_interface_name: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(trait_or_interface_name);
        let conn = self.conn()?;

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt})-[:{et}]->(trait_node:{nt}) \
                 WHERE trait_node.name = '{name_esc}' \
                 AND (e.kind = 'implements' OR e.kind = 'inherits') \
                 RETURN {NODE_COLS}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        // Fallback: try without aliasing the edge (KuzuQL requires the edge alias for filtering)
        if result.by_ref().count() == 0 {
            let conn2 = self.conn()?;
            let mut r2 = conn2
                .query(&format!(
                    "MATCH (n:{nt})-[e:{et}]->(trait_node:{nt}) \
                     WHERE trait_node.name = '{name_esc}' \
                     AND (e.kind = 'implements' OR e.kind = 'inherits') \
                     RETURN {NODE_COLS}"
                ))
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            return rows_to_nodes(&mut r2);
        }
        // Re-query since we consumed the iterator
        let conn3 = self.conn()?;
        let mut r3 = conn3
            .query(&format!(
                "MATCH (n:{nt})-[e:{et}]->(trait_node:{nt}) \
                 WHERE trait_node.name = '{name_esc}' \
                 AND (e.kind = 'implements' OR e.kind = 'inherits') \
                 RETURN {NODE_COLS}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut r3)
    }

    fn trace_path(&self, branch: &str, from: &str, to: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);

        // BFS from `from` to `to` following Calls edges.
        let from_esc = esc(from);
        let conn = self.conn()?;
        let mut start_result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.name = '{from_esc}' RETURN {NODE_COLS} LIMIT 1"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let start_nodes = rows_to_nodes(&mut start_result)?;
        if start_nodes.is_empty() {
            return Ok(Vec::new());
        }

        // BFS: queue of (current_name, path_so_far)
        let mut queue: std::collections::VecDeque<(String, Vec<String>)> = std::collections::VecDeque::new();
        queue.push_back((from.to_owned(), vec![from.to_owned()]));
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(from.to_owned());

        const MAX_HOPS: usize = 6;
        while let Some((current, path)) = queue.pop_front() {
            if path.len() > MAX_HOPS {
                continue;
            }
            let cur_esc = esc(&current);
            let conn2 = self.conn()?;
            let mut callee_result = conn2
                .query(&format!(
                    "MATCH (caller:{nt})-[:{et} {{kind: 'calls'}}]->(n:{nt}) \
                     WHERE caller.name = '{cur_esc}' \
                     RETURN {NODE_COLS}"
                ))
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            for node in rows_to_nodes(&mut callee_result)? {
                let node_name = node.name.clone();
                if node_name == to {
                    // Found — resolve full path names to nodes
                    let mut result_nodes = Vec::new();
                    for name in &path {
                        let conn3 = self.conn()?;
                        let n_esc = esc(name);
                        let mut r = conn3
                            .query(&format!(
                                "MATCH (n:{nt}) WHERE n.name = '{n_esc}' RETURN {NODE_COLS} LIMIT 1"
                            ))
                            .map_err(|e| GitCortexError::Store(e.to_string()))?;
                        result_nodes.extend(rows_to_nodes(&mut r)?);
                    }
                    result_nodes.push(node);
                    return Ok(result_nodes);
                }
                if visited.insert(node_name.clone()) {
                    let mut new_path = path.clone();
                    new_path.push(node_name.clone());
                    queue.push_back((node_name, new_path));
                }
            }
        }
        Ok(Vec::new())
    }

    fn list_symbols_in_range(
        &self,
        branch: &str,
        file: &Path,
        start_line: u32,
        end_line: u32,
    ) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let file_esc = esc(file.to_string_lossy().as_ref());
        let conn = self.conn()?;

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) \
                 WHERE n.file = '{file_esc}' \
                 AND n.start_line <= {end_line} \
                 AND n.end_line >= {start_line} \
                 RETURN {NODE_COLS} ORDER BY n.start_line"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        rows_to_nodes(&mut result)
    }

    fn find_unused_symbols(&self, branch: &str, kind: Option<NodeKind>) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let conn = self.conn()?;

        let kind_filter = match &kind {
            Some(k) => format!("AND n.kind = '{k}'"),
            None => String::new(),
        };

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) \
                 WHERE NOT EXISTS {{ MATCH (:{nt})-[:{et} {{kind: 'calls'}}]->(n) }} \
                 AND NOT EXISTS {{ MATCH (:{nt})-[:{et} {{kind: 'uses'}}]->(n) }} \
                 AND n.kind <> 'file' AND n.kind <> 'folder' AND n.kind <> 'module' \
                 {kind_filter} \
                 RETURN {NODE_COLS} ORDER BY n.file, n.start_line"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        rows_to_nodes(&mut result)
    }

    fn get_subgraph(
        &self,
        branch: &str,
        seed_name: &str,
        depth: u8,
        direction: &str,
    ) -> Result<SubGraph> {
        self.ensure_branch(branch)?;
        let depth = depth.min(5);
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);

        let seed_esc = esc(seed_name);
        let conn = self.conn()?;
        let mut seed_result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.name = '{seed_esc}' RETURN {NODE_COLS} LIMIT 1"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let seed_nodes = rows_to_nodes(&mut seed_result)?;
        if seed_nodes.is_empty() {
            return Ok(SubGraph { nodes: Vec::new(), edges: Vec::new() });
        }

        let mut all_node_ids: HashSet<String> = HashSet::new();
        let mut all_nodes: Vec<Node> = Vec::new();
        let mut frontier_names: Vec<String> = vec![seed_name.to_owned()];

        for node in seed_nodes {
            all_node_ids.insert(node.id.as_str().to_owned());
            all_nodes.push(node);
        }

        for _ in 0..depth {
            let mut next_frontier: Vec<String> = Vec::new();
            for name in &frontier_names {
                let name_esc = esc(name);
                // Outbound (callees): what this node calls
                if direction == "out" || direction == "both" {
                    let conn2 = self.conn()?;
                    let mut r = conn2
                        .query(&format!(
                            "MATCH (caller:{nt})-[:{et}]->(n:{nt}) \
                             WHERE caller.name = '{name_esc}' \
                             RETURN {NODE_COLS}"
                        ))
                        .map_err(|e| GitCortexError::Store(e.to_string()))?;
                    for node in rows_to_nodes(&mut r)? {
                        let id = node.id.as_str().to_owned();
                        if all_node_ids.insert(id) {
                            next_frontier.push(node.name.clone());
                            all_nodes.push(node);
                        }
                    }
                }
                // Inbound (callers): what calls this node
                if direction == "in" || direction == "both" {
                    let conn3 = self.conn()?;
                    let mut r = conn3
                        .query(&format!(
                            "MATCH (n:{nt})-[:{et}]->(target:{nt}) \
                             WHERE target.name = '{name_esc}' \
                             RETURN {NODE_COLS}"
                        ))
                        .map_err(|e| GitCortexError::Store(e.to_string()))?;
                    for node in rows_to_nodes(&mut r)? {
                        let id = node.id.as_str().to_owned();
                        if all_node_ids.insert(id) {
                            next_frontier.push(node.name.clone());
                            all_nodes.push(node);
                        }
                    }
                }
            }
            if next_frontier.is_empty() {
                break;
            }
            frontier_names = next_frontier;
        }

        // Collect edges between the nodes in the subgraph
        let ids_list: Vec<String> = all_node_ids.iter().map(|id| format!("'{}'", esc(id))).collect();
        let ids_str = ids_list.join(", ");
        let all_edges = if ids_list.is_empty() {
            Vec::new()
        } else {
            let conn4 = self.conn()?;
            let result = conn4
                .query(&format!(
                    "MATCH (s:{nt})-[e:{et}]->(d:{nt}) \
                     WHERE s.id IN [{ids_str}] AND d.id IN [{ids_str}] \
                     RETURN s.id, d.id, e.kind"
                ))
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            let mut edges = Vec::new();
            for row in result {
                let src_str = str_val(&row[0])?;
                let dst_str = str_val(&row[1])?;
                let kind_str = str_val(&row[2])?;
                edges.push(Edge {
                    src: NodeId::try_from(src_str.as_str())
                        .map_err(|e| GitCortexError::Store(format!("bad src id: {e}")))?,
                    dst: NodeId::try_from(dst_str.as_str())
                        .map_err(|e| GitCortexError::Store(format!("bad dst id: {e}")))?,
                    kind: edge_kind_from_str(&kind_str),
                });
            }
            edges
        };

        Ok(SubGraph { nodes: all_nodes, edges: all_edges })
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
     n.start_line, n.end_line, n.loc, n.visibility, n.is_async, n.is_unsafe, \
     n.is_static, n.is_abstract, n.is_final, n.is_property, n.is_generator, n.is_const, \
     n.generic_bounds";

fn rows_to_nodes(result: &mut kuzu::QueryResult) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for row in result.by_ref() {
        match row_to_node(row) {
            Ok(n) => nodes.push(n),
            Err(e) => tracing::debug!("skipping malformed node row: {e}"),
        }
    }
    Ok(nodes)
}

fn row_to_node(row: Vec<Value>) -> Result<Node> {
    if row.len() < 18 {
        return Err(GitCortexError::Store(format!(
            "expected 18 columns, got {}",
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
    let is_static = bool_val(&row[11])?;
    let is_abstract = bool_val(&row[12])?;
    let is_final = bool_val(&row[13])?;
    let is_property = bool_val(&row[14])?;
    let is_generator = bool_val(&row[15])?;
    let is_const = bool_val(&row[16])?;
    let generic_bounds_str = str_val(&row[17])?;
    let generic_bounds: Vec<String> = if generic_bounds_str.is_empty() {
        Vec::new()
    } else {
        generic_bounds_str.split('|').map(String::from).collect()
    };

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
            is_static,
            is_abstract,
            is_final,
            is_property,
            is_generator,
            is_const,
            generic_bounds,
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
        // KuzuDB returns Null(String) for empty-string columns inserted after a
        // DETACH DELETE in a prior transaction. Treat as empty string.
        Value::Null(_) => Ok(String::new()),
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
        // Null booleans arise from legacy rows written before the column existed;
        // treat them as false rather than failing the whole query.
        Value::Null(_) => Ok(false),
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
        "interface" => NodeKind::Interface,
        "type_alias" => NodeKind::TypeAlias,
        "function" => NodeKind::Function,
        "method" => NodeKind::Method,
        "property" => NodeKind::Property,
        "constant" => NodeKind::Constant,
        "macro" => NodeKind::Macro,
        "annotation" => NodeKind::Annotation,
        "enum_member" => NodeKind::EnumMember,
        _ => NodeKind::Function,
    }
}

fn edge_kind_from_str(s: &str) -> EdgeKind {
    match s {
        "calls" => EdgeKind::Calls,
        "implements" => EdgeKind::Implements,
        "inherits" => EdgeKind::Inherits,
        "uses" => EdgeKind::Uses,
        "imports" => EdgeKind::Imports,
        "annotated" => EdgeKind::Annotated,
        "throws" => EdgeKind::Throws,
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
