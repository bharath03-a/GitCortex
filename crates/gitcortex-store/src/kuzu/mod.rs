use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node, NodeId},
    schema::{NodeKind, SCHEMA_VERSION},
    store::{
        AttributeFilter, CallSite, CallersDeep, GraphStats, GraphStore, SubGraph, SymbolContext,
        TypeHierarchy,
    },
};
use kuzu::{Connection, Database, SystemConfig};

use crate::{branch, schema as db_schema};

mod bulk;
mod conv;
mod escape;
mod queries;
mod values;

use conv::{edge_kind_from_str, lang_scope_clause, vis_str};
use escape::{esc, esc_multiline};
use queries::{collect_ids, rows_to_nodes, NODE_COLS, SYMBOL_RANK};
use values::{i64_val, str_val};

// Batch sizes for `UNWIND`-based inserts. Nodes carry a (≤16 KB) def_body, so
// their chunk is kept small to bound query-string size; edges are three ids
// each, so they batch much larger.
const NODE_INSERT_CHUNK: usize = 128;
const EDGE_INSERT_CHUNK: usize = 1000;

/// Render a `Node` as a Cypher struct literal `{id:'…', kind:'…', …}` for use
/// inside an `UNWIND [...] AS r CREATE` batch. String fields are escaped and
/// single-quoted; bools/ints are emitted bare.
fn node_struct_literal(node: &Node) -> String {
    let id = esc(&node.id.as_str());
    let kind = esc(&node.kind.to_string());
    let name = esc(&node.name);
    let qname = esc(&node.qualified_name);
    let file = esc(node.file.to_string_lossy().as_ref());
    let sl = node.span.start_line as i64;
    let el = node.span.end_line as i64;
    let loc = node.metadata.loc as i64;
    let vis = esc(&vis_str(&node.metadata.visibility));
    let m = &node.metadata;
    let generic_bounds = esc(&m.generic_bounds.join("|"));
    let annotations = esc(&m.annotations.join("|"));
    let def_sig = esc_multiline(&m.definition.signature);
    let def_body = esc_multiline(&m.definition.body);
    let def_doc = esc_multiline(m.definition.doc_comment.as_deref().unwrap_or(""));
    let def_start_byte = m.definition.start_byte as i64;
    let def_end_byte = m.definition.end_byte as i64;
    let complexity = match m.lld.complexity {
        Some(c) => c as i64,
        None => -1i64,
    };

    format!(
        "{{id:'{id}', kind:'{kind}', name:'{name}', qualified_name:'{qname}', file:'{file}', \
         start_line:{sl}, end_line:{el}, loc:{loc}, visibility:'{vis}', \
         is_async:{ia}, is_unsafe:{iu}, is_static:{ist}, is_abstract:{iab}, is_final:{ifi}, \
         is_property:{ip}, is_generator:{ig}, is_const:{ic}, generic_bounds:'{generic_bounds}', \
         def_signature:'{def_sig}', def_body:'{def_body}', def_doc:'{def_doc}', \
         def_start_byte:{def_start_byte}, def_end_byte:{def_end_byte}, \
         complexity:{complexity}, annotations:'{annotations}'}}",
        ia = m.is_async,
        iu = m.is_unsafe,
        ist = m.is_static,
        iab = m.is_abstract,
        ifi = m.is_final,
        ip = m.is_property,
        ig = m.is_generator,
        ic = m.is_const,
    )
}

/// True when the branch's node table has zero rows (fresh / never indexed).
fn node_table_is_empty(conn: &Connection, nt: &str) -> Result<bool> {
    let mut r = conn
        .query(&format!("MATCH (n:{nt}) RETURN count(n) AS c LIMIT 1"))
        .map_err(|e| GitCortexError::Store(format!("count nodes: {e}")))?;
    match r.by_ref().next() {
        Some(row) => match &row[0] {
            kuzu::Value::Int64(n) => Ok(*n == 0),
            _ => Ok(false),
        },
        None => Ok(true),
    }
}

