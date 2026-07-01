//! Pure in-degree centrality over `Calls` edges — shared by `tour.rs` (which
//! uses it to rank entry points) and `find_god_nodes` (which surfaces it
//! directly as named hub detection, GitCortex's no-LLM answer to what other
//! tools call "god object" / "god node" detection).
//!
//! No clustering here — see `clustering.rs` for label-propagation community
//! detection, which is a separate, coarser-grained signal.

use std::collections::HashMap;

use gitcortex_core::{error::Result, graph::Edge, schema::EdgeKind, store::GraphStore};
use serde::Serialize;

/// Count inbound `Calls` edges per node id. Mirrors the algorithm `tour.rs`
/// has used since v0.3 — extracted here so both callers stay in lock-step
/// instead of drifting via duplicated logic.
pub fn in_degree_by_calls(edges: &[Edge]) -> HashMap<String, u32> {
    let mut in_degree: HashMap<String, u32> = HashMap::new();
    for e in edges {
        if matches!(e.kind, EdgeKind::Calls) {
            *in_degree.entry(e.dst.as_str()).or_insert(0) += 1;
        }
    }
    in_degree
}

#[derive(Debug, Clone, Serialize)]
pub struct GodNode {
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub start_line: u32,
    pub in_degree: u32,
}

/// Default floor: a symbol needs at least this many inbound calls to count
/// as a hub. Chosen to sit above "normal" fan-in for a small/medium repo
/// without requiring per-repo tuning.
const DEFAULT_MIN_IN_DEGREE: u32 = 10;
const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

/// Find high-fan-in "hub" symbols — functions/methods many other symbols
/// call into. Deterministic: ranked by in-degree descending, ties broken by
/// `qualified_name` ascending, so re-running on the same indexed state
/// always produces byte-identical output.
pub fn find_god_nodes<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    min_in_degree: Option<u32>,
    limit: Option<usize>,
) -> Result<Vec<GodNode>> {
    let min_in_degree = min_in_degree.unwrap_or(DEFAULT_MIN_IN_DEGREE);
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let in_degree = in_degree_by_calls(&edges);

    let mut scored: Vec<(GodNode, u32)> = nodes
        .into_iter()
        .filter_map(|n| {
            let deg = in_degree.get(&n.id.as_str()).copied().unwrap_or(0);
            if deg < min_in_degree {
                return None;
            }
            Some((
                GodNode {
                    name: n.name,
                    qualified_name: n.qualified_name,
                    kind: n.kind.to_string(),
                    file: n.file.display().to_string(),
                    start_line: n.span.start_line,
                    in_degree: deg,
                },
                deg,
            ))
        })
        .collect();

    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.qualified_name.cmp(&b.0.qualified_name))
    });

    Ok(scored.into_iter().take(limit).map(|(g, _)| g).collect())
}
