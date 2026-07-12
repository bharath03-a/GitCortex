use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::GitCortexError,
    schema::{CodeSmell, DesignPattern, EdgeConfidence, EdgeKind, NodeKind, SolidHint, Visibility},
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

/// Source-text capture for a node — signature, body slice, preceding doc-comment,
/// and byte range into the original file. Filled during pass 1 from the
/// tree-sitter node's byte range; cheap (no extra parsing).
///
/// Powers wiki rendering, tour narration, and future semantic search.
/// Empty default means "not captured" — legacy rows return all-empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DefinitionText {
    /// First line(s) of the definition up to (and excluding) the body block.
    /// E.g. `pub fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()>`.
    pub signature: String,
    /// Full source slice of the node, including signature and body.
    pub body: String,
    /// Doc-comment immediately preceding the node (`///`, `//!`, `/** */`, `"""`).
    /// `None` when absent.
    pub doc_comment: Option<String>,
    /// Byte offsets into the parent file. `(0, 0)` if not captured.
    pub start_byte: u32,
    pub end_byte: u32,
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
    /// Decorator / annotation names applied to this symbol, e.g.
    /// `["dataclass"]`, `["Override"]`, `["derive", "Serialize"]`. Captured
    /// regardless of whether the decorator is defined in-repo, so framework
    /// decorators (`@app.route`, `@Test`) remain queryable even though their
    /// `Annotated` edge target is external and dropped.
    pub annotations: Vec<String>,
    /// Pass-2 LLD annotations. Empty until pass 2 runs.
    pub lld: LldLabels,
    /// Raw source-text capture — signature, body, doc-comment, byte range.
    pub definition: DefinitionText,
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
    /// Source line of the relationship's origin, when meaningful. Set for
    /// `Calls` edges (the line of the call expression) so call sites can be
    /// pinpointed; `None` for structural edges (Contains, Implements, …).
    #[serde(default)]
    pub line: Option<u32>,
    /// How confident the indexer is this edge is real (see [`EdgeConfidence`]).
    #[serde(default)]
    pub confidence: EdgeConfidence,
}

impl Edge {
    /// Construct an edge with no associated source line (structural edges).
    /// Defaults to `Extracted` confidence.
    pub fn new(src: NodeId, dst: NodeId, kind: EdgeKind) -> Self {
        Self {
            src,
            dst,
            kind,
            line: None,
            confidence: EdgeConfidence::Extracted,
        }
    }

    /// Construct a `Calls` edge carrying the call-expression line.
    pub fn call(src: NodeId, dst: NodeId, line: u32) -> Self {
        Self {
            src,
            dst,
            kind: EdgeKind::Calls,
            line: Some(line),
            confidence: EdgeConfidence::Extracted,
        }
    }

    /// Set the edge's confidence (builder-style), e.g. mark a cross-file
    /// name-resolved edge as `Inferred`.
    pub fn with_confidence(mut self, confidence: EdgeConfidence) -> Self {
        self.confidence = confidence;
        self
    }
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
    /// Tuple: `(caller_id, callee_name, call_line)`.
    pub deferred_calls: Vec<(NodeId, String, u32)>,
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
    /// Markdown section/file → referenced code symbol name. Intentionally
    /// unscoped by language — a doc can reference a symbol in any language.
    pub deferred_doc_refs: Vec<(NodeId, String)>,
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
            && self.deferred_doc_refs.is_empty()
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
        self.deferred_doc_refs.extend(other.deferred_doc_refs);
    }
}

// ── Graph algorithms (pure, no I/O) ──────────────────────────────────────────

/// Count inbound `Calls` edges per destination node id.
/// Shared by `gitcortex-mcp` (centrality/clustering/tour) and `gitcortex-viz`
/// so both surfaces always use the same algorithm and cannot drift.
pub fn in_degree_by_calls(edges: &[Edge]) -> HashMap<String, u32> {
    let mut in_degree: HashMap<String, u32> = HashMap::new();
    for e in edges {
        if matches!(e.kind, EdgeKind::Calls) {
            *in_degree.entry(e.dst.as_str()).or_insert(0) += 1;
        }
    }
    in_degree
}

