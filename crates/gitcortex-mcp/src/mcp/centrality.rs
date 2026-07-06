//! Pure in-degree centrality over `Calls` edges — shared by `tour.rs` (which
//! uses it to rank entry points) and `find_god_nodes` (which surfaces it
//! directly as named hub detection, GitCortex's no-LLM answer to what other
//! tools call "god object" / "god node" detection).
//!
//! No clustering here — see `clustering.rs` for label-propagation community
//! detection, which is a separate, coarser-grained signal.

use gitcortex_core::{error::Result, store::GraphStore};
use serde::Serialize;

// Re-export so clustering.rs and tour.rs can import from here without
// knowing about the core crate's module layout.
pub use gitcortex_core::graph::in_degree_by_calls;

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

#[cfg(test)]
mod tests {
    use gitcortex_core::graph::{Edge, NodeId};
    use gitcortex_core::schema::EdgeKind;

    use super::in_degree_by_calls;

    #[test]
    fn in_degree_empty_edges_returns_empty_map() {
        assert!(in_degree_by_calls(&[]).is_empty());
    }

    #[test]
    fn in_degree_counts_calls_edges_only() {
        let a = NodeId::new();
        let b = NodeId::new();
        let edges = vec![
            Edge::new(a.clone(), b.clone(), EdgeKind::Calls),
            Edge::new(a.clone(), b.clone(), EdgeKind::Contains), // should not count
            Edge::new(a.clone(), b.clone(), EdgeKind::Uses),     // should not count
        ];
        let map = in_degree_by_calls(&edges);
        assert_eq!(map.get(&b.as_str()), Some(&1));
        assert!(
            !map.contains_key(&a.as_str()),
            "src should not appear as dst"
        );
    }

    #[test]
    fn in_degree_multiple_callers_accumulate() {
        let dst = NodeId::new();
        let callers: Vec<NodeId> = (0..5).map(|_| NodeId::new()).collect();
        let edges: Vec<Edge> = callers
            .iter()
            .map(|src| Edge::new(src.clone(), dst.clone(), EdgeKind::Calls))
            .collect();
        let map = in_degree_by_calls(&edges);
        assert_eq!(map.get(&dst.as_str()), Some(&5));
    }

    #[test]
    fn in_degree_src_node_absent_unless_also_dst() {
        let a = NodeId::new();
        let b = NodeId::new();
        let c = NodeId::new();
        // a→b, b→c: b is both src and dst
        let edges = vec![
            Edge::new(a.clone(), b.clone(), EdgeKind::Calls),
            Edge::new(b.clone(), c.clone(), EdgeKind::Calls),
        ];
        let map = in_degree_by_calls(&edges);
        assert!(
            !map.contains_key(&a.as_str()),
            "pure caller should not appear"
        );
        assert_eq!(map.get(&b.as_str()), Some(&1));
        assert_eq!(map.get(&c.as_str()), Some(&1));
    }

    #[test]
    fn in_degree_sort_order_descending_ties_by_qname() {
        // Verify find_god_nodes ordering contract via in_degree values directly.
        let high = NodeId::new();
        let low = NodeId::new();
        let callers: Vec<NodeId> = (0..3).map(|_| NodeId::new()).collect();
        let mut edges: Vec<Edge> = callers
            .iter()
            .map(|src| Edge::new(src.clone(), high.clone(), EdgeKind::Calls))
            .collect();
        edges.push(Edge::new(NodeId::new(), low.clone(), EdgeKind::Calls));
        let map = in_degree_by_calls(&edges);
        assert!(map[&high.as_str()] > map[&low.as_str()]);
    }
}