/// Bulk-load a full-index diff via CSV `COPY`. Stages CSVs in a unique temp
/// dir, loads them, then removes the dir. See [`bulk`] for the rationale.
fn bulk_apply(conn: &Connection, nt: &str, et: &str, diff: &GraphDiff) -> Result<()> {
    // Unique staging dir per call: pid + nanos + a process-wide atomic counter,
    // so concurrent `apply_diff`s (e.g. parallel tests in one binary) never
    // share a directory.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let stage = std::env::temp_dir().join(format!(
        "gcx-bulk-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        SEQ.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&stage)
        .map_err(|e| GitCortexError::Store(format!("create staging dir: {e}")))?;

    let result = bulk::bulk_load(conn, nt, et, &stage, &diff.added_nodes, &diff.added_edges);

    // Best-effort cleanup regardless of load outcome.
    let _ = std::fs::remove_dir_all(&stage);

    result.map(|_| ())
}

const DEFERRED_CHUNK: usize = 500;

/// Resolve a batch of deferred cross-file edges via one UNWIND query per
/// language-scope group instead of one query per pair.
///
/// Pairs are grouped by the caller's language family so the scope clause is
/// uniform across all rows in a chunk. Each group is split into chunks of at
/// most [`DEFERRED_CHUNK`] pairs to keep query strings bounded.
fn resolve_deferred_batch(
    conn: &Connection,
    nt: &str,
    et: &str,
    pairs: &[(NodeId, String)],
    caller_file: &HashMap<String, String>,
    edge_kind: &str,
    kind_filter: &str,
) -> Result<()> {
    if pairs.is_empty() {
        return Ok(());
    }
    let mut by_scope: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (src_id, tgt_name) in pairs {
        let src_str = src_id.as_str();
        let scope = caller_file
            .get(src_str.as_str())
            .map(|f| lang_scope_clause(f, "tgt"))
            .unwrap_or_default();
        by_scope
            .entry(scope)
            .or_default()
            .push((src_str, tgt_name.clone()));
    }
    for (scope_clause, group) in &by_scope {
        for chunk in group.chunks(DEFERRED_CHUNK) {
            let list = chunk
                .iter()
                .map(|(src, tgt)| format!("{{s:'{}',t:'{}'}}", esc(src), esc(tgt)))
                .collect::<Vec<_>>()
                .join(",");
            let kind_and = if kind_filter.is_empty() {
                String::new()
            } else {
                format!(" AND ({kind_filter})")
            };
            conn.query(&format!(
                "UNWIND [{list}] AS r \
                 MATCH (src:{nt} {{id: r.s}}), (tgt:{nt}) \
                 WHERE tgt.name = r.t{kind_and}{scope_clause} \
                 CREATE (src)-[:{et} {{kind: '{edge_kind}'}}]->(tgt)"
            ))
            .map_err(|e| GitCortexError::Store(format!("batch deferred {edge_kind}: {e}")))?;
        }
    }
    Ok(())
}

