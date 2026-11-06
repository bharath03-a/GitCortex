use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{bail, Result};
use gitcortex_core::{
    graph::{Edge, Node},
    schema::EdgeKind,
    store::GraphStore,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
    Both,
}
impl Direction {
    pub fn as_store(&self) -> &'static str {
        match self {
            Self::Up => "in",
            Self::Down => "out",
            Self::Both => "both",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeOrder {
    Store,
    Deterministic,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExportScope {
    pub seed: Option<String>,
    pub depth: u8,
    pub direction: Direction,
    pub edge_kinds: Option<Vec<EdgeKind>>,
    pub max_nodes: Option<usize>,
    #[serde(skip)]
    pub order: NodeOrder,
}
impl Default for ExportScope {
    fn default() -> Self {
        Self {
            seed: None,
            depth: 2,
            direction: Direction::Both,
            edge_kinds: None,
            max_nodes: Some(5000),
            order: NodeOrder::Deterministic,
        }
    }
}
impl ExportScope {
    pub fn legacy_full() -> Self {
        Self {
            max_nodes: None,
            order: NodeOrder::Store,
            ..Self::default()
        }
    }
}
#[derive(Debug, Clone)]
pub struct ScopedGraph {
    pub branch: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub truncated: bool,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub seed: Option<Node>,
}
impl ScopedGraph {
    /// Construct an explicit graph without consulting any branch or store.
    pub fn from_explicit(branch: String, nodes: Vec<Node>, edges: Vec<Edge>) -> Result<Self> {
        let ids: HashSet<_> = nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &edges {
            if !ids.contains(&edge.src.as_str()) || !ids.contains(&edge.dst.as_str()) {
                bail!("dangling edge: {} -> {}", edge.src, edge.dst);
            }
        }
        Ok(Self {
            branch,
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            nodes,
            edges,
            truncated: false,
            seed: None,
        })
    }
}
#[derive(Debug)]
pub enum SeedError {
    NotFound(String),
    Ambiguous(Vec<Node>),
}
impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "seed not found: {s}"),
            Self::Ambiguous(nodes) => {
                write!(f, "ambiguous seed; candidates:")?;
                for n in nodes {
                    write!(f, "\n  {} ({})", n.qualified_name, n.id)?;
                }
                Ok(())
            }
        }
    }
}
impl std::error::Error for SeedError {}
fn seed_from(nodes: &[Node], seed: &str) -> Result<Node> {
    for level in 0..3 {
        let mut matches: Vec<_> = nodes
            .iter()
            .filter(|n| match level {
                0 => n.id.as_str() == seed,
                1 => n.qualified_name == seed,
                _ => n.name == seed,
            })
            .cloned()
            .collect();
        if matches.len() == 1 {
            return Ok(matches.remove(0));
        }
        if !matches.is_empty() {
            matches.sort_by_key(|n| (n.file.clone(), n.span.start_line, n.id.as_str()));
            return Err(SeedError::Ambiguous(matches).into());
        }
    }
    Err(SeedError::NotFound(seed.into()).into())
}
pub fn resolve_seed(store: &dyn GraphStore, branch: &str, seed: &str) -> Result<Node> {
    seed_from(&store.list_all_nodes(branch)?, seed)
}
pub fn edge_allowed(edge: &Edge, scope: &ExportScope) -> bool {
    scope
        .edge_kinds
        .as_ref()
        .map_or(true, |kinds| kinds.contains(&edge.kind))
}
pub fn resolve_scope(
    store: &dyn GraphStore,
    branch: &str,
    scope: &ExportScope,
) -> Result<ScopedGraph> {
    scope_graph(
        branch,
        store.list_all_nodes(branch)?,
        store.list_all_edges(branch)?,
        scope,
    )
}
fn scope_graph(
    branch: &str,
    mut nodes: Vec<Node>,
    mut edges: Vec<Edge>,
    scope: &ExportScope,
) -> Result<ScopedGraph> {
    if scope.depth > 5 {
        bail!("depth must be at most 5");
    }
    // Legacy rendering is a byte-preserving view of store rows, without
    // sorting, capping, or endpoint pruning.
    if scope.seed.is_none()
        && scope.edge_kinds.is_none()
        && scope.max_nodes.is_none()
        && scope.order == NodeOrder::Store
    {
        return Ok(ScopedGraph {
            branch: branch.into(),
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            nodes,
            edges,
            truncated: false,
            seed: None,
        });
    }
    let seed = scope
        .seed
        .as_ref()
        .map(|s| seed_from(&nodes, s))
        .transpose()?;
    edges.retain(|e| edge_allowed(e, scope));
    let mut distances: Option<HashMap<String, u8>> = None;
    if let Some(seed) = &seed {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        for e in &edges {
            if scope.direction != Direction::Up {
                adjacency
                    .entry(e.src.as_str())
                    .or_default()
                    .push(e.dst.as_str());
            }
            if scope.direction != Direction::Down {
                adjacency
                    .entry(e.dst.as_str())
                    .or_default()
                    .push(e.src.as_str());
            }
        }
        let mut dist = HashMap::from([(seed.id.as_str(), 0u8)]);
        let mut queue = VecDeque::from([(seed.id.as_str(), 0u8)]);
        while let Some((id, depth)) = queue.pop_front() {
            if depth >= scope.depth {
                continue;
            }
            if let Some(next) = adjacency.get(&id) {
                for next_id in next {
                    if !dist.contains_key(next_id) {
                        dist.insert(next_id.clone(), depth + 1);
                        queue.push_back((next_id.clone(), depth + 1));
                    }
                }
            }
        }
        nodes.retain(|n| dist.contains_key(&n.id.as_str()));
        distances = Some(dist);
    }
    let ids: HashSet<_> = nodes.iter().map(|n| n.id.as_str()).collect();
    edges.retain(|e| ids.contains(&e.src.as_str()) && ids.contains(&e.dst.as_str()));
    let total_nodes = nodes.len();
    let total_edges = edges.len();
    if scope.order == NodeOrder::Deterministic {
        nodes.sort_by_key(|n| (n.file.clone(), n.span.start_line, n.id.as_str()));
        edges.sort_by_key(|e| {
            (
                e.src.as_str(),
                e.dst.as_str(),
                e.kind.to_string(),
                e.line,
                format!("{:?}", e.confidence),
            )
        });
    }
    if let Some(cap) = scope.max_nodes {
        if nodes.len() > cap {
            if let Some(dist) = &distances {
                match scope.order {
                    NodeOrder::Deterministic => {
                        nodes.sort_by_key(|n| {
                            (
                                dist.get(&n.id.as_str()).copied().unwrap_or(u8::MAX),
                                n.file.clone(),
                                n.span.start_line,
                                n.id.as_str(),
                            )
                        });
                        nodes.truncate(cap);
                        nodes.sort_by_key(|n| (n.file.clone(), n.span.start_line, n.id.as_str()));
                    }
                    NodeOrder::Store => {
                        let mut indexed: Vec<(usize, Node)> =
                            nodes.into_iter().enumerate().collect();
                        indexed.sort_by_key(|(idx, n)| {
                            (dist.get(&n.id.as_str()).copied().unwrap_or(u8::MAX), *idx)
                        });
                        indexed.truncate(cap);
                        indexed.sort_by_key(|(idx, _)| *idx);
                        nodes = indexed.into_iter().map(|(_, n)| n).collect();
                    }
                }
            } else {
                nodes.truncate(cap);
            }
        }
    }
    let ids: HashSet<_> = nodes.iter().map(|n| n.id.as_str()).collect();
    edges.retain(|e| ids.contains(&e.src.as_str()) && ids.contains(&e.dst.as_str()));
    Ok(ScopedGraph {
        branch: branch.into(),
        truncated: nodes.len() < total_nodes,
        nodes,
        edges,
        total_nodes,
        total_edges,
        seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcortex_core::{
        graph::{NodeId, Span},
        schema::NodeKind,
    };
    fn node(name: &str, line: u32) -> Node {
        Node {
            id: NodeId::stable(name),
            name: name.into(),
            qualified_name: format!("test::{name}"),
            file: "sample.rs".into(),
            span: Span {
                start_line: line,
                end_line: line,
            },
            kind: NodeKind::Function,
            metadata: Default::default(),
        }
    }
    fn fixture() -> (Vec<Node>, Vec<Edge>) {
        let nodes = vec![node("a", 3), node("b", 2), node("c", 1)];
        let edges = vec![
            Edge::new(nodes[0].id.clone(), nodes[1].id.clone(), EdgeKind::Imports),
            Edge::new(nodes[1].id.clone(), nodes[2].id.clone(), EdgeKind::Calls),
        ];
        (nodes, edges)
    }
    fn scoped(seed: &str, direction: Direction, kinds: Option<Vec<EdgeKind>>) -> ScopedGraph {
        let (nodes, edges) = fixture();
        scope_graph(
            "main",
            nodes,
            edges,
            &ExportScope {
                seed: Some(seed.into()),
                direction,
                edge_kinds: kinds,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn resolve_seed_prefers_id_then_qualified_then_name() {
        let (mut nodes, _) = fixture();
        nodes[1].name = nodes[0].id.as_str();
        assert_eq!(
            seed_from(&nodes, &nodes[0].id.as_str()).unwrap().id,
            nodes[0].id
        );
        nodes[1].name = nodes[0].qualified_name.clone();
        assert_eq!(
            seed_from(&nodes, &nodes[0].qualified_name).unwrap().id,
            nodes[0].id
        );
        assert_eq!(seed_from(&nodes, "a").unwrap().id, nodes[0].id);
        assert!(seed_from(&nodes, "missing").unwrap_err().is::<SeedError>());
    }
    #[test]
    fn resolve_seed_ambiguous() {
        let (mut nodes, _) = fixture();
        nodes[1].name = "a".into();
        assert!(
            matches!(seed_from(&nodes,"a").unwrap_err().downcast_ref::<SeedError>(),Some(SeedError::Ambiguous(n)) if n.len()==2)
        );
    }
    #[test]
    fn up_returns_callers_only() {
        assert_eq!(
            scoped("b", Direction::Up, None)
                .nodes
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }
    #[test]
    fn down_returns_callees_only() {
        assert_eq!(
            scoped("b", Direction::Down, None)
                .nodes
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "b"]
        );
    }
    #[test]
    fn edge_kind_filter_applies_during_traversal() {
        assert_eq!(
            scoped("a", Direction::Down, Some(vec![EdgeKind::Calls]))
                .nodes
                .len(),
            1
        );
        assert_eq!(
            scoped("a", Direction::Down, Some(vec![EdgeKind::Imports]))
                .nodes
                .len(),
            2
        );
    }
    #[test]
    fn cap_is_deterministic() {
        let (nodes, edges) = fixture();
        let mut reversed = nodes.clone();
        reversed.reverse();
        let scope = ExportScope {
            max_nodes: Some(2),
            ..Default::default()
        };
        let a = scope_graph("main", nodes, edges.clone(), &scope).unwrap();
        let b = scope_graph("main", reversed, edges, &scope).unwrap();
        assert_eq!(a.nodes, b.nodes);
        assert_eq!(a.edges, b.edges);
        assert!(a.truncated);
        assert_eq!((a.total_nodes, a.total_edges), (3, 2));
    }
    #[test]
    fn cap_keeps_seed() {
        let (nodes, edges) = fixture();
        let g = scope_graph(
            "main",
            nodes,
            edges,
            &ExportScope {
                seed: Some("a".into()),
                depth: 2,
                max_nodes: Some(1),
                edge_kinds: None,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(g.truncated);
        assert_eq!(g.nodes.len(), 1);
        assert_eq!(g.nodes[0].name, "a");
    }
    #[test]
    fn cap_prefers_closer_hops() {
        let (nodes, edges) = fixture();
        let g = scope_graph(
            "main",
            nodes,
            edges,
            &ExportScope {
                seed: Some("a".into()),
                depth: 2,
                max_nodes: Some(2),
                edge_kinds: None,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(g.truncated);
        let names: Vec<_> = g.nodes.iter().map(|n| n.name.as_str()).collect();
        // Seed `a` (depth 0) and `b` (depth 1) kept over `c` (depth 2),
        // then sorted deterministically by (file, start_line, id).
        assert_eq!(names, vec!["b", "a"]);
    }
    #[test]
    fn legacy_full_preserves_store_order() {
        let (nodes, edges) = fixture();
        let g = scope_graph(
            "main",
            nodes.clone(),
            edges.clone(),
            &ExportScope::legacy_full(),
        )
        .unwrap();
        assert_eq!(g.nodes, nodes);
        assert_eq!(g.edges, edges);
    }
    #[test]
    fn legacy_full_scope_uncapped_over_5000() {
        let nodes: Vec<_> = (0..5002).map(|i| node(&i.to_string(), i)).collect();
        let g = scope_graph("main", nodes, vec![], &ExportScope::legacy_full()).unwrap();
        assert_eq!(g.nodes.len(), 5002);
        assert!(!g.truncated);
    }
    #[test]
    fn from_explicit_keeps_unresolvable_ids() {
        let n = node("deleted", 1);
        let g = ScopedGraph::from_explicit("head".into(), vec![n.clone()], vec![]).unwrap();
        assert_eq!(g.nodes, vec![n]);
        assert!(!g.truncated);
    }
    #[test]
    fn from_explicit_rejects_dangling_edge() {
        let (nodes, edges) = fixture();
        assert!(ScopedGraph::from_explicit("main".into(), vec![nodes[0].clone()], edges).is_err());
    }
}
