//! Label propagation community detection — GitCortex's no-LLM answer to
//! Graphify's Leiden clustering. Deliberately a simpler algorithm: label
//! propagation has no objective function to optimise (just "agree with your
//! neighbours"), so it's cheap, has no extra dependency, and is easy to keep
//! fully deterministic — a hard requirement for a tool whose entire value
//! proposition is "same input, same answer, every time."
//!
//! Adjacency is undirected, built from `Contains` + `Calls` edges (same edge
//! set `tour.rs` uses for its centrality signal — these two are the
//! structurally/behaviourally "close" relationships; `Uses`/`Imports` are
//! left out as weaker signals that would blur cluster boundaries).

use std::collections::HashMap;

use gitcortex_core::{
    error::Result,
    graph::{Edge, Node},
    schema::EdgeKind,
    store::GraphStore,
};
use serde::Serialize;

use super::centrality::in_degree_by_calls;

const MAX_ITERATIONS: u32 = 20;
const DEFAULT_MIN_CLUSTER_SIZE: usize = 3;
const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct ClusterMember {
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub start_line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    /// Representative member's qualified name — the highest in-degree symbol
    /// in the cluster (ties broken by qualified_name), used as a human label.
    pub label: String,
    /// True member count — may exceed `members.len()` if the cluster was
    /// truncated to `MAX_MEMBERS_PER_CLUSTER`.
    pub size: usize,
    pub members: Vec<ClusterMember>,
}

/// Cap members shown per cluster so one large, densely-connected cluster
/// can't dump hundreds of entries — `size` stays the honest true count.
const MAX_MEMBERS_PER_CLUSTER: usize = 25;

/// Detect communities via synchronous label propagation, returning clusters
/// of `min_cluster_size` or more members, ranked by size descending (ties
/// broken by label).
pub fn find_clusters<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    min_cluster_size: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<Cluster>> {
    let min_cluster_size = min_cluster_size.unwrap_or(DEFAULT_MIN_CLUSTER_SIZE).max(2);
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;

    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    let in_degree = in_degree_by_calls(&edges);
    let adjacency = build_undirected_adjacency(&edges);
    let labels = propagate_labels(&nodes, &adjacency);

    // Group node ids by final label.
    let mut groups: HashMap<String, Vec<&Node>> = HashMap::new();
    for n in &nodes {
        let id = n.id.as_str();
        let label = labels.get(&id).cloned().unwrap_or(id);
        groups.entry(label).or_default().push(n);
    }

    let mut clusters: Vec<Cluster> = groups
        .into_values()
        .filter(|members| members.len() >= min_cluster_size)
        .map(|mut members| {
            // Deterministic member order within a cluster.
            members.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

            // Representative: highest in-degree, ties broken by qualified_name.
            // members.len() >= min_cluster_size (>= 2) from the filter above,
            // so max_by on a non-empty iterator always returns Some.
            let representative = members
                .iter()
                .max_by(|a, b| {
                    let deg_a = in_degree.get(&a.id.as_str()).copied().unwrap_or(0);
                    let deg_b = in_degree.get(&b.id.as_str()).copied().unwrap_or(0);
                    deg_a
                        .cmp(&deg_b)
                        .then_with(|| b.qualified_name.cmp(&a.qualified_name))
                })
                .unwrap_or_else(|| &members[0]);

            Cluster {
                label: representative.qualified_name.clone(),
                size: members.len(),
                members: members
                    .iter()
                    .take(MAX_MEMBERS_PER_CLUSTER)
                    .map(|n| ClusterMember {
                        name: n.name.clone(),
                        qualified_name: n.qualified_name.clone(),
                        kind: n.kind.to_string(),
                        file: n.file.display().to_string(),
                        start_line: n.span.start_line,
                    })
                    .collect(),
            }
        })
        .collect();

    clusters.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.label.cmp(&b.label)));
    clusters.truncate(limit);
    Ok(clusters)
}

/// Returns a `node_id → cluster_label` map using label propagation on
/// `Contains`+`Calls` edges. The cluster label is the representative node id
/// (stable UUID). Used by `tour.rs` to group steps by community.
pub(super) fn node_cluster_labels(nodes: &[Node], edges: &[Edge]) -> HashMap<String, String> {
    let adjacency = build_undirected_adjacency(edges);
    propagate_labels(nodes, &adjacency)
}

