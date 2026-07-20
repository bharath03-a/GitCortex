use std::path::Path;

use crate::{
    error::Result,
    graph::{Edge, GraphDiff, Node},
    schema::{EdgeConfidence, EdgeKind, NodeKind, Visibility},
};

/// Structural predicate set for `search_by_attributes`. All fields are
/// optional; a `None` field imposes no constraint. Set fields are ANDed.
#[derive(Debug, Default, Clone)]
pub struct AttributeFilter {
    pub kind: Option<NodeKind>,
    pub is_async: Option<bool>,
    pub visibility: Option<Visibility>,
    /// Inclusive lower bound on cyclomatic complexity. Nodes without a recorded
    /// complexity never match a complexity bound.
    pub min_complexity: Option<u32>,
    /// Inclusive upper bound on cyclomatic complexity.
    pub max_complexity: Option<u32>,
    /// Case-insensitive substring the node name must contain.
    pub name_contains: Option<String>,
    /// Case-insensitive: the node must carry an annotation/decorator whose name
    /// contains this string (e.g. "route" matches `@app.route`, "Test" matches
    /// `@Test`).
    pub annotation: Option<String>,
}

impl AttributeFilter {
    /// True when every set predicate holds for `node`.
    pub fn matches(&self, node: &Node) -> bool {
        if let Some(k) = &self.kind {
            if &node.kind != k {
                return false;
            }
        }
        if let Some(a) = self.is_async {
            if node.metadata.is_async != a {
                return false;
            }
        }
        if let Some(v) = &self.visibility {
            if &node.metadata.visibility != v {
                return false;
            }
        }
        if let Some(min) = self.min_complexity {
            match node.metadata.lld.complexity {
                Some(c) if c >= min => {}
                _ => return false,
            }
        }
        if let Some(max) = self.max_complexity {
            match node.metadata.lld.complexity {
                Some(c) if c <= max => {}
                _ => return false,
            }
        }
        if let Some(sub) = &self.name_contains {
            if !node
                .name
                .to_ascii_lowercase()
                .contains(&sub.to_ascii_lowercase())
            {
                return false;
            }
        }
        if let Some(ann) = &self.annotation {
            let needle = ann.to_ascii_lowercase();
            if !node
                .metadata
                .annotations
                .iter()
                .any(|a| a.to_ascii_lowercase().contains(&needle))
            {
                return false;
            }
        }
        true
    }

    /// True when no predicate is set — an unconstrained filter.
    pub fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.is_async.is_none()
            && self.visibility.is_none()
            && self.min_complexity.is_none()
            && self.max_complexity.is_none()
            && self.name_contains.is_none()
            && self.annotation.is_none()
    }
}

/// A subgraph centred on a seed node, returned by `get_subgraph`.
pub struct SubGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Callers of a symbol grouped by hop distance.
pub struct CallersDeep {
    /// Groups indexed 0..depth-1. `hops[0]` = direct callers (hop 1).
    pub hops: Vec<Vec<Node>>,
    /// Risk score derived from total affected count and depth reached.
    pub risk_level: &'static str,
}

/// Aggregate counts for a branch's graph, returned by `graph_stats`.
/// First-call orientation for an agent: how big is the graph, what kinds of
/// symbols dominate, how connected is it.
pub struct GraphStats {
    pub total_nodes: u64,
    pub total_edges: u64,
    /// `(kind, count)` pairs, sorted by count descending.
    pub nodes_by_kind: Vec<(String, u64)>,
    /// `(kind, count)` pairs, sorted by count descending.
    pub edges_by_kind: Vec<(String, u64)>,
}

/// A single call site: the calling symbol and the source line of the call.
pub struct CallSite {
    pub caller: Node,
    /// 1-indexed line of the call expression, when recorded.
    pub line: Option<u32>,
}

/// Up-and-down type relationships for a named type, returned by `type_hierarchy`.
pub struct TypeHierarchy {
    /// Types this type implements or extends (its supertypes / interfaces).
    pub supertypes: Vec<Node>,
    /// Types that implement or extend this type (its subtypes / implementors).
    pub subtypes: Vec<Node>,
}

