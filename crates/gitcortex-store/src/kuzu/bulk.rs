//! Bulk load path for full (re)indexing.
//!
//! Inserting nodes/edges one row at a time — or even via `UNWIND … MATCH …
//! CREATE` — is dominated by per-row primary-key lookups: on a 500k-LOC repo
//! that is minutes. KuzuDB's `COPY … FROM <csv>` resolves the FROM/TO primary
//! keys in one bulk pass; in microbenchmarks it loaded 20k edges in ~180ms vs
//! ~19s for per-row `MATCH`+`CREATE` (~100×).
//!
//! This path is only taken for a fresh branch (empty node table, no deletes in
//! the diff) — i.e. the first full index. Incremental hook updates stay on the
//! per-row path in `mod.rs`, where the row count is tiny and `COPY`'s
//! file-staging overhead would not pay off.

use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, Node, NodeId},
    resolution::{database_candidate_count_is_resolvable, target_kind_is_valid},
    schema::{EdgeConfidence, EdgeKind},
};
use kuzu::Connection;

use super::conv::{language_extensions, vis_str};
use super::RawReference;

/// Quote a string field for an RFC-4180 CSV cell: always wrap in double quotes
/// and double any interior quote. Safe for values containing commas, quotes,
/// and newlines (docstrings, signatures).
fn csv_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Serialize one node as a CSV line, columns in `node table` declaration order.
fn node_csv_line(n: &Node) -> String {
    let m = &n.metadata;
    let d = &m.definition;
    // Order must match `schema::ensure_branch` node-table columns.
    let complexity: i64 = m.lld.complexity.map(|c| c as i64).unwrap_or(-1);
    format!(
        "{id},{kind},{name},{qname},{file},{sl},{el},{loc},{vis},{ia},{iu},{ist},{iab},{ifi},{ip},{ig},{ic},{gb},{ds},{db},{dd},{dsb},{deb},{cx},{ann}",
        id = csv_quote(&n.id.as_str()),
        kind = csv_quote(&n.kind.to_string()),
        name = csv_quote(&n.name),
        qname = csv_quote(&n.qualified_name),
        file = csv_quote(n.file.to_string_lossy().as_ref()),
        sl = n.span.start_line,
        el = n.span.end_line,
        loc = m.loc,
        vis = csv_quote(&vis_str(&m.visibility)),
        ia = m.is_async,
        iu = m.is_unsafe,
        ist = m.is_static,
        iab = m.is_abstract,
        ifi = m.is_final,
        ip = m.is_property,
        ig = m.is_generator,
        ic = m.is_const,
        gb = csv_quote(&m.generic_bounds.join("|")),
        ds = csv_quote(&d.signature),
        db = csv_quote(&d.body),
        dd = csv_quote(d.doc_comment.as_deref().unwrap_or("")),
        dsb = d.start_byte,
        deb = d.end_byte,
        cx = complexity,
        ann = csv_quote(&m.annotations.join("|")),
    )
}

