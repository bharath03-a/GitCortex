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

use std::collections::HashSet;
use std::io::{BufWriter, Write};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, Node},
};
use kuzu::Connection;

use super::conv::vis_str;

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
    format!(
        "{id},{kind},{name},{qname},{file},{sl},{el},{loc},{vis},{ia},{iu},{ist},{iab},{ifi},{ip},{ig},{ic},{gb},{ds},{db},{dd},{dsb},{deb}",
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
    )
}

/// Bulk-load `nodes` and `edges` into the fresh branch tables `nt`/`et` via
/// `COPY`. Edges whose endpoints are not among the loaded node ids are dropped
/// (same silent-skip semantics as the per-row `MATCH`-based path). `tmp_dir`
/// is where the staging CSVs are written; the caller owns its lifetime.
///
/// Returns the number of edges actually written.
pub(super) fn bulk_load(
    conn: &Connection,
    nt: &str,
    et: &str,
    tmp_dir: &std::path::Path,
    nodes: &[Node],
    edges: &[Edge],
) -> Result<usize> {
    let nodes_csv = tmp_dir.join("nodes.csv");
    let edges_csv = tmp_dir.join("edges.csv");

    // ── Nodes ────────────────────────────────────────────────────────────────
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
            if !seen_edges.insert((s.clone(), d.clone(), k.clone())) {
                continue;
            }
            writeln!(w, "{},{},{}", csv_quote(&s), csv_quote(&d), csv_quote(&k))
                .map_err(|e| GitCortexError::Store(format!("write edges.csv: {e}")))?;
            edge_count += 1;
        }
        w.flush()
            .map_err(|e| GitCortexError::Store(format!("flush edges.csv: {e}")))?;
    }

    // ── COPY ───────────────────────────────────────────────────────────────────
    // Header-less CSV; column order matches the table declaration.
    let nodes_path = nodes_csv.to_string_lossy().replace('\'', "\\'");
    conn.query(&format!(
        "COPY {nt} FROM '{nodes_path}' (HEADER=false, PARALLEL=false)"
    ))
    .map_err(|e| GitCortexError::Store(format!("COPY nodes: {e}")))?;

    if edge_count > 0 {
        let edges_path = edges_csv.to_string_lossy().replace('\'', "\\'");
        conn.query(&format!(
            "COPY {et} FROM '{edges_path}' (HEADER=false, PARALLEL=false)"
        ))
        .map_err(|e| GitCortexError::Store(format!("COPY edges: {e}")))?;
    }

    Ok(edge_count)
}
