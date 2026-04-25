use std::path::Path;

use crate::{
    error::Result,
    graph::{GraphDiff, Node},
};

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

    /// Find all nodes matching `name` (exact, case-sensitive) on `branch`.
    fn lookup_symbol(&self, branch: &str, name: &str) -> Result<Vec<Node>>;

    /// Find all call-site nodes whose outgoing `Calls` edge points to a node
    /// named `function_name` on `branch`.
    fn find_callers(&self, branch: &str, function_name: &str) -> Result<Vec<Node>>;

    /// List all top-level definitions in `file` on `branch`.
    fn list_definitions(&self, branch: &str, file: &Path) -> Result<Vec<Node>>;

    /// Return the graph delta between two branches as a `GraphDiff`.
    /// Nodes/edges present in `to` but not `from` are in `added_*`.
    /// Nodes/edges present in `from` but not `to` are in `removed_*`.
    fn branch_diff(&self, from: &str, to: &str) -> Result<GraphDiff>;

    // ── Indexing state ───────────────────────────────────────────────────────

    /// Last commit SHA successfully indexed for `branch`. `None` if the branch
    /// has never been indexed.
    fn last_indexed_sha(&self, branch: &str) -> Result<Option<String>>;

    /// Persist the commit SHA after a successful index run.
    fn set_last_indexed_sha(&mut self, branch: &str, sha: &str) -> Result<()>;
}