/// Reconcile a fresh full-index branch's edges against `unique_refs` completely
/// in memory so `bulk_load` can `COPY` the final edge set in one pass.
///
/// Mirrors the exact semantics of `reconcile_reference_names` (clear any direct
/// edge whose `(src_id, dst.name, kind)` is governed by a raw reference fact,
/// resolve `Imports` first so import verification sees all in-repo imports,
/// then resolve `Calls` preserving every call-site line and the remaining
/// relationship kinds with identical language-family scoping and candidate
/// fanout caps).
pub(super) fn reconcile_full_index_edges(
    nodes: &[Node],
    edges: &[Edge],
    unique_refs: &[RawReference],
) -> Vec<Edge> {
    if unique_refs.is_empty() {
        return edges.to_vec();
    }

    struct IndexedNode<'a> {
        id: NodeId,
        id_str: String,
        name: &'a str,
        kind: gitcortex_core::schema::NodeKind,
        file: String,
    }

    let mut dedup_nodes: Vec<IndexedNode<'_>> = Vec::with_capacity(nodes.len());
    let mut node_index_by_id: HashMap<String, usize> = HashMap::with_capacity(nodes.len());
    let mut nodes_by_name: HashMap<&str, Vec<usize>> = HashMap::new();

    for n in nodes {
        let id_str = n.id.as_str();
        if node_index_by_id.contains_key(&id_str) {
            continue;
        }
        let idx = dedup_nodes.len();
        node_index_by_id.insert(id_str.clone(), idx);
        nodes_by_name.entry(n.name.as_str()).or_default().push(idx);
        dedup_nodes.push(IndexedNode {
            id: n.id.clone(),
            id_str,
            name: n.name.as_str(),
            kind: n.kind.clone(),
            file: n.file.to_string_lossy().into_owned(),
        });
    }

    // Drop any pre-existing edge that `reconcile_reference_names` would clear
    // (`src.id = fact.src_id AND dst.name = fact.target_name AND e.kind = fact.kind`).
    let cleared: HashSet<(&str, &str, &EdgeKind)> = unique_refs
        .iter()
        .map(|fact| (fact.src_id.as_str(), fact.target_name.as_str(), &fact.kind))
        .collect();

    let mut out: Vec<Edge> = Vec::with_capacity(edges.len());
    let mut seen_non_call: HashSet<(String, String, EdgeKind)> = HashSet::new();
    let mut imported_by_file: HashMap<String, HashSet<String>> = HashMap::new();

    for e in edges {
        let s = e.src.as_str();
        let d = e.dst.as_str();
        let (Some(&src_idx), Some(&dst_idx)) = (node_index_by_id.get(&s), node_index_by_id.get(&d))
        else {
            continue;
        };
        if e.kind != EdgeKind::Calls
            && !seen_non_call.insert((s.clone(), d.clone(), e.kind.clone()))
        {
            continue;
        }
        if cleared.contains(&(s.as_str(), dedup_nodes[dst_idx].name, &e.kind)) {
            continue;
        }
        if e.kind == EdgeKind::Imports {
            imported_by_file
                .entry(dedup_nodes[src_idx].file.clone())
                .or_default()
                .insert(dedup_nodes[dst_idx].name.to_owned());
        }
        out.push(e.clone());
    }

    let resolve_targets =
        |target_name: &str, edge_kind: &EdgeKind, caller_file: &str| -> Vec<usize> {
            let Some(candidates) = nodes_by_name.get(target_name) else {
                return Vec::new();
            };
            let exts = language_extensions(caller_file);
            let matched: Vec<usize> = candidates
                .iter()
                .copied()
                .filter(|&idx| {
                    let node = &dedup_nodes[idx];
                    if !target_kind_is_valid(edge_kind, &node.kind) {
                        return false;
                    }
                    match exts {
                        Some(exts) => exts.iter().any(|ext| node.file.ends_with(ext)),
                        None => true,
                    }
                })
                .collect();
            if matched.is_empty() || !database_candidate_count_is_resolvable(matched.len() as i64) {
                return Vec::new();
            }
            matched
        };

    // Pass 1: resolve `Imports` facts first so later edge kinds see all
    // import-verified targets in `imported_by_file`.
    let mut new_import_entries: Vec<(String, String)> = Vec::new();
    for fact in unique_refs.iter().filter(|f| f.kind == EdgeKind::Imports) {
        let Some(&src_idx) = node_index_by_id.get(&fact.src_id) else {
            continue;
        };
        let targets = resolve_targets(&fact.target_name, &EdgeKind::Imports, &fact.src_file);
        if targets.is_empty() {
            continue;
        }
        let src_file = &dedup_nodes[src_idx].file;
        let verified = imported_by_file
            .get(src_file)
            .is_some_and(|names| names.contains(&fact.target_name));
        let confidence = if verified {
            EdgeConfidence::Resolved
        } else {
            EdgeConfidence::Inferred
        };
        for tgt_idx in targets {
            new_import_entries.push((src_file.clone(), dedup_nodes[tgt_idx].name.to_owned()));
            out.push(Edge {
                src: dedup_nodes[src_idx].id.clone(),
                dst: dedup_nodes[tgt_idx].id.clone(),
                kind: EdgeKind::Imports,
                line: None,
                confidence,
            });
        }
    }
    for (file, name) in new_import_entries {
        imported_by_file.entry(file).or_default().insert(name);
    }

    // Pass 2: resolve `Calls` (preserving every call-site line) and remaining
    // relationship kinds.
    for fact in unique_refs.iter().filter(|f| f.kind != EdgeKind::Imports) {
        if fact.kind == EdgeKind::Calls && fact.line < 0 {
            continue;
        }
        if !matches!(
            fact.kind,
            EdgeKind::Calls
                | EdgeKind::Uses
                | EdgeKind::Implements
                | EdgeKind::Inherits
                | EdgeKind::Throws
                | EdgeKind::Annotated
                | EdgeKind::References
                | EdgeKind::HandledBy
        ) {
            continue;
        }
        let Some(&src_idx) = node_index_by_id.get(&fact.src_id) else {
            continue;
        };
        let targets = resolve_targets(&fact.target_name, &fact.kind, &fact.src_file);
        if targets.is_empty() {
            continue;
        }
        let verified = imported_by_file
            .get(&dedup_nodes[src_idx].file)
            .is_some_and(|names| names.contains(&fact.target_name));
        let confidence = if verified {
            EdgeConfidence::Resolved
        } else {
            EdgeConfidence::Inferred
        };
        let line = if fact.kind == EdgeKind::Calls {
            Some(fact.line as u32)
        } else {
            None
        };
        for tgt_idx in targets {
            out.push(Edge {
                src: dedup_nodes[src_idx].id.clone(),
                dst: dedup_nodes[tgt_idx].id.clone(),
                kind: fact.kind.clone(),
                line,
                confidence,
            });
        }
    }

    // Suppress unused-field lint if `id_str` is only used for map insertion.
    let _ = dedup_nodes.first().map(|n| n.id_str.len());

    out
}