/// 360-degree view of a single symbol.
pub struct SymbolContext {
    /// The node matching `name` (first match if multiple).
    pub definition: Node,
    /// Functions/methods that call this symbol (direct callers).
    pub callers: Vec<Node>,
    /// Functions/methods that this symbol calls (direct callees).
    pub callees: Vec<Node>,
    /// Functions/types that reference this symbol via `Uses` edges.
    pub used_by: Vec<Node>,
}

/// Backend-agnostic interface for the knowledge graph store.
///
/// The v0.1 implementation is `KuzuGraphStore` (local embedded DB).
/// A remote backend can be plugged in by implementing this trait without
/// touching the indexer or MCP layers.
pub trait GraphStore: Send + Sync {
    // ── Write operations ─────────────────────────────────────────────────────

    /// Apply an incremental diff to the named branch's graph.
    fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()>;

    // ── Read operations ──────────────────────────────────────────────────────

    /// Find all nodes matching `name` on `branch`.
    /// When `fuzzy` is true, matches any node whose name *contains* `name`
    /// (case-sensitive substring). When false, exact match only.
    fn lookup_symbol(&self, branch: &str, name: &str, fuzzy: bool) -> Result<Vec<Node>>;

    /// Find all call-site nodes whose outgoing `Calls` edge points to a node
    /// named `function_name` on `branch` (single hop).
    fn find_callers(&self, branch: &str, function_name: &str) -> Result<Vec<Node>>;

    /// Like `find_callers` but also returns each caller's edge confidence.
    /// The default conservatively marks all callers as `Inferred`; backends
    /// should override with a query that reads `e.confidence` from the edge.
    fn find_callers_with_confidence(
        &self,
        branch: &str,
        function_name: &str,
    ) -> Result<Vec<(Node, EdgeConfidence)>> {
        Ok(self
            .find_callers(branch, function_name)?
            .into_iter()
            .map(|n| (n, EdgeConfidence::Inferred))
            .collect())
    }

    /// Find direct callers for one exact target node ID, including edge
    /// confidence. Agent-facing queries use this after symbol disambiguation so
    /// common names such as `get` cannot merge unrelated call graphs.
    ///
    /// The default implementation filters the full graph in memory. Stores
    /// should override this with an indexed query.
    fn find_callers_by_id_with_confidence(
        &self,
        branch: &str,
        target_id: &str,
    ) -> Result<Vec<(Node, EdgeConfidence)>> {
        let callers: std::collections::HashMap<String, Node> = self
            .list_all_nodes(branch)?
            .into_iter()
            .map(|n| (n.id.as_str(), n))
            .collect();
        let mut out = Vec::new();
        for edge in self.list_all_edges(branch)? {
            if edge.kind == EdgeKind::Calls && edge.dst.as_str() == target_id {
                if let Some(node) = callers.get(&edge.src.as_str()) {
                    out.push((node.clone(), edge.confidence));
                }
            }
        }
        Ok(out)
    }

    /// Multi-hop BFS: find callers up to `depth` hops away.
    /// Returns callers grouped by hop distance (1..=depth).
    /// `depth` is capped at 5 to prevent runaway queries.
    fn find_callers_deep(
        &self,
        branch: &str,
        function_name: &str,
        depth: u8,
    ) -> Result<CallersDeep>;

    /// Return a 360° view of a symbol: its definition, direct callers,
    /// direct callees, and nodes that reference it via `Uses` edges.
    fn symbol_context(&self, branch: &str, name: &str) -> Result<SymbolContext>;

    /// List all top-level definitions in `file` on `branch`.
    fn list_definitions(&self, branch: &str, file: &Path) -> Result<Vec<Node>>;

    /// Return all nodes in `branch`'s graph.
    fn list_all_nodes(&self, branch: &str) -> Result<Vec<Node>>;

    /// Return all edges in `branch`'s graph.
    fn list_all_edges(&self, branch: &str) -> Result<Vec<Edge>>;