fn build_undirected_adjacency(edges: &[Edge]) -> HashMap<String, Vec<String>> {
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for e in edges {
        if matches!(e.kind, EdgeKind::Contains | EdgeKind::Calls) {
            let (a, b) = (e.src.as_str(), e.dst.as_str());
            adjacency.entry(a.clone()).or_default().push(b.clone());
            adjacency.entry(b).or_default().push(a);
        }
    }
    adjacency
}

/// Synchronous label propagation: every node starts labelled with its own
/// id; each pass, every node (visited in a fixed, deterministic order)
/// adopts the most common label among its neighbours, ties broken by the
/// lexicographically lowest candidate label's qualified name. Stops at
/// convergence (no label changed) or `MAX_ITERATIONS`, whichever first.
fn propagate_labels(
    nodes: &[Node],
    adjacency: &HashMap<String, Vec<String>>,
) -> HashMap<String, String> {
    let qname_of: HashMap<String, &str> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.qualified_name.as_str()))
        .collect();

    // Fixed, deterministic visiting order — independent of HashMap iteration
    // order, which is what makes re-runs on identical input byte-identical.
    let mut order: Vec<String> = nodes.iter().map(|n| n.id.as_str()).collect();
    order.sort_by_key(|id| qname_of.get(id.as_str()).copied().unwrap_or(""));

    let mut labels: HashMap<String, String> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.id.as_str()))
        .collect();

    for _ in 0..MAX_ITERATIONS {
        let mut changed = false;
        for id in &order {
            let Some(neighbors) = adjacency.get(id) else {
                continue;
            };
            if neighbors.is_empty() {
                continue;
            }

            let mut counts: HashMap<&str, u32> = HashMap::new();
            for nb in neighbors {
                if let Some(label) = labels.get(nb) {
                    *counts.entry(label.as_str()).or_insert(0) += 1;
                }
            }
            if counts.is_empty() {
                continue;
            }

            let best = counts
                .into_iter()
                .max_by(|(la, ca), (lb, cb)| {
                    ca.cmp(cb).then_with(|| {
                        let qa = qname_of.get(*la).copied().unwrap_or("");
                        let qb = qname_of.get(*lb).copied().unwrap_or("");
                        // Reverse: we want the LOWEST qualified_name to win
                        // under max_by, so compare b against a.
                        qb.cmp(qa)
                    })
                })
                .map(|(label, _)| label.to_owned());

            if let Some(new_label) = best {
                if labels.get(id) != Some(&new_label) {
                    labels.insert(id.clone(), new_label);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    labels
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gitcortex_core::graph::{Edge, NodeId, NodeMetadata, Span};
    use gitcortex_core::schema::{EdgeKind, NodeKind};

    use super::*;

    fn make_node(name: &str) -> Node {
        Node {
            id: NodeId::new(),
            kind: NodeKind::Function,
            name: name.to_owned(),
            qualified_name: name.to_owned(),
            file: PathBuf::from("src/lib.rs"),
            span: Span {
                start_line: 1,
                end_line: 5,
            },
            metadata: NodeMetadata::default(),
        }
    }

    // ── adjacency ────────────────────────────────────────────────────────────

    #[test]
    fn build_undirected_adjacency_includes_both_directions() {
        let src = NodeId::new();
        let dst = NodeId::new();
        let edges = vec![Edge::new(src.clone(), dst.clone(), EdgeKind::Calls)];
        let adj = build_undirected_adjacency(&edges);
        assert!(adj.get(&src.as_str()).unwrap().contains(&dst.as_str()));
        assert!(adj.get(&dst.as_str()).unwrap().contains(&src.as_str()));
    }

    #[test]
    fn build_undirected_adjacency_excludes_uses_edges() {
        let src = NodeId::new();
        let dst = NodeId::new();
        let edges = vec![Edge::new(src.clone(), dst, EdgeKind::Uses)];
        let adj = build_undirected_adjacency(&edges);
        assert!(!adj.contains_key(&src.as_str()));
    }

    #[test]
    fn build_undirected_adjacency_includes_contains_edges() {
        let parent = NodeId::new();
        let child = NodeId::new();
        let edges = vec![Edge::new(parent.clone(), child.clone(), EdgeKind::Contains)];
        let adj = build_undirected_adjacency(&edges);
        assert!(adj.get(&parent.as_str()).unwrap().contains(&child.as_str()));
        assert!(adj.get(&child.as_str()).unwrap().contains(&parent.as_str()));
    }

    // ── label propagation ────────────────────────────────────────────────────

    #[test]
    fn propagate_labels_empty_nodes_returns_empty() {
        let labels = propagate_labels(&[], &HashMap::new());
        assert!(labels.is_empty());
    }

    #[test]
    fn propagate_labels_isolated_node_keeps_own_id() {
        let node = make_node("lone_fn");
        let id = node.id.as_str();
        let labels = propagate_labels(&[node], &HashMap::new());
        // Isolated node has no neighbours → never updated → stays as its own id.
        assert_eq!(labels.get(&id), Some(&id));
    }

    #[test]
    fn propagate_labels_two_connected_nodes_converge() {
        let a = make_node("alpha");
        let b = make_node("beta");
        let a_id = a.id.as_str();
        let b_id = b.id.as_str();

        let mut adj = HashMap::new();
        adj.insert(a_id.clone(), vec![b_id.clone()]);
        adj.insert(b_id.clone(), vec![a_id.clone()]);

        let labels = propagate_labels(&[a, b], &adj);
        // Both must end up with the same label after convergence.
        assert_eq!(
            labels[&a_id], labels[&b_id],
            "connected pair must converge to same label"
        );
    }

    #[test]
    fn propagate_labels_three_node_chain_converges() {
        // A — B — C: all three should converge to the same community.
        let a = make_node("aaa");
        let b = make_node("bbb");
        let c = make_node("ccc");
        let a_id = a.id.as_str();
        let b_id = b.id.as_str();
        let c_id = c.id.as_str();

        let mut adj = HashMap::new();
        adj.insert(a_id.clone(), vec![b_id.clone()]);
        adj.insert(b_id.clone(), vec![a_id.clone(), c_id.clone()]);
        adj.insert(c_id.clone(), vec![b_id.clone()]);

        let labels = propagate_labels(&[a, b, c], &adj);
        assert_eq!(labels[&a_id], labels[&b_id]);
        assert_eq!(labels[&b_id], labels[&c_id]);
    }

    #[test]
    fn propagate_labels_deterministic_same_input_same_output() {
        // Core guarantee: identical state → byte-identical output.
        // Build a 6-node graph with two triangles that share one edge.
        let nodes: Vec<Node> = ["p", "q", "r", "x", "y", "z"]
            .iter()
            .map(|n| make_node(n))
            .collect();
        let ids: Vec<String> = nodes.iter().map(|n| n.id.as_str()).collect();

        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        // Triangle 1: p-q-r
        for (a, b) in [(&ids[0], &ids[1]), (&ids[1], &ids[2]), (&ids[2], &ids[0])] {
            adj.entry(a.clone()).or_default().push(b.clone());
            adj.entry(b.clone()).or_default().push(a.clone());
        }
        // Triangle 2: x-y-z
        for (a, b) in [(&ids[3], &ids[4]), (&ids[4], &ids[5]), (&ids[5], &ids[3])] {
            adj.entry(a.clone()).or_default().push(b.clone());
            adj.entry(b.clone()).or_default().push(a.clone());
        }

        let run1 = propagate_labels(&nodes, &adj);
        let run2 = propagate_labels(&nodes, &adj);
        assert_eq!(run1, run2, "label propagation must be deterministic");
    }

    #[test]
    fn propagate_labels_two_triangles_form_distinct_clusters() {
        // p-q-r and x-y-z: disjoint → different final labels.
        let nodes: Vec<Node> = ["p", "q", "r", "x", "y", "z"]
            .iter()
            .map(|n| make_node(n))
            .collect();
        let ids: Vec<String> = nodes.iter().map(|n| n.id.as_str()).collect();

        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for (a, b) in [(&ids[0], &ids[1]), (&ids[1], &ids[2]), (&ids[2], &ids[0])] {
            adj.entry(a.clone()).or_default().push(b.clone());
            adj.entry(b.clone()).or_default().push(a.clone());
        }
        for (a, b) in [(&ids[3], &ids[4]), (&ids[4], &ids[5]), (&ids[5], &ids[3])] {
            adj.entry(a.clone()).or_default().push(b.clone());
            adj.entry(b.clone()).or_default().push(a.clone());
        }

        let labels = propagate_labels(&nodes, &adj);

        let group1_label = &labels[&ids[0]];
        let group2_label = &labels[&ids[3]];
        // All within each triangle share a label.
        assert_eq!(&labels[&ids[1]], group1_label);
        assert_eq!(&labels[&ids[2]], group1_label);
        assert_eq!(&labels[&ids[4]], group2_label);
        assert_eq!(&labels[&ids[5]], group2_label);
        // The two groups have different labels.
        assert_ne!(
            group1_label, group2_label,
            "disjoint triangles must form distinct clusters"
        );
    }
}