/// Find import cycles via Tarjan's SCC over `EdgeKind::Imports` edges.
/// Returns one `Vec<String>` (node IDs) per cycle; cycles of size 1 (self-loops)
/// are excluded.
pub fn find_import_cycles(edges: &[Edge]) -> Vec<Vec<String>> {
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for e in edges {
        if matches!(e.kind, EdgeKind::Imports) {
            adj.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
    }

    let nodes: Vec<String> = adj.keys().cloned().collect();
    let mut index_counter = 0usize;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: HashMap<String, bool> = HashMap::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut lowlink: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<Vec<String>> = Vec::new();

    #[allow(clippy::too_many_arguments)]
    fn strongconnect(
        v: &str,
        adj: &HashMap<String, Vec<String>>,
        counter: &mut usize,
        stack: &mut Vec<String>,
        on_stack: &mut HashMap<String, bool>,
        index: &mut HashMap<String, usize>,
        lowlink: &mut HashMap<String, usize>,
        result: &mut Vec<Vec<String>>,
    ) {
        index.insert(v.to_owned(), *counter);
        lowlink.insert(v.to_owned(), *counter);
        *counter += 1;
        stack.push(v.to_owned());
        on_stack.insert(v.to_owned(), true);

        if let Some(neighbours) = adj.get(v) {
            for w in neighbours.clone() {
                if !index.contains_key(w.as_str()) {
                    strongconnect(&w, adj, counter, stack, on_stack, index, lowlink, result);
                    let ll_w = lowlink[w.as_str()];
                    let ll_v = lowlink[v];
                    lowlink.insert(v.to_owned(), ll_v.min(ll_w));
                } else if *on_stack.get(w.as_str()).unwrap_or(&false) {
                    let idx_w = index[w.as_str()];
                    let ll_v = lowlink[v];
                    lowlink.insert(v.to_owned(), ll_v.min(idx_w));
                }
            }
        }

        if lowlink[v] == index[v] {
            let mut scc: Vec<String> = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.insert(w.clone(), false);
                scc.push(w.clone());
                if w == v {
                    break;
                }
            }
            if scc.len() > 1 {
                result.push(scc);
            }
        }
    }

    for v in &nodes {
        if !index.contains_key(v.as_str()) {
            strongconnect(
                v,
                &adj,
                &mut index_counter,
                &mut stack,
                &mut on_stack,
                &mut index,
                &mut lowlink,
                &mut result,
            );
        }
    }

    result
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

    fn import_edge(src: &NodeId, dst: &NodeId) -> Edge {
        Edge::new(src.clone(), dst.clone(), EdgeKind::Imports)
    }

    #[test]
    fn cycles_empty_when_imports_are_acyclic() {
        let (a, b, c) = (NodeId::new(), NodeId::new(), NodeId::new());
        // a → b → c, no back edge.
        let edges = vec![import_edge(&a, &b), import_edge(&b, &c)];
        assert!(find_import_cycles(&edges).is_empty());
    }

    #[test]
    fn cycles_detects_two_node_cycle() {
        let (a, b) = (NodeId::new(), NodeId::new());
        let edges = vec![import_edge(&a, &b), import_edge(&b, &a)];
        let cycles = find_import_cycles(&edges);
        assert_eq!(cycles.len(), 1);
        let members: std::collections::HashSet<&String> = cycles[0].iter().collect();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&a.as_str()));
        assert!(members.contains(&b.as_str()));
    }

    #[test]
    fn cycles_ignores_non_import_edges() {
        let (a, b) = (NodeId::new(), NodeId::new());
        // A calls-cycle must not register as an import cycle.
        let edges = vec![
            Edge::new(a.clone(), b.clone(), EdgeKind::Calls),
            Edge::new(b.clone(), a.clone(), EdgeKind::Calls),
        ];
        assert!(find_import_cycles(&edges).is_empty());
    }

    #[test]
    fn cycles_detects_three_node_cycle() {
        let (a, b, c) = (NodeId::new(), NodeId::new(), NodeId::new());
        let edges = vec![
            import_edge(&a, &b),
            import_edge(&b, &c),
            import_edge(&c, &a),
        ];
        let cycles = find_import_cycles(&edges);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
    }
}