    /// Return a deterministic page of nodes ordered by stable node ID.
    ///
    /// The default implementation slices `list_all_nodes` for compatibility.
    /// Stores should override this with query-level `OFFSET`/`LIMIT` push-down
    /// so visualization clients can progressively load very large graphs.
    fn list_nodes_page(&self, branch: &str, offset: usize, limit: usize) -> Result<Vec<Node>> {
        let mut nodes = self.list_all_nodes(branch)?;
        nodes.sort_by_key(|node| node.id.as_str());
        Ok(nodes.into_iter().skip(offset).take(limit).collect())
    }

    /// Return a deterministic page of edges ordered by source, destination,
    /// kind, and source line. See [`GraphStore::list_nodes_page`].
    fn list_edges_page(&self, branch: &str, offset: usize, limit: usize) -> Result<Vec<Edge>> {
        let mut edges = self.list_all_edges(branch)?;
        edges.sort_by_key(|edge| {
            (
                edge.src.as_str(),
                edge.dst.as_str(),
                edge.kind.to_string(),
                edge.line,
            )
        });
        Ok(edges.into_iter().skip(offset).take(limit).collect())
    }

    /// Return edges of a specific `kind` in `branch`'s graph.
    /// The default filters `list_all_edges` in-memory; backends should override
    /// with a `WHERE`-clause push-down for large graphs.
    fn list_edges_by_kind(&self, branch: &str, kind: EdgeKind) -> Result<Vec<Edge>> {
        Ok(self
            .list_all_edges(branch)?
            .into_iter()
            .filter(|e| e.kind == kind)
            .collect())
    }

    /// Find nodes matching a structural `filter` (kind, async, visibility,
    /// complexity range, name substring), up to `limit` results.
    ///
    /// The default filters `list_all_nodes` in-memory; backends should override
    /// with a `WHERE`-clause push-down.
    fn search_by_attributes(
        &self,
        branch: &str,
        filter: &AttributeFilter,
        limit: usize,
    ) -> Result<Vec<Node>> {
        let mut nodes: Vec<Node> = self
            .list_all_nodes(branch)?
            .into_iter()
            .filter(|n| filter.matches(n))
            .collect();
        nodes.truncate(limit);
        Ok(nodes)
    }

    /// Aggregate node/edge counts (total + per-kind) for `branch`.
    ///
    /// The default counts in-memory from `list_all_nodes`/`list_all_edges`;
    /// backends should override with a `COUNT` push-down.
    fn graph_stats(&self, branch: &str) -> Result<GraphStats> {
        use std::collections::HashMap;

        fn tally<T, F>(items: &[T], key: F) -> Vec<(String, u64)>
        where
            F: Fn(&T) -> String,
        {
            let mut counts: HashMap<String, u64> = HashMap::new();
            for item in items {
                *counts.entry(key(item)).or_insert(0) += 1;
            }
            let mut pairs: Vec<(String, u64)> = counts.into_iter().collect();
            // Sort by count desc, then kind name asc for deterministic output.
            pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            pairs
        }

        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;
        Ok(GraphStats {
            total_nodes: nodes.len() as u64,
            total_edges: edges.len() as u64,
            nodes_by_kind: tally(&nodes, |n| n.kind.to_string()),
            edges_by_kind: tally(&edges, |e| e.kind.to_string()),
        })
    }

    /// Return nodes whose `name` or `qualified_name` contains `query` (case-
    /// sensitive substring), up to `limit` results. Implementations should push
    /// the filter to the store rather than scanning all nodes in memory.
    ///
    /// The default falls back to `list_all_nodes` for stores that don't
    /// override this method (e.g. the in-memory test stub).
    fn search_nodes(&self, branch: &str, query: &str, limit: usize) -> Result<Vec<Node>> {
        let q = query.to_ascii_lowercase();
        let mut nodes: Vec<Node> = self
            .list_all_nodes(branch)?
            .into_iter()
            .filter(|n| {
                n.name.to_ascii_lowercase().contains(&q)
                    || n.qualified_name.to_ascii_lowercase().contains(&q)
            })
            .collect();
        nodes.truncate(limit);
        Ok(nodes)
    }