/// Like [`resolve_deferred_batch`] but for `Calls` edges, carrying each call's
/// source line onto the created edge. Tuples are `(caller_id, callee_name, line)`.
fn resolve_calls_batch(
    conn: &Connection,
    nt: &str,
    et: &str,
    triples: &[(NodeId, String, u32)],
    caller_file: &HashMap<String, String>,
) -> Result<()> {
    if triples.is_empty() {
        return Ok(());
    }
    let mut by_scope: HashMap<String, Vec<(String, String, u32)>> = HashMap::new();
    for (src_id, tgt_name, line) in triples {
        let src_str = src_id.as_str();
        let scope = caller_file
            .get(src_str.as_str())
            .map(|f| lang_scope_clause(f, "tgt"))
            .unwrap_or_default();
        by_scope
            .entry(scope)
            .or_default()
            .push((src_str, tgt_name.clone(), *line));
    }
    for (scope_clause, group) in &by_scope {
        for chunk in group.chunks(DEFERRED_CHUNK) {
            let list = chunk
                .iter()
                .map(|(src, tgt, line)| {
                    format!("{{s:'{}',t:'{}',ln:{}}}", esc(src), esc(tgt), line)
                })
                .collect::<Vec<_>>()
                .join(",");
            conn.query(&format!(
                "UNWIND [{list}] AS r \
                 MATCH (src:{nt} {{id: r.s}}), (tgt:{nt}) \
                 WHERE tgt.name = r.t AND (tgt.kind = 'function' OR tgt.kind = 'method'){scope_clause} \
                 CREATE (src)-[:{et} {{kind: 'calls', line: r.ln}}]->(tgt)"
            ))
            .map_err(|e| GitCortexError::Store(format!("batch deferred calls: {e}")))?;
        }
    }
    Ok(())
}

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

        // ── Fast path: bulk COPY load for a fresh full index ───────────────────
        // When the branch's node table is empty this is a first full index.
        // Stage the nodes/edges as CSV and `COPY` them in — ~100× faster than
        // per-row MATCH/CREATE on large repos.
        //
        // The diff's `removed_*` fields are ignored on this path: the indexer
        // emits a `removed_files` entry for every parsed file + its ancestor
        // folders (so an incremental re-parse first clears the old nodes), but
        // against an empty table those deletes are vacuous. Deferred cross-file
        // resolution is likewise skipped — on a full index every in-repo name
        // is already in `added_edges`; the only `deferred_*` left are external
        // (stdlib) names the store couldn't resolve anyway.
        let empty = node_table_is_empty(&conn, &nt)?;
        if std::env::var_os("GCX_TIMING").is_some() {
            eprintln!(
                "[gcx-timing] apply_diff path: table_empty={empty} nodes={} edges={}",
                diff.added_nodes.len(),
                diff.added_edges.len()
            );
        }
        if empty {
            return bulk_apply(&conn, &nt, &et, diff);
        }

        // Transaction 1: commit all deletes first.
        // KuzuDB has a quirk where DETACH DELETE + CREATE in the same transaction
        // can produce NULL for the last STRING column in newly created nodes.
        // Splitting into separate transactions avoids this.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin delete transaction: {e}")))?;

        // 1. Remove nodes for deleted/replaced files.
        //    Skip directory paths (no extension) — folder nodes are reused across
        //    incremental updates to preserve their Contains edges to sibling files.
        for file in &diff.removed_files {
            if file.extension().is_none() {
                continue;
            }
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

        // Build a remap table: for each Folder node in the diff, if a folder at
        // that path already exists in the DB, reuse its ID so that existing
        // Contains edges to sibling files are preserved.
        // One batch query instead of one query per folder.
        let mut id_remap: HashMap<String, String> = HashMap::new();
        let folder_nodes: Vec<&Node> = diff
            .added_nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Folder)
            .collect();
        if !folder_nodes.is_empty() {
            let path_list = folder_nodes
                .iter()
                .map(|n| format!("'{}'", esc(n.file.to_string_lossy().as_ref())))
                .collect::<Vec<_>>()
                .join(", ");
            let mut rows = conn
                .query(&format!(
                    "MATCH (n:{nt}) WHERE n.file IN [{path_list}] AND n.kind = 'folder' \
                     RETURN n.file, n.id"
                ))
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            let mut existing_by_path: HashMap<String, String> = HashMap::new();
            for row in rows.by_ref() {
                if let (Ok(file), Ok(id)) = (str_val(&row[0]), str_val(&row[1])) {
                    existing_by_path.insert(file, id);
                }
            }
            for node in &folder_nodes {
                let path_str = node.file.to_string_lossy().into_owned();
                if let Some(existing_id) = existing_by_path.get(&path_str) {
                    tracing::debug!("folder remap: {} → {}", node.file.display(), existing_id);
                    id_remap.insert(node.id.as_str().to_owned(), existing_id.clone());
                }
            }
        }

        // Transaction 2: insert new nodes. Deduplicate by ID first so a rename
        // delta (or any other case producing the same NodeId twice) never hits a
        // PK violation. Folder nodes remapped to existing DB nodes are skipped.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin node insert transaction: {e}")))?;

        // Batch node inserts via `UNWIND [<struct>, …] CREATE`. One query per
        // chunk instead of one per node — a ~100× cut in round-trips on a full
        // index of a large repo. Chunk size is kept modest because each row
        // carries the (truncated) def_body, so a chunk can still be a few MB.
        let mut seen_node_ids: HashSet<String> = HashSet::new();
        let rows: Vec<String> = diff
            .added_nodes
            .iter()
            .filter(|n| seen_node_ids.insert(n.id.as_str().to_owned()))
            // Folder node remapped to an existing DB node — skip INSERT.
            .filter(|n| !id_remap.contains_key(&n.id.as_str()))
            .map(node_struct_literal)
            .collect();

        for chunk in rows.chunks(NODE_INSERT_CHUNK) {
            let list = chunk.join(", ");
            conn.query(&format!(
                "UNWIND [{list}] AS r \
                 CREATE (:{nt} {{\
                    id: r.id, kind: r.kind, name: r.name, \
                    qualified_name: r.qualified_name, file: r.file, \
                    start_line: r.start_line, end_line: r.end_line, loc: r.loc, \
                    visibility: r.visibility, is_async: r.is_async, is_unsafe: r.is_unsafe, \
                    is_static: r.is_static, is_abstract: r.is_abstract, is_final: r.is_final, \
                    is_property: r.is_property, is_generator: r.is_generator, is_const: r.is_const, \
                    generic_bounds: r.generic_bounds, \
                    def_signature: r.def_signature, def_body: r.def_body, def_doc: r.def_doc, \
                    def_start_byte: r.def_start_byte, def_end_byte: r.def_end_byte, \
                    complexity: r.complexity, annotations: r.annotations\
                 }})"
            ))
            .map_err(|e| GitCortexError::Store(format!("batch insert nodes: {e}")))?;
        }

        // Commit node inserts so the edge MATCH queries in step 3 see them.
        conn.query("COMMIT")
            .map_err(|e| GitCortexError::Store(format!("commit nodes: {e}")))?;

        // Transaction 3: insert edges and resolve deferred references.
        conn.query("BEGIN TRANSACTION")
            .map_err(|e| GitCortexError::Store(format!("begin edge transaction: {e}")))?;

        // 4. Insert new edges. Deduplicate by (src,dst,kind) to avoid creating
        //    parallel edges. Remap folder IDs to existing DB nodes where applicable.
        //    MATCH yields nothing for missing endpoints → skip silently.
        let mut seen_edges: HashSet<(String, String, String)> = HashSet::new();
        let edge_rows: Vec<String> = diff
            .added_edges
            .iter()
            .filter(|e| {
                seen_edges.insert((
                    e.src.as_str().to_owned(),
                    e.dst.as_str().to_owned(),
                    e.kind.to_string(),
                ))
            })
            .map(|edge| {
                let src_raw = edge.src.as_str();
                let dst_raw = edge.dst.as_str();
                let s = esc(id_remap
                    .get(&src_raw)
                    .map(String::as_str)
                    .unwrap_or(&src_raw));
                let d = esc(id_remap
                    .get(&dst_raw)
                    .map(String::as_str)
                    .unwrap_or(&dst_raw));
                let k = esc(&edge.kind.to_string());
                let line = edge.line.map(|l| l as i64).unwrap_or(-1);
                format!("{{s:'{s}', d:'{d}', k:'{k}', ln:{line}}}")
            })
            .collect();

        // Batch edge inserts via `UNWIND … MATCH … CREATE`. Edge rows are tiny
        // (three ids), so a larger chunk than nodes is fine. Endpoints missing
        // from the store yield no MATCH row and are skipped silently — same
        // semantics as the per-edge version.
        for chunk in edge_rows.chunks(EDGE_INSERT_CHUNK) {
            let list = chunk.join(", ");
            conn.query(&format!(
                "UNWIND [{list}] AS r \
                 MATCH (s:{nt} {{id: r.s}}), (d:{nt} {{id: r.d}}) \
                 CREATE (s)-[:{et} {{kind: r.k, line: r.ln}}]->(d)"
            ))
            .map_err(|e| GitCortexError::Store(format!("batch insert edges: {e}")))?;
        }

        // 6. Resolve cross-file deferred edges against the full store.
        //    The diff-local pass couldn't find these callees/types because they
        //    live in unchanged files. Batched by language scope: one UNWIND query
        //    per language per edge kind instead of one query per pair.
        let caller_file: HashMap<String, String> = diff
            .added_nodes
            .iter()
            .map(|n| {
                (
                    n.id.as_str().to_owned(),
                    n.file.to_string_lossy().into_owned(),
                )
            })
            .collect();

        resolve_calls_batch(&conn, &nt, &et, &diff.deferred_calls, &caller_file)?;
        resolve_deferred_batch(
            &conn,
            &nt,
            &et,
            &diff.deferred_uses,
            &caller_file,
            "uses",
            "tgt.kind = 'struct' OR tgt.kind = 'enum' OR tgt.kind = 'trait' \
             OR tgt.kind = 'interface' OR tgt.kind = 'type_alias'",
        )?;
        resolve_deferred_batch(
            &conn,
            &nt,
            &et,
            &diff.deferred_implements,
            &caller_file,
            "implements",
            "tgt.kind = 'trait' OR tgt.kind = 'interface'",
        )?;
        resolve_deferred_batch(
            &conn,
            &nt,
            &et,
            &diff.deferred_inherits,
            &caller_file,
            "inherits",
            "tgt.kind = 'struct' OR tgt.kind = 'interface' OR tgt.kind = 'trait'",
        )?;
        resolve_deferred_batch(
            &conn,
            &nt,
            &et,
            &diff.deferred_throws,
            &caller_file,
            "throws",
            "",
        )?;
        resolve_deferred_batch(
            &conn,
            &nt,
            &et,
            &diff.deferred_annotated,
            &caller_file,
            "annotated",
            "tgt.kind = 'annotation' OR tgt.kind = 'macro' OR tgt.kind = 'function'",
        )?;

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
                "MATCH (n:{nt}) WHERE {condition} RETURN {NODE_COLS} ORDER BY {SYMBOL_RANK}"
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

        // Definition — best match by kind priority (type decl > fn/method >
        // … > module/file), so `wiki Echo` resolves to `type Echo` not a
        // same-named method.
        let mut def_result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.name = '{name_esc}' \
                 RETURN {NODE_COLS} ORDER BY {SYMBOL_RANK} LIMIT 1"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let mut defs = rows_to_nodes(&mut def_result)?;
        if defs.is_empty() {
            return Err(GitCortexError::Store(format!(
                "symbol '{name}' not found on branch '{branch}'"
            )));
        }
        let definition = defs.remove(0);

        // Scope callers/callees/used-by to THIS specific definition (by id),
        // not by name. Otherwise a Java `welcome` would pull in callees from
        // a Python `welcome` that happens to share the name. `find_callers`
        // as a standalone tool remains name-based — callers without a specific
        // definition node have no other handle.
        let def_id = esc(&definition.id.as_str());

        let mut caller_result = conn
            .query(&format!(
                "MATCH (n:{nt})-[:{et} {{kind: 'calls'}}]->(callee:{nt}) \
                 WHERE callee.id = '{def_id}' \
                 RETURN DISTINCT {NODE_COLS}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let callers = rows_to_nodes(&mut caller_result)?;

        let mut callee_result = conn
            .query(&format!(
                "MATCH (caller:{nt})-[:{et} {{kind: 'calls'}}]->(n:{nt}) \
                 WHERE caller.id = '{def_id}' \
                 RETURN {NODE_COLS}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let callees = rows_to_nodes(&mut callee_result)?;

        let mut used_result = conn
            .query(&format!(
                "MATCH (n:{nt})-[:{et} {{kind: 'uses'}}]->(ty:{nt}) \
                 WHERE ty.id = '{def_id}' \
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

    fn search_nodes(&self, branch: &str, query: &str, limit: usize) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        // Lowercase both sides for case-insensitive substring matching.
        let q = esc(&query.to_ascii_lowercase());
        let conn = self.conn()?;
        // Push substring filter into Cypher so only matching rows cross the FFI
        // boundary. A 500-candidate cap keeps scoring overhead bounded even on
        // very large repos. The in-process scorer in search.rs re-ranks and
        // truncates to the caller-supplied limit.
        let cap = (limit * 50).max(500);
        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) \
                 WHERE contains(lower(n.name), '{q}') OR contains(lower(n.qualified_name), '{q}') \
                 RETURN {NODE_COLS} \
                 LIMIT {cap}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut result)
    }

    fn get_nodes_by_ids(&self, branch: &str, ids: &[String]) -> Result<Vec<Node>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let conn = self.conn()?;
        let id_list = ids
            .iter()
            .map(|id| format!("'{}'", esc(id)))
            .collect::<Vec<_>>()
            .join(", ");
        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) WHERE n.id IN [{id_list}] RETURN {NODE_COLS}"
            ))
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
                "MATCH (s:{nt})-[e:{et}]->(d:{nt}) RETURN s.id, d.id, e.kind, e.line"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        let mut out = Vec::new();
        for row in result {
            let src_str = str_val(&row[0])?;
            let dst_str = str_val(&row[1])?;
            let kind_str = str_val(&row[2])?;
            let line = i64_val(&row[3]).ok().filter(|l| *l >= 0).map(|l| l as u32);
            out.push(Edge {
                src: NodeId::try_from(src_str.as_str())
                    .map_err(|e| GitCortexError::Store(format!("bad src id: {e}")))?,
                dst: NodeId::try_from(dst_str.as_str())
                    .map_err(|e| GitCortexError::Store(format!("bad dst id: {e}")))?,
                kind: edge_kind_from_str(&kind_str),
                line,
            });
        }
        Ok(out)
    }

    fn search_by_attributes(
        &self,
        branch: &str,
        filter: &AttributeFilter,
        limit: usize,
    ) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let conn = self.conn()?;

        // Build AND-joined WHERE clauses from the set predicates.
        let mut clauses: Vec<String> = Vec::new();
        if let Some(k) = &filter.kind {
            clauses.push(format!("n.kind = '{}'", esc(&k.to_string())));
        }
        if let Some(a) = filter.is_async {
            clauses.push(format!("n.is_async = {a}"));
        }
        if let Some(v) = &filter.visibility {
            clauses.push(format!("n.visibility = '{}'", esc(&vis_str(v))));
        }
        // complexity is stored as -1 when absent; a bound must also exclude -1.
        if let Some(min) = filter.min_complexity {
            clauses.push(format!("n.complexity >= {min} AND n.complexity >= 0"));
        }
        if let Some(max) = filter.max_complexity {
            clauses.push(format!("n.complexity <= {max} AND n.complexity >= 0"));
        }
        if let Some(sub) = &filter.name_contains {
            clauses.push(format!(
                "contains(lower(n.name), '{}')",
                esc(&sub.to_ascii_lowercase())
            ));
        }
        if let Some(ann) = &filter.annotation {
            // annotations stored pipe-joined; substring match finds the name.
            clauses.push(format!(
                "contains(lower(n.annotations), '{}')",
                esc(&ann.to_ascii_lowercase())
            ));
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt}) {where_clause} \
                 RETURN {NODE_COLS} ORDER BY {SYMBOL_RANK} LIMIT {limit}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut result)
    }

    fn graph_stats(&self, branch: &str) -> Result<GraphStats> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let conn = self.conn()?;

        // Per-kind counts pushed into Cypher so only aggregate rows cross FFI.
        let read_counts = |query: &str| -> Result<Vec<(String, u64)>> {
            let result = conn
                .query(query)
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            let mut pairs: Vec<(String, u64)> = Vec::new();
            for row in result {
                let kind = str_val(&row[0])?;
                let count = i64_val(&row[1])?.max(0) as u64;
                pairs.push((kind, count));
            }
            // Count desc, then kind asc — deterministic, matches trait default.
            pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            Ok(pairs)
        };

        let nodes_by_kind = read_counts(&format!("MATCH (n:{nt}) RETURN n.kind, count(*) AS c"))?;
        let edges_by_kind = read_counts(&format!(
            "MATCH (:{nt})-[e:{et}]->(:{nt}) RETURN e.kind, count(*) AS c"
        ))?;

        Ok(GraphStats {
            total_nodes: nodes_by_kind.iter().map(|(_, c)| c).sum(),
            total_edges: edges_by_kind.iter().map(|(_, c)| c).sum(),
            nodes_by_kind,
            edges_by_kind,
        })
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
                "MATCH (n:{nt})-[e:{et}]->(trait_node:{nt}) \
                 WHERE trait_node.name = '{name_esc}' \
                 AND (e.kind = 'implements' OR e.kind = 'inherits') \
                 RETURN DISTINCT {NODE_COLS} ORDER BY {SYMBOL_RANK}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut result)
    }

    fn find_type_usages(&self, branch: &str, type_name: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(type_name);
        let conn = self.conn()?;
        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt})-[e:{et} {{kind: 'uses'}}]->(ty:{nt}) \
                 WHERE ty.name = '{name_esc}' \
                 RETURN DISTINCT {NODE_COLS} ORDER BY {SYMBOL_RANK}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut result)
    }

    fn find_call_sites(&self, branch: &str, function_name: &str) -> Result<Vec<CallSite>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(function_name);
        let conn = self.conn()?;
        // Return the caller columns plus the call edge's line. Alias caller as
        // `n` so NODE_COLS maps positionally; append e.line as the last column.
        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt})-[e:{et} {{kind: 'calls'}}]->(callee:{nt}) \
                 WHERE callee.name = '{name_esc}' \
                 RETURN {NODE_COLS}, e.line ORDER BY {SYMBOL_RANK}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;

        let mut sites = Vec::new();
        for row in result.by_ref() {
            // NODE_COLS is 25 columns; e.line is the 26th (index 25).
            let line = row.get(25).and_then(|v| match v {
                kuzu::Value::Int64(n) if *n >= 0 => Some(*n as u32),
                _ => None,
            });
            match queries::row_to_node(row) {
                Ok(caller) => sites.push(CallSite { caller, line }),
                Err(e) => tracing::debug!("skipping malformed call-site row: {e}"),
            }
        }
        Ok(sites)
    }

    fn find_importers(&self, branch: &str, symbol_name: &str) -> Result<Vec<Node>> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(symbol_name);
        let conn = self.conn()?;
        let mut result = conn
            .query(&format!(
                "MATCH (n:{nt})-[e:{et} {{kind: 'imports'}}]->(target:{nt}) \
                 WHERE target.name = '{name_esc}' \
                 RETURN DISTINCT {NODE_COLS} ORDER BY {SYMBOL_RANK}"
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        rows_to_nodes(&mut result)
    }

    fn type_hierarchy(&self, branch: &str, name: &str) -> Result<TypeHierarchy> {
        self.ensure_branch(branch)?;
        let nt = db_schema::node_table(branch);
        let et = db_schema::edge_table(branch);
        let name_esc = esc(name);
        let conn = self.conn()?;

        // Supertypes: types this type implements or extends (self → super).
        let mut super_result = conn
            .query(&format!(
                "MATCH (n:{nt})-[e:{et}]->(super:{nt}) \
                 WHERE n.name = '{name_esc}' \
                 AND (e.kind = 'implements' OR e.kind = 'inherits') \
                 RETURN DISTINCT {} ORDER BY {}",
                NODE_COLS.replace("n.", "super."),
                SYMBOL_RANK.replace("n.", "super.")
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let supertypes = rows_to_nodes(&mut super_result)?;

        // Subtypes: types that implement or extend this type (sub → self).
        let mut sub_result = conn
            .query(&format!(
                "MATCH (sub:{nt})-[e:{et}]->(n:{nt}) \
                 WHERE n.name = '{name_esc}' \
                 AND (e.kind = 'implements' OR e.kind = 'inherits') \
                 RETURN DISTINCT {} ORDER BY {}",
                NODE_COLS.replace("n.", "sub."),
                SYMBOL_RANK.replace("n.", "sub.")
            ))
            .map_err(|e| GitCortexError::Store(e.to_string()))?;
        let subtypes = rows_to_nodes(&mut sub_result)?;

        Ok(TypeHierarchy {
            supertypes,
            subtypes,
        })
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
        let mut queue: std::collections::VecDeque<(String, Vec<String>)> =
            std::collections::VecDeque::new();
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
            return Ok(SubGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            });
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
        let ids_list: Vec<String> = all_node_ids
            .iter()
            .map(|id| format!("'{}'", esc(id)))
            .collect();
        let ids_str = ids_list.join(", ");
        let all_edges = if ids_list.is_empty() {
            Vec::new()
        } else {
            let conn4 = self.conn()?;
            let result = conn4
                .query(&format!(
                    "MATCH (s:{nt})-[e:{et}]->(d:{nt}) \
                     WHERE s.id IN [{ids_str}] AND d.id IN [{ids_str}] \
                     RETURN s.id, d.id, e.kind, e.line"
                ))
                .map_err(|e| GitCortexError::Store(e.to_string()))?;
            let mut edges = Vec::new();
            for row in result {
                let src_str = str_val(&row[0])?;
                let dst_str = str_val(&row[1])?;
                let kind_str = str_val(&row[2])?;
                let line = i64_val(&row[3]).ok().filter(|l| *l >= 0).map(|l| l as u32);
                edges.push(Edge {
                    src: NodeId::try_from(src_str.as_str())
                        .map_err(|e| GitCortexError::Store(format!("bad src id: {e}")))?,
                    dst: NodeId::try_from(dst_str.as_str())
                        .map_err(|e| GitCortexError::Store(format!("bad dst id: {e}")))?,
                    kind: edge_kind_from_str(&kind_str),
                    line,
                });
            }
            edges
        };

        Ok(SubGraph {
            nodes: all_nodes,
            edges: all_edges,
        })
    }

    // ── Indexing state ────────────────────────────────────────────────────────

    fn last_indexed_sha(&self, branch_name: &str) -> Result<Option<String>> {
        branch::read_last_sha(&self.repo_id, branch_name)
    }

    fn set_last_indexed_sha(&mut self, branch_name: &str, sha: &str) -> Result<()> {
        branch::write_last_sha(&self.repo_id, branch_name, sha)
    }
}
