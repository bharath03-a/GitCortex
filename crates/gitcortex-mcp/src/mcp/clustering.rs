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
        let label = labels
            .get(n.id.as_str().as_str())
            .cloned()
            .unwrap_or_else(|| n.id.as_str());
        groups.entry(label).or_default().push(n);
    }

    let mut clusters: Vec<Cluster> = groups
        .into_values()
        .filter(|members| members.len() >= min_cluster_size)
        .map(|mut members| {
            // Deterministic member order within a cluster.
            members.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

            // Representative: highest in-degree, ties broken by qualified_name.
            let representative = members
                .iter()
                .max_by(|a, b| {
                    let deg_a = in_degree.get(&a.id.as_str()).copied().unwrap_or(0);
                    let deg_b = in_degree.get(&b.id.as_str()).copied().unwrap_or(0);
                    deg_a
                        .cmp(&deg_b)
                        .then_with(|| b.qualified_name.cmp(&a.qualified_name))
                })
                .expect("non-empty group");

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
    use super::*;

    #[test]
    fn build_undirected_adjacency_includes_both_directions() {
        let src = gitcortex_core::graph::NodeId::new();
        let dst = gitcortex_core::graph::NodeId::new();
        let edges = vec![Edge::new(src.clone(), dst.clone(), EdgeKind::Calls)];
        let adj = build_undirected_adjacency(&edges);
        assert!(adj.get(&src.as_str()).unwrap().contains(&dst.as_str()));
        assert!(adj.get(&dst.as_str()).unwrap().contains(&src.as_str()));
    }

    #[test]
    fn build_undirected_adjacency_excludes_uses_edges() {
        let src = gitcortex_core::graph::NodeId::new();
        let dst = gitcortex_core::graph::NodeId::new();
        let edges = vec![Edge::new(src.clone(), dst, EdgeKind::Uses)];
        let adj = build_undirected_adjacency(&edges);
        assert!(!adj.contains_key(&src.as_str()));
    }
}