    /// Resolve a set of node IDs to full nodes. Order is not guaranteed; IDs
    /// that don't exist on `branch` are silently skipped.
    ///
    /// The default falls back to `list_all_nodes`; backends should override
    /// with an indexed ID lookup.
    fn get_nodes_by_ids(&self, branch: &str, ids: &[String]) -> Result<Vec<Node>> {
        let idset: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        Ok(self
            .list_all_nodes(branch)?
            .into_iter()
            .filter(|n| idset.contains(n.id.as_str().as_str()))
            .collect())
    }

    /// Return the graph delta between two branches as a `GraphDiff`.
    /// Nodes/edges present in `to` but not `from` are in `added_*`.
    /// Nodes/edges present in `from` but not `to` are in `removed_*`.
    fn branch_diff(&self, from: &str, to: &str) -> Result<GraphDiff>;

    // ── Wave 2 tools ─────────────────────────────────────────────────────────

    /// Find all functions/methods called by `function_name` up to `depth` hops.
    /// Returns callees grouped by hop distance (1..=depth). Capped at 5.
    fn find_callees(&self, branch: &str, function_name: &str, depth: u8) -> Result<CallersDeep>;

    /// Find all structs/classes that implement/inherit `trait_or_interface_name`.
    fn find_implementors(&self, branch: &str, trait_or_interface_name: &str) -> Result<Vec<Node>>;

