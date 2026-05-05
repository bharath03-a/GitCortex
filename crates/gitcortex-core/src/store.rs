use std::path::Path;

use crate::{
    error::Result,
    graph::{Edge, GraphDiff, Node},
    schema::NodeKind,
};

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

    // ── Indexing state ───────────────────────────────────────────────────────

    /// Last commit SHA successfully indexed for `branch`. `None` if the branch
    /// has never been indexed.
    fn last_indexed_sha(&self, branch: &str) -> Result<Option<String>>;

    /// Persist the commit SHA after a successful index run.
    fn set_last_indexed_sha(&mut self, branch: &str, sha: &str) -> Result<()>;
}