/// Names of the three per-branch KuzuDB tables targeted by [`bulk_load`].
pub(super) struct BranchTables<'a> {
    pub(super) nodes: &'a str,
    pub(super) edges: &'a str,
    pub(super) refs: &'a str,
}

/// Bulk-load `nodes`, `edges`, and `refs` into the fresh branch tables via
/// `COPY`. Edges whose endpoints are not among the loaded node ids are dropped
/// (same silent-skip semantics as the per-row `MATCH`-based path). `tmp_dir`
/// is where the staging CSVs are written; the caller owns its lifetime.
///
/// Returns the number of edges actually written.
pub(super) fn bulk_load(
    conn: &Connection,
    tables: BranchTables<'_>,
    tmp_dir: &std::path::Path,
    nodes: &[Node],
    edges: &[Edge],
    refs: &[RawReference],
) -> Result<usize> {
    let (nt, et, rt) = (tables.nodes, tables.edges, tables.refs);
    let nodes_csv = tmp_dir.join("nodes.csv");
    let edges_csv = tmp_dir.join("edges.csv");
    let refs_csv = tmp_dir.join("refs.csv");

    // ── Nodes ──────────────────────────────────────────────────────────────────
    let mut node_ids: HashSet<String> = HashSet::with_capacity(nodes.len());
    {
        let f = std::fs::File::create(&nodes_csv)
            .map_err(|e| GitCortexError::Store(format!("create nodes.csv: {e}")))?;
        let mut w = BufWriter::new(f);
        for n in nodes {
            let id = n.id.as_str();
            // Dedup by id — a rename delta can surface the same id twice.
            if !node_ids.insert(id.clone()) {
                continue;
            }
            writeln!(w, "{}", node_csv_line(n))
                .map_err(|e| GitCortexError::Store(format!("write nodes.csv: {e}")))?;
        }
        w.flush()
            .map_err(|e| GitCortexError::Store(format!("flush nodes.csv: {e}")))?;
    }

    // ── Edges ──────────────────────────────────────────────────────────────────
    let mut edge_count = 0usize;
    let mut seen_edges: HashSet<(String, String, String)> = HashSet::new();
    {
        let f = std::fs::File::create(&edges_csv)
            .map_err(|e| GitCortexError::Store(format!("create edges.csv: {e}")))?;
        let mut w = BufWriter::new(f);
        for e in edges {
            let s = e.src.as_str();
            let d = e.dst.as_str();
            // COPY rel requires both endpoints to exist; drop dangling edges.
            if !node_ids.contains(&s) || !node_ids.contains(&d) {
                continue;
            }
            let k = e.kind.to_string();
            if e.kind != gitcortex_core::schema::EdgeKind::Calls
                && !seen_edges.insert((s.clone(), d.clone(), k.clone()))
            {
                continue;
            }
            let line = e.line.map(|l| l as i64).unwrap_or(-1);
            writeln!(
                w,
                "{},{},{},{},{}",
                csv_quote(&s),
                csv_quote(&d),
                csv_quote(&k),
                line,
                csv_quote(&e.confidence.to_string())
            )
            .map_err(|e| GitCortexError::Store(format!("write edges.csv: {e}")))?;
            edge_count += 1;
        }
        w.flush()
            .map_err(|e| GitCortexError::Store(format!("flush edges.csv: {e}")))?;
    }

    // ── Reference facts ────────────────────────────────────────────────────────
    let mut ref_count = 0usize;
    if !refs.is_empty() {
        let f = std::fs::File::create(&refs_csv)
            .map_err(|e| GitCortexError::Store(format!("create refs.csv: {e}")))?;
        let mut w = BufWriter::new(f);
        let mut seen_refs: HashSet<&str> = HashSet::with_capacity(refs.len());
        for fact in refs {
            if !seen_refs.insert(fact.id.as_str()) {
                continue;
            }
            // Column order matches `schema::ensure_branch` reference-table columns:
            // (id, src_id, src_file, target_name, kind, line).
            writeln!(
                w,
                "{},{},{},{},{},{}",
                csv_quote(&fact.id),
                csv_quote(&fact.src_id),
                csv_quote(&fact.src_file),
                csv_quote(&fact.target_name),
                csv_quote(&fact.kind.to_string()),
                fact.line,
            )
            .map_err(|e| GitCortexError::Store(format!("write refs.csv: {e}")))?;
            ref_count += 1;
        }
        w.flush()
            .map_err(|e| GitCortexError::Store(format!("flush refs.csv: {e}")))?;
    }

    // ── COPY ───────────────────────────────────────────────────────────────────
    // Header-less CSV; column order matches the table declaration.
    let nodes_path = nodes_csv.to_string_lossy().replace('\'', "\\'");
    // PARALLEL=false is load-bearing, not just conservative: Kuzu's parallel
    // CSV importer doesn't support quoted fields containing newlines
    // (kuzudb/kuzu#5778), and our node CSVs routinely have them — docstrings
    // and multi-line signatures survive into `csv_quote()`'s RFC-4180
    // doubled-quote escaping. Flipping this risks reintroducing a crash class
    // adjacent to the one fixed in 361cbbf, on a now-archived dependency with
    // no upstream to report a regression to.
    conn.query(&format!(
        "COPY {nt} FROM '{nodes_path}' (HEADER=false, PARALLEL=false, ESCAPE='\"')"
    ))
    .map_err(|e| GitCortexError::Store(format!("COPY nodes: {e}")))?;

    if edge_count > 0 {
        let edges_path = edges_csv.to_string_lossy().replace('\'', "\\'");
        // Same reasoning as the nodes COPY above — edge CSVs can also carry
        // multiline confidence/kind strings via csv_quote(); keep PARALLEL=false.
        conn.query(&format!(
            "COPY {et} FROM '{edges_path}' (HEADER=false, PARALLEL=false, ESCAPE='\"')"
        ))
        .map_err(|e| GitCortexError::Store(format!("COPY edges: {e}")))?;
    }

    if ref_count > 0 {
        let refs_path = refs_csv.to_string_lossy().replace('\'', "\\'");
        conn.query(&format!(
            "COPY {rt} FROM '{refs_path}' (HEADER=false, PARALLEL=false, ESCAPE='\"')"
        ))
        .map_err(|e| GitCortexError::Store(format!("COPY refs: {e}")))?;
    }

    Ok(edge_count)
}