    /// Return the in-repo modules that a module named `module_name` depends on,
    /// resolved by following its `Imports` edges to the defining module of each
    /// imported symbol. Answers "what does this module depend on".
    ///
    /// External/stdlib imports are not graphed, so only intra-repo dependencies
    /// appear. The default walks nodes + edges in-memory; backends should
    /// override with a join query.
    fn module_dependencies(&self, branch: &str, module_name: &str) -> Result<Vec<Node>> {
        use crate::schema::{EdgeKind, NodeKind};
        use std::collections::{HashMap, HashSet};

        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;

        // Source module node ids (there may be several files with this stem).
        let src_ids: HashSet<String> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module && n.name == module_name)
            .map(|n| n.id.as_str())
            .collect();
        if src_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Map every node id to its file, and every file to its module node.
        let id_to_file: HashMap<String, String> = nodes
            .iter()
            .map(|n| (n.id.as_str(), n.file.to_string_lossy().into_owned()))
            .collect();
        let file_to_module: HashMap<String, &Node> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module)
            .map(|n| (n.file.to_string_lossy().into_owned(), n))
            .collect();

        let src_files: HashSet<&String> =
            src_ids.iter().filter_map(|id| id_to_file.get(id)).collect();

        let mut seen: HashSet<String> = HashSet::new();
        let mut deps: Vec<Node> = Vec::new();
        for e in &edges {
            if !matches!(e.kind, EdgeKind::Imports) {
                continue;
            }
            if !src_ids.contains(&e.src.as_str()) {
                continue;
            }
            // Resolve the imported symbol's defining module.
            let Some(sym_file) = id_to_file.get(&e.dst.as_str()) else {
                continue;
            };
            // Skip self-imports within the same module file.
            if src_files.contains(sym_file) {
                continue;
            }
            if let Some(dst_mod) = file_to_module.get(sym_file) {
                if seen.insert(dst_mod.id.as_str()) {
                    deps.push((*dst_mod).clone());
                }
            }
        }
        Ok(deps)
    }

    /// Find functions/methods that reference a type named `type_name` as a
    /// parameter or return type (following `Uses` edges). Answers "where is
    /// type T used in a signature" — the type-level analogue of find_callers.
    ///
    /// The default walks `list_all_edges`; backends should override with a
    /// directed Cypher match.
    fn find_type_usages(&self, branch: &str, type_name: &str) -> Result<Vec<Node>> {
        use crate::schema::EdgeKind;
        use std::collections::HashSet;

        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;

        let target_ids: HashSet<String> = nodes
            .iter()
            .filter(|n| n.name == type_name)
            .map(|n| n.id.as_str())
            .collect();
        if target_ids.is_empty() {
            return Ok(Vec::new());
        }

        let user_ids: Vec<String> = edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Uses) && target_ids.contains(&e.dst.as_str()))
            .map(|e| e.src.as_str())
            .collect();
        self.get_nodes_by_ids(branch, &user_ids)
    }

    /// Find every call site of the function named `function_name`: the calling
    /// symbol plus the source line of each call expression (following `Calls`
    /// edges). Where `find_callers` returns only the calling symbols, this also
    /// pinpoints the line each call happens on.
    ///
    /// The default walks `list_all_edges`; backends should override with a
    /// directed Cypher match that returns the edge line.
    fn find_call_sites(&self, branch: &str, function_name: &str) -> Result<Vec<CallSite>> {
        use crate::schema::EdgeKind;
        use std::collections::HashMap;

        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;

        let target_ids: std::collections::HashSet<String> = nodes
            .iter()
            .filter(|n| n.name == function_name)
            .map(|n| n.id.as_str())
            .collect();
        if target_ids.is_empty() {
            return Ok(Vec::new());
        }

        let by_id: HashMap<String, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        let mut sites = Vec::new();
        for e in &edges {
            if matches!(e.kind, EdgeKind::Calls) && target_ids.contains(&e.dst.as_str()) {
                if let Some(caller) = by_id.get(&e.src.as_str()) {
                    sites.push(CallSite {
                        caller: (*caller).clone(),
                        line: e.line,
                    });
                }
            }
        }
        Ok(sites)
    }

    /// Find the module/file nodes that import a symbol named `symbol_name`
    /// (following `Imports` edges). Answers "who imports X".
    ///
    /// The default walks `list_all_edges`; backends should override with a
    /// directed Cypher match.
    fn find_importers(&self, branch: &str, symbol_name: &str) -> Result<Vec<Node>> {
        use crate::schema::EdgeKind;
        use std::collections::HashSet;

        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;

        let target_ids: HashSet<String> = nodes
            .iter()
            .filter(|n| n.name == symbol_name)
            .map(|n| n.id.as_str())
            .collect();
        if target_ids.is_empty() {
            return Ok(Vec::new());
        }

        let importer_ids: Vec<String> = edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Imports) && target_ids.contains(&e.dst.as_str()))
            .map(|e| e.src.as_str())
            .collect();
        self.get_nodes_by_ids(branch, &importer_ids)
    }

    /// Return both directions of the type relation for `name`: the types it
    /// implements/extends (supertypes) and the types that implement/extend it
    /// (subtypes), following `Implements` and `Inherits` edges.
    ///
    /// The default walks `list_all_edges`; backends should override with a
    /// directed Cypher match.
    fn type_hierarchy(&self, branch: &str, name: &str) -> Result<TypeHierarchy> {
        use crate::schema::EdgeKind;
        use std::collections::HashSet;

        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;

        let self_ids: HashSet<String> = nodes
            .iter()
            .filter(|n| n.name == name)
            .map(|n| n.id.as_str())
            .collect();
        if self_ids.is_empty() {
            return Ok(TypeHierarchy {
                supertypes: Vec::new(),
                subtypes: Vec::new(),
            });
        }

        let is_hierarchy = |k: &EdgeKind| matches!(k, EdgeKind::Implements | EdgeKind::Inherits);
        let mut super_ids: Vec<String> = Vec::new();
        let mut sub_ids: Vec<String> = Vec::new();
        for e in &edges {
            if !is_hierarchy(&e.kind) {
                continue;
            }
            // self → super
            if self_ids.contains(&e.src.as_str()) {
                super_ids.push(e.dst.as_str());
            }
            // sub → self
            if self_ids.contains(&e.dst.as_str()) {
                sub_ids.push(e.src.as_str());
            }
        }

        Ok(TypeHierarchy {
            supertypes: self.get_nodes_by_ids(branch, &super_ids)?,
            subtypes: self.get_nodes_by_ids(branch, &sub_ids)?,
        })
    }

    /// Find all call paths between `from` and `to` using BFS.
    /// Returns at most one path (the shortest), as a sequence of nodes.
    fn trace_path(&self, branch: &str, from: &str, to: &str) -> Result<Vec<Node>>;

    /// Find all nodes in `file` whose span overlaps `[start_line, end_line]`.
    fn list_symbols_in_range(
        &self,
        branch: &str,
        file: &Path,
        start_line: u32,
        end_line: u32,
    ) -> Result<Vec<Node>>;

    /// Find symbols with no incoming Calls or Uses edges (potential dead code).
    /// If `kind` is provided, filters to only that NodeKind.
    fn find_unused_symbols(&self, branch: &str, kind: Option<NodeKind>) -> Result<Vec<Node>>;

    /// Return a subgraph centred on `seed_name` up to `depth` hops.
    /// `direction`: "in" (callers), "out" (callees), or "both".
    fn get_subgraph(
        &self,
        branch: &str,
        seed_name: &str,
        depth: u8,
        direction: &str,
    ) -> Result<SubGraph>;

    /// Return a subgraph around one exact seed node ID. Agent-facing queries
    /// use this after disambiguation so repeated short names cannot merge
    /// unrelated neighborhoods.
    ///
    /// The default scans the graph in memory. Stores should override this with
    /// indexed traversal.
    fn get_subgraph_by_id(
        &self,
        branch: &str,
        seed_id: &str,
        depth: u8,
        direction: &str,
    ) -> Result<SubGraph> {
        let nodes = self.list_all_nodes(branch)?;
        let edges = self.list_all_edges(branch)?;
        let by_id: std::collections::HashMap<String, Node> = nodes
            .into_iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        if !by_id.contains_key(seed_id) {
            return Ok(SubGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            });
        }

        let mut selected: std::collections::HashSet<String> =
            [seed_id.to_owned()].into_iter().collect();
        let mut frontier = vec![seed_id.to_owned()];
        for _ in 0..depth.min(5) {
            let mut next = Vec::new();
            for edge in &edges {
                let src = edge.src.as_str();
                let dst = edge.dst.as_str();
                let neighbour = if (direction == "out" || direction == "both")
                    && frontier.contains(&src)
                {
                    Some(dst)
                } else if (direction == "in" || direction == "both") && frontier.contains(&dst) {
                    Some(src)
                } else {
                    None
                };
                if let Some(id) = neighbour {
                    if selected.insert(id.clone()) {
                        next.push(id);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }

        let selected_edges = edges
            .into_iter()
            .filter(|edge| {
                selected.contains(&edge.src.as_str()) && selected.contains(&edge.dst.as_str())
            })
            .collect();
        let selected_nodes = selected
            .into_iter()
            .filter_map(|id| by_id.get(&id).cloned())
            .collect();
        Ok(SubGraph {
            nodes: selected_nodes,
            edges: selected_edges,
        })
    }

    /// Return one bounded hop around an exact seed ID. Visualization clients
    /// use repeated calls to expand large graphs without materializing an
    /// unbounded multi-hop neighborhood.
    fn get_neighborhood_by_id(
        &self,
        branch: &str,
        seed_id: &str,
        direction: &str,
        limit: usize,
    ) -> Result<SubGraph> {
        let all_nodes = self.list_all_nodes(branch)?;
        let by_id: std::collections::HashMap<String, Node> = all_nodes
            .into_iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        if !by_id.contains_key(seed_id) {
            return Ok(SubGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            });
        }
        let mut edges = Vec::new();
        for edge in self.list_all_edges(branch)? {
            let incoming = edge.dst.as_str() == seed_id;
            let outgoing = edge.src.as_str() == seed_id;
            if ((direction == "in" && incoming)
                || (direction == "out" && outgoing)
                || (direction == "both" && (incoming || outgoing)))
                && edges.len() < limit
            {
                edges.push(edge);
            }
        }
        let mut ids = std::collections::HashSet::from([seed_id.to_owned()]);
        for edge in &edges {
            ids.insert(edge.src.as_str());
            ids.insert(edge.dst.as_str());
        }
        let nodes = ids
            .into_iter()
            .filter_map(|id| by_id.get(&id).cloned())
            .collect();
        Ok(SubGraph { nodes, edges })
    }

    // ── Indexing state ───────────────────────────────────────────────────────

    /// Last commit SHA successfully indexed for `branch`. `None` if the branch
    /// has never been indexed.
    fn last_indexed_sha(&self, branch: &str) -> Result<Option<String>>;

    /// Persist the commit SHA after a successful index run.
    fn set_last_indexed_sha(&mut self, branch: &str, sha: &str) -> Result<()>;
}
