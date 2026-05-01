use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility},
};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rayon::prelude::*;

use crate::{
    differ::{Differ, FileChange},
    parser::{parser_for_path, ParseResult},
};

type FileIndexResult = Result<(
    GraphDiff,
    Vec<(NodeId, String)>,
    Vec<(NodeId, String)>,
    Vec<(NodeId, String)>,
    Vec<(NodeId, String)>,
)>;

// ── IncrementalIndexer ────────────────────────────────────────────────────────

/// Orchestrates the differ and parser to produce a `GraphDiff` from
/// `last_sha..HEAD`. Stateless — callers own the repo root and last SHA.
pub struct IncrementalIndexer {
    repo_root: PathBuf,
    ignorer: Gitignore,
}

impl IncrementalIndexer {
    /// Build an indexer rooted at `repo_root`.
    ///
    /// Reads `.gitcortex/ignore` (if present) for exclusion patterns.
    pub fn new(repo_root: &Path) -> Result<Self> {
        let ignorer = build_ignorer(repo_root);
        Ok(Self {
            repo_root: repo_root.to_owned(),
            ignorer,
        })
    }

    /// Run an incremental index from `from_sha` (exclusive) to HEAD.
    ///
    /// Returns:
    /// - `GraphDiff` — changes to apply to the store
    /// - `String`    — the HEAD SHA to persist as `last_indexed_sha`
    ///
    /// When `from_sha` is `None` the entire HEAD tree is indexed (first run).
    pub fn run(&self, from_sha: Option<&str>) -> Result<(GraphDiff, String)> {
        let differ = Differ::open(&self.repo_root)?;
        let head_sha = differ.head_sha()?;

        if from_sha.map(|s| s == head_sha).unwrap_or(false) {
            return Ok((GraphDiff::default(), head_sha));
        }

        let supported = self.supported_extensions();
        let changes = differ.changed_files(from_sha, &supported)?;

        if changes.is_empty() {
            return Ok((GraphDiff::default(), head_sha));
        }

        let (to_parse, to_delete): (Vec<_>, Vec<_>) = changes
            .into_iter()
            .partition(|c| !matches!(c, FileChange::Deleted(_)));

        // Parse added/modified files in parallel.
        let per_file: Vec<FileIndexResult> = to_parse
            .par_iter()
            .map(|change| self.index_file(change.path()))
            .collect();

        // Merge diffs and collect all deferred (cross-file) references.
        let mut merged = GraphDiff::default();
        let mut all_calls: Vec<(NodeId, String)> = Vec::new();
        let mut all_uses: Vec<(NodeId, String)> = Vec::new();
        let mut all_implements: Vec<(NodeId, String)> = Vec::new();
        let mut all_imports: Vec<(NodeId, String)> = Vec::new();
        for result in per_file {
            let (diff, calls, uses, implements, imports) = result?;
            merged.merge(diff);
            all_calls.extend(calls);
            all_uses.extend(uses);
            all_implements.extend(implements);
            all_imports.extend(imports);
        }

        // Build name → NodeId index from all nodes added in this diff.
        // Used to resolve all four deferred edge types.
        let name_to_ids: HashMap<&str, Vec<&NodeId>> = {
            let mut map: HashMap<&str, Vec<&NodeId>> = HashMap::new();
            for node in &merged.added_nodes {
                map.entry(node.name.as_str()).or_default().push(&node.id);
            }
            map
        };

        merged.deferred_calls = resolve_deferred(
            &name_to_ids,
            &all_calls,
            EdgeKind::Calls,
            &mut merged.added_edges,
        );
        merged.deferred_uses = resolve_deferred(
            &name_to_ids,
            &all_uses,
            EdgeKind::Uses,
            &mut merged.added_edges,
        );
        merged.deferred_implements = resolve_deferred(
            &name_to_ids,
            &all_implements,
            EdgeKind::Implements,
            &mut merged.added_edges,
        );
        // Imports use placeholder src IDs so we can't resolve them against the store;
        // resolve what we can locally and silently drop the rest.
        let _ = resolve_deferred(
            &name_to_ids,
            &all_imports,
            EdgeKind::Imports,
            &mut merged.added_edges,
        );

        for deleted in to_delete {
            merged.removed_files.push(deleted.path().to_owned());
        }

        // Synthesise Folder and File structural nodes from the file paths of
        // all nodes in this diff, then append them to the merged result.
        let (struct_nodes, struct_edges) = build_structural_nodes(&merged);
        merged.added_nodes.extend(struct_nodes);
        merged.added_edges.extend(struct_edges);

        Ok((merged, head_sha))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["rs", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go"]
    }

    fn index_file(&self, repo_relative_path: &Path) -> FileIndexResult {
        let abs_path = self.repo_root.join(repo_relative_path);
        let empty = || {
            (
                GraphDiff::default(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        };

        if self.should_ignore(repo_relative_path) {
            return Ok(empty());
        }

        let source = fs::read_to_string(&abs_path).map_err(|e| GitCortexError::Parse {
            file: abs_path.clone(),
            message: e.to_string(),
        })?;

        if source.len() > 512 * 1024 {
            return Ok(empty());
        }

        let parser = match parser_for_path(repo_relative_path) {
            Some(p) => p,
            None => return Ok(empty()),
        };

        let ParseResult {
            nodes,
            edges,
            deferred_calls,
            deferred_uses,
            deferred_implements,
            deferred_imports,
        } = parser.parse(repo_relative_path, &source)?;

        let mut diff = GraphDiff::default();
        // Remove old code nodes for this file and old structural nodes for
        // every ancestor folder (they'll be re-synthesised after merging).
        diff.removed_files.push(repo_relative_path.to_owned());
        for ancestor in repo_relative_path.ancestors().skip(1) {
            if ancestor == Path::new("") || ancestor == Path::new(".") {
                break;
            }
            diff.removed_files.push(ancestor.to_path_buf());
        }
        diff.added_nodes = nodes;
        diff.added_edges = edges;

        Ok((
            diff,
            deferred_calls,
            deferred_uses,
            deferred_implements,
            deferred_imports,
        ))
    }

    fn should_ignore(&self, path: &Path) -> bool {
        self.ignorer
            .matched_path_or_any_parents(path, false)
            .is_ignore()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve deferred `(src_id, target_name)` pairs against a diff-local
/// name→NodeId map. Returns the subset that couldn't be resolved (because the
/// target lives in an unchanged file not present in this diff). The caller
/// stores the unresolved entries in `GraphDiff.deferred_*` so the store can
/// resolve them against its full existing database.
fn resolve_deferred(
    name_to_ids: &HashMap<&str, Vec<&NodeId>>,
    deferred: &[(NodeId, String)],
    kind: EdgeKind,
    edges: &mut Vec<Edge>,
) -> Vec<(NodeId, String)> {
    let mut unresolved = Vec::new();
    for (src_id, target_name) in deferred {
        if let Some(dst_ids) = name_to_ids.get(target_name.as_str()) {
            for dst_id in dst_ids {
                let edge = Edge {
                    src: src_id.clone(),
                    dst: (*dst_id).clone(),
                    kind: kind.clone(),
                };
                if !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        } else {
            unresolved.push((src_id.clone(), target_name.clone()));
        }
    }
    unresolved
}

/// Derives `Folder` and `File` structural nodes from the code nodes already in
/// `diff`. For each unique file path seen in `diff.added_nodes`:
///   - Creates one `File` node (kind = File, file = the_file_path).
///   - Creates `Folder` nodes for every ancestor directory component.
///   - Adds Contains edges: folder→subfolder, folder→file, file→top-level nodes.
///
/// Top-level nodes are code nodes that are NOT already the `dst` of a Contains
/// edge within the same file (i.e. not already contained by a module/struct).
fn build_structural_nodes(diff: &GraphDiff) -> (Vec<Node>, Vec<Edge>) {
    use std::collections::HashSet;

    // Which code node IDs already have an incoming Contains edge?
    let contained: HashSet<&NodeId> = diff
        .added_edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains)
        .map(|e| &e.dst)
        .collect();

    // Group code nodes by file path.
    let mut by_file: HashMap<&Path, Vec<&Node>> = HashMap::new();
    for node in &diff.added_nodes {
        by_file.entry(node.file.as_path()).or_default().push(node);
    }

    let mut new_nodes: Vec<Node> = Vec::new();
    let mut new_edges: Vec<Edge> = Vec::new();
    // folder path → NodeId (so we can build hierarchy edges)
    let mut folder_ids: HashMap<PathBuf, NodeId> = HashMap::new();

    // ── File nodes ────────────────────────────────────────────────────────────
    let mut file_ids: HashMap<&Path, NodeId> = HashMap::new();
    for (file_path, code_nodes) in &by_file {
        let file_id = NodeId::new();
        let name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        new_nodes.push(Node {
            id: file_id.clone(),
            name,
            kind: NodeKind::File,
            qualified_name: file_path.to_string_lossy().into_owned(),
            file: file_path.to_path_buf(),
            span: Span {
                start_line: 1,
                end_line: 1,
            },
            metadata: NodeMetadata {
                visibility: Visibility::Pub,
                ..Default::default()
            },
        });
        file_ids.insert(file_path, file_id.clone());

        // File → top-level code node Contains edges.
        for node in code_nodes {
            if !contained.contains(&node.id) {
                new_edges.push(Edge {
                    src: file_id.clone(),
                    dst: node.id.clone(),
                    kind: EdgeKind::Contains,
                });
            }
        }
    }

    // ── Folder nodes (collect unique ancestor directories) ────────────────────
    let unique_dirs: HashSet<PathBuf> = by_file
        .keys()
        .flat_map(|p| p.ancestors().skip(1))
        .filter(|p| *p != Path::new("") && *p != Path::new("."))
        .map(|p| p.to_path_buf())
        .collect();

    for dir in &unique_dirs {
        let dir_id = folder_ids.entry(dir.clone()).or_default().clone();
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(dir.to_str().unwrap_or(""))
            .to_owned();
        new_nodes.push(Node {
            id: dir_id,
            name,
            kind: NodeKind::Folder,
            qualified_name: dir.to_string_lossy().into_owned(),
            file: dir.clone(),
            span: Span {
                start_line: 1,
                end_line: 1,
            },
            metadata: NodeMetadata {
                visibility: Visibility::Pub,
                ..Default::default()
            },
        });
    }

    // ── Folder→File and Folder→Folder Contains edges ──────────────────────────
    for (file_path, file_id) in &file_ids {
        if let Some(parent) = file_path.parent().filter(|p| *p != Path::new("")) {
            if let Some(dir_id) = folder_ids.get(parent) {
                new_edges.push(Edge {
                    src: dir_id.clone(),
                    dst: file_id.clone(),
                    kind: EdgeKind::Contains,
                });
            }
        }
    }
    for dir in &unique_dirs {
        let child_id = folder_ids[dir].clone();
        if let Some(parent) = dir
            .parent()
            .filter(|p| *p != Path::new("") && *p != Path::new("."))
        {
            if let Some(parent_id) = folder_ids.get(parent) {
                new_edges.push(Edge {
                    src: parent_id.clone(),
                    dst: child_id,
                    kind: EdgeKind::Contains,
                });
            }
        }
    }

    (new_nodes, new_edges)
}

fn build_ignorer(repo_root: &Path) -> Gitignore {
    let ignore_path = repo_root.join(".gitcortex/ignore");
    let mut builder = GitignoreBuilder::new(repo_root);
    if ignore_path.exists() {
        let _ = builder.add(ignore_path);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}
