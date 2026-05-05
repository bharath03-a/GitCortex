use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::GitCortexError,
    schema::{CodeSmell, DesignPattern, EdgeKind, NodeKind, SolidHint, Visibility},
};

// ── Identifiers ──────────────────────────────────────────────────────────────

/// Stable, globally unique node identifier. UUID v4 assigned at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<&str> for NodeId {
    type Error = GitCortexError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Uuid::parse_str(s)
            .map(NodeId)
            .map_err(|e| GitCortexError::Store(format!("invalid NodeId '{s}': {e}")))
    }
}

// ── Source location ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub end_line: u32,
}

// ── LLD metadata ──────────────────────────────────────────────────────────────

/// LLD annotations added during pass-2 analysis. All fields are optional because
/// pass 2 runs asynchronously — nodes are queryable before annotations arrive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LldLabels {
    pub solid_hints: Vec<SolidHint>,
    pub patterns: Vec<DesignPattern>,
    pub smells: Vec<CodeSmell>,
    /// Cyclomatic complexity (functions/methods only).
    pub complexity: Option<u32>,
}

/// Per-node metadata collected during pass-1 (structural) indexing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodeMetadata {
    /// Lines of code for this node's body.
    pub loc: u32,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_unsafe: bool,
    /// Java `static`, Python `@staticmethod`, Go package-level functions.
    pub is_static: bool,
    /// Java/TypeScript `abstract`, Python NotImplemented stubs, sealed traits.
    pub is_abstract: bool,
    /// Java `final` class/method, Rust sealed types, TypeScript `readonly`.
    pub is_final: bool,
    /// Python `@property`, TypeScript getter/setter, Rust associated `const`.
    pub is_property: bool,
    /// Python generators (`yield`), TypeScript `function*`, async generators.
    pub is_generator: bool,
    /// Rust `const fn`, TypeScript `const` assertion, Java `static final` fields.
    pub is_const: bool,
    /// Captured generic constraints, e.g. `["T: Send", "T: 'static"]` or
    /// `["T extends Base", "K extends keyof T"]`.
    pub generic_bounds: Vec<String>,
    /// Pass-2 LLD annotations. Empty until pass 2 runs.
    pub lld: LldLabels,
}

// ── Core graph types ──────────────────────────────────────────────────────────

/// A single named entity in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    /// Short unqualified name (e.g. `"greet"`, not `"Person::greet"`).
    pub name: String,
    /// Qualified path within the module hierarchy (e.g. `"crate::person::Person::greet"`).
    pub qualified_name: String,
    /// Repo-relative path to the source file.
    pub file: PathBuf,
    pub span: Span,
    pub metadata: NodeMetadata,
}

/// A directed relationship between two nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub src: NodeId,
    pub dst: NodeId,
    pub kind: EdgeKind,
}

// ── Graph diff ────────────────────────────────────────────────────────────────

/// Incremental change set produced by the indexer after each commit.
/// Applying a `GraphDiff` to the store brings the persisted graph up to date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GraphDiff {
    pub added_nodes: Vec<Node>,
    /// Explicit node IDs to remove (e.g. from a targeted replacement).
    pub removed_node_ids: Vec<NodeId>,
    /// Files that were deleted. The store removes all nodes whose `file`
    /// field matches any path in this list. Preferred over `removed_node_ids`
    /// when whole files are gone because the indexer does not need to know
    /// prior node IDs (keeping indexer ↔ store decoupled).
    pub removed_files: Vec<PathBuf>,
    pub added_edges: Vec<Edge>,
    pub removed_edges: Vec<(NodeId, NodeId, EdgeKind)>,
    /// Cross-file calls that couldn't be resolved against the diff-local node
    /// set (because the callee lives in an unchanged file). The store resolves
    /// these after inserting the new nodes, using its full existing data.
    pub deferred_calls: Vec<(NodeId, String)>,
    /// Same for parameter/return-type Uses edges.
    pub deferred_uses: Vec<(NodeId, String)>,
    /// Same for struct→trait Implements edges.
    pub deferred_implements: Vec<(NodeId, String)>,
    /// Same for `extends` / inheritance edges.
    pub deferred_inherits: Vec<(NodeId, String)>,
    /// Same for `throws ExceptionType` edges.
    pub deferred_throws: Vec<(NodeId, String)>,
    /// Same for decorator/annotation references.
    pub deferred_annotated: Vec<(NodeId, String)>,
}

impl GraphDiff {
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_node_ids.is_empty()
            && self.removed_files.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
            && self.deferred_calls.is_empty()
            && self.deferred_uses.is_empty()
            && self.deferred_implements.is_empty()
            && self.deferred_inherits.is_empty()
            && self.deferred_throws.is_empty()
            && self.deferred_annotated.is_empty()
    }

    /// Merge another diff into this one. Used when multiple files change
    /// in parallel and their per-file diffs are combined before a single
    /// store write.
    pub fn merge(&mut self, other: GraphDiff) {
        self.added_nodes.extend(other.added_nodes);
        self.removed_node_ids.extend(other.removed_node_ids);
        self.removed_files.extend(other.removed_files);
        self.added_edges.extend(other.added_edges);
        self.removed_edges.extend(other.removed_edges);
        self.deferred_calls.extend(other.deferred_calls);
        self.deferred_uses.extend(other.deferred_uses);
        self.deferred_implements.extend(other.deferred_implements);
        self.deferred_inherits.extend(other.deferred_inherits);
        self.deferred_throws.extend(other.deferred_throws);
        self.deferred_annotated.extend(other.deferred_annotated);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_is_unique() {
        let a = NodeId::new();
        let b = NodeId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn graph_diff_merge() {
        let node = Node {
            id: NodeId::new(),
            kind: NodeKind::Function,
            name: "foo".into(),
            qualified_name: "crate::foo".into(),
            file: PathBuf::from("src/lib.rs"),
            span: Span {
                start_line: 1,
                end_line: 3,
            },
            metadata: NodeMetadata::default(),
        };
        let mut base = GraphDiff::default();
        let other = GraphDiff {
            added_nodes: vec![node],
            ..Default::default()
        };
        base.merge(other);
        assert_eq!(base.added_nodes.len(), 1);
    }

    #[test]
    fn graph_diff_is_empty_on_default() {
        assert!(GraphDiff::default().is_empty());
    }
}
