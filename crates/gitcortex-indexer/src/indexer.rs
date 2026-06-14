use std::{
    collections::{HashMap, HashSet},
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
    Vec<(NodeId, String, u32)>, // deferred_calls
    Vec<(NodeId, String)>,      // deferred_uses
    Vec<(NodeId, String)>,      // deferred_implements
    Vec<(NodeId, String)>,      // deferred_imports
    Vec<(NodeId, String)>,      // deferred_inherits
    Vec<(NodeId, String)>,      // deferred_throws
    Vec<(NodeId, String)>,      // deferred_annotated
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

        let timing = std::env::var_os("GCX_TIMING").is_some();
        let t0 = std::time::Instant::now();
        macro_rules! mark {
            ($label:expr) => {
                if timing {
                    eprintln!("[gcx-timing] {:>8.2?}  {}", t0.elapsed(), $label);
                }
            };
        }

        let supported = self.supported_extensions();
        let changes = differ.changed_files(from_sha, &supported)?;

        if changes.is_empty() {
            return Ok((GraphDiff::default(), head_sha));
        }

        let (to_parse, to_delete): (Vec<_>, Vec<_>) = changes
            .into_iter()
            .partition(|c| !matches!(c, FileChange::Deleted(_)));
        mark!(format!("diff: {} files to parse", to_parse.len()));

        // Parse added/modified files in parallel.
        let per_file: Vec<FileIndexResult> = to_parse
            .par_iter()
            .map(|change| self.index_file(change.path()))
            .collect();
        mark!("parse (parallel) done");

        // Merge diffs and collect all deferred (cross-file) references.
        let mut merged = GraphDiff::default();
        let mut all_calls: Vec<(NodeId, String, u32)> = Vec::new();
        let mut all_uses: Vec<(NodeId, String)> = Vec::new();
        let mut all_implements: Vec<(NodeId, String)> = Vec::new();
        let mut all_imports: Vec<(NodeId, String)> = Vec::new();
        let mut all_inherits: Vec<(NodeId, String)> = Vec::new();
        let mut all_throws: Vec<(NodeId, String)> = Vec::new();
        let mut all_annotated: Vec<(NodeId, String)> = Vec::new();
        for result in per_file {
            let (diff, calls, uses, implements, imports, inherits, throws, annotated) = result?;
            merged.merge(diff);
            all_calls.extend(calls);
            all_uses.extend(uses);
            all_implements.extend(implements);
            all_imports.extend(imports);
            all_inherits.extend(inherits);
            all_throws.extend(throws);
            all_annotated.extend(annotated);
        }
        mark!(format!(
            "merge done: {} nodes, {} direct edges, {} deferred calls",
            merged.added_nodes.len(),
            merged.added_edges.len(),
            all_calls.len()
        ));

        // Build name → NodeId index from all nodes added in this diff.
        // Used to resolve all four deferred edge types.
        let name_to_ids: HashMap<&str, Vec<&NodeId>> = {
            let mut map: HashMap<&str, Vec<&NodeId>> = HashMap::new();
            for node in &merged.added_nodes {
                map.entry(node.name.as_str()).or_default().push(&node.id);
            }
            map
        };

        // Parallel id → file map. Used to constrain deferred resolution to the
        // caller's language family — without this, a Java caller's deferred
        // `greet` would be resolved to all `greet` symbols across every
        // language in the diff, producing spurious cross-language edges.
        let id_to_file: HashMap<&NodeId, &Path> = merged
            .added_nodes
            .iter()
            .map(|n| (&n.id, n.file.as_path()))
            .collect();

        // Edge dedup set, seeded from the direct edges the parsers already
        // emitted. `resolve_deferred` consults this instead of scanning the
        // `added_edges` Vec — the old `Vec::contains` made resolution O(E²)
        // and dominated full-index time on large repos.
        let mut seen_edges: HashSet<(String, String, String)> = merged
            .added_edges
            .iter()
            .map(|e| (e.src.as_str(), e.dst.as_str(), e.kind.to_string()))
            .collect();

        merged.deferred_calls = resolve_calls(
            &name_to_ids,
            &id_to_file,
            &all_calls,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        merged.deferred_uses = resolve_deferred(
            &name_to_ids,
            &id_to_file,
            &all_uses,
            EdgeKind::Uses,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        merged.deferred_implements = resolve_deferred(
            &name_to_ids,
            &id_to_file,
            &all_implements,
            EdgeKind::Implements,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        // Imports use placeholder src IDs so we can't resolve them against the store;
        // resolve what we can locally and silently drop the rest.
        let _ = resolve_deferred(
            &name_to_ids,
            &id_to_file,
            &all_imports,
            EdgeKind::Imports,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        merged.deferred_inherits = resolve_deferred(
            &name_to_ids,
            &id_to_file,
            &all_inherits,
            EdgeKind::Inherits,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        merged.deferred_throws = resolve_deferred(
            &name_to_ids,
            &id_to_file,
            &all_throws,
            EdgeKind::Throws,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        merged.deferred_annotated = resolve_deferred(
            &name_to_ids,
            &id_to_file,
            &all_annotated,
            EdgeKind::Annotated,
            &mut merged.added_edges,
            &mut seen_edges,
        );
        mark!(format!(
            "resolve_deferred done: {} total edges",
            merged.added_edges.len()
        ));

        // Mirror annotation NAMES onto each decorated node's metadata. The
        // `Annotated` edge above only survives when the decorator is defined
        // in-repo; most framework decorators (`@app.route`, `@Test`) are
        // external and dropped. Storing the names as metadata keeps them
        // queryable regardless.
        {
            let mut ann_by_id: HashMap<String, Vec<String>> = HashMap::new();
            for (node_id, name) in &all_annotated {
                ann_by_id
                    .entry(node_id.as_str())
                    .or_default()
                    .push(name.clone());
            }
            for node in &mut merged.added_nodes {
                if let Some(names) = ann_by_id.get(&node.id.as_str()) {
                    let mut seen: HashSet<String> = HashSet::new();
                    node.metadata.annotations = names
                        .iter()
                        .filter(|n| seen.insert((*n).clone()))
                        .cloned()
                        .collect();
                }
            }
        }

        for deleted in to_delete {
            merged.removed_files.push(deleted.path().to_owned());
        }

        // Synthesise Folder and File structural nodes from the file paths of
        // all nodes in this diff, then append them to the merged result.
        let (struct_nodes, struct_edges) = build_structural_nodes(&merged);
        merged.added_nodes.extend(struct_nodes);
        merged.added_edges.extend(struct_edges);
        mark!("structural nodes done");

        Ok((merged, head_sha))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec![
            "rs", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go", "java",
        ]
    }

    fn index_file(&self, repo_relative_path: &Path) -> FileIndexResult {
        let abs_path = self.repo_root.join(repo_relative_path);
        let empty = || {
            (
                GraphDiff::default(),
                Vec::<(NodeId, String, u32)>::new(),
                Vec::<(NodeId, String)>::new(),
                Vec::<(NodeId, String)>::new(),
                Vec::<(NodeId, String)>::new(),
                Vec::<(NodeId, String)>::new(),
                Vec::<(NodeId, String)>::new(),
                Vec::<(NodeId, String)>::new(),
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
            deferred_inherits,
            deferred_throws,
            deferred_annotated,
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
            deferred_inherits,
            deferred_throws,
            deferred_annotated,
        ))
    }

    fn should_ignore(&self, path: &Path) -> bool {
        self.ignorer
            .matched_path_or_any_parents(path, false)
            .is_ignore()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// When a name resolves to more than this many same-language definitions, it's
/// treated as ambiguous and produces NO edges. Hot names like `get`, `save`,
/// `__init__`, `filter` have hundreds of definitions; linking a call site to
/// every one of them is both useless (no precision) and the source of an
/// edge explosion that made full indexing O(call_sites × defs). Skipping the
/// over-ambiguous names keeps the precise majority and drops the noise.
const MAX_RESOLVE_FANOUT: usize = 8;

/// Resolve deferred `(src_id, target_name)` pairs against a diff-local
/// name→NodeId map. Returns the subset that couldn't be resolved (because the
/// target lives in an unchanged file not present in this diff). The caller
/// stores the unresolved entries in `GraphDiff.deferred_*` so the store can
/// resolve them against its full existing database.
///
/// Candidate destinations are filtered to the same language family as the
/// source (by file extension) — otherwise a Java caller's deferred `greet`
/// would resolve to every `greet` symbol across all languages in the diff.
///
/// `seen` dedups edges in O(1); the previous `Vec::contains` made this O(E²)
/// and dominated full-index time on large repos.
fn resolve_deferred(
    name_to_ids: &HashMap<&str, Vec<&NodeId>>,
    id_to_file: &HashMap<&NodeId, &Path>,
    deferred: &[(NodeId, String)],
    kind: EdgeKind,
    edges: &mut Vec<Edge>,
    seen: &mut HashSet<(String, String, String)>,
) -> Vec<(NodeId, String)> {
    let mut unresolved = Vec::new();
    for (src_id, target_name) in deferred {
        let src_exts = id_to_file
            .get(src_id)
            .and_then(|p| language_extensions_for_path(p));

        let Some(dst_ids) = name_to_ids.get(target_name.as_str()) else {
            unresolved.push((src_id.clone(), target_name.clone()));
            continue;
        };

        // Same-language candidates only.
        let candidates: Vec<&NodeId> = dst_ids
            .iter()
            .copied()
            .filter(|dst_id| match (src_exts, id_to_file.get(*dst_id)) {
                (Some(exts), Some(dst_path)) => language_extensions_for_path(dst_path)
                    .map(|dst_exts| dst_exts.first() == exts.first())
                    .unwrap_or(true),
                _ => true,
            })
            .collect();

        // Over-ambiguous name: don't fan out. Treat as resolved (not pushed to
        // `unresolved`) so the store doesn't retry the same explosive match.
        if candidates.len() > MAX_RESOLVE_FANOUT {
            continue;
        }

        let mut matched_any = false;
        for dst_id in candidates {
            matched_any = true;
            let key = (src_id.as_str(), dst_id.as_str(), kind.to_string());
            if seen.insert(key) {
                edges.push(Edge {
                    src: src_id.clone(),
                    dst: dst_id.clone(),
                    kind: kind.clone(),
                    line: None,
                });
            }
        }
        if !matched_any {
            unresolved.push((src_id.clone(), target_name.clone()));
        }
    }
    unresolved
}

/// Like [`resolve_deferred`] but for `Calls` edges, carrying each call's source
/// line onto the resulting edge. Unresolved calls are returned as
/// `(caller_id, callee_name, line)` so the store can resolve them later and
/// still record the line.
fn resolve_calls(
    name_to_ids: &HashMap<&str, Vec<&NodeId>>,
    id_to_file: &HashMap<&NodeId, &Path>,
    deferred: &[(NodeId, String, u32)],
    edges: &mut Vec<Edge>,
    seen: &mut HashSet<(String, String, String)>,
) -> Vec<(NodeId, String, u32)> {
    let mut unresolved = Vec::new();
    for (src_id, target_name, line) in deferred {
        let src_exts = id_to_file
            .get(src_id)
            .and_then(|p| language_extensions_for_path(p));

        let Some(dst_ids) = name_to_ids.get(target_name.as_str()) else {
            unresolved.push((src_id.clone(), target_name.clone(), *line));
            continue;
        };

        let candidates: Vec<&NodeId> = dst_ids
            .iter()
            .copied()
            .filter(|dst_id| match (src_exts, id_to_file.get(*dst_id)) {
                (Some(exts), Some(dst_path)) => language_extensions_for_path(dst_path)
                    .map(|dst_exts| dst_exts.first() == exts.first())
                    .unwrap_or(true),
                _ => true,
            })
            .collect();

        if candidates.len() > MAX_RESOLVE_FANOUT {
            continue;
        }

        let mut matched_any = false;
        for dst_id in candidates {
            matched_any = true;
            let key = (
                src_id.as_str(),
                dst_id.as_str(),
                EdgeKind::Calls.to_string(),
            );
            if seen.insert(key) {
                edges.push(Edge::call(src_id.clone(), dst_id.clone(), *line));
            }
        }
        if !matched_any {
            unresolved.push((src_id.clone(), target_name.clone(), *line));
        }
    }
    unresolved
}

/// File extension family for `path`. Mirrors the store-side helper — kept
/// duplicated to avoid pulling kuzu into the indexer.
fn language_extensions_for_path(path: &Path) -> Option<&'static [&'static str]> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(&[".rs"]),
        "py" => Some(&[".py"]),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            Some(&[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"])
        }
        "go" => Some(&[".go"]),
        "java" => Some(&[".java"]),
        _ => None,
    }
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
                    line: None,
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
                    line: None,
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
                    line: None,
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
