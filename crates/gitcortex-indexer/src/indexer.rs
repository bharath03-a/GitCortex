use std::{
    fs,
    path::{Path, PathBuf},
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::GraphDiff,
};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rayon::prelude::*;

use crate::{
    differ::{Differ, FileChange},
    parser::parser_for_path,
};

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
    /// - `String` — the HEAD SHA to persist as the new `last_indexed_sha`
    ///
    /// When `from_sha` is `None` the entire HEAD tree is indexed (first run).
    pub fn run(&self, from_sha: Option<&str>) -> Result<(GraphDiff, String)> {
        let differ = Differ::open(&self.repo_root)?;
        let head_sha = differ.head_sha()?;

        // Idempotency guard: nothing to do if already up-to-date.
        if from_sha.map(|s| s == head_sha).unwrap_or(false) {
            return Ok((GraphDiff::default(), head_sha));
        }

        let supported = self.supported_extensions();
        let changes = differ.changed_files(from_sha, &supported)?;

        if changes.is_empty() {
            return Ok((GraphDiff::default(), head_sha));
        }

        // Partition into files to parse and files to delete.
        let (to_parse, to_delete): (Vec<_>, Vec<_>) =
            changes.into_iter().partition(|c| !matches!(c, FileChange::Deleted(_)));

        // Parse added/modified files in parallel via rayon.
        let per_file_diffs: Vec<Result<GraphDiff>> = to_parse
            .par_iter()
            .map(|change| self.index_file(change.path()))
            .collect();

        // Merge all per-file diffs, short-circuiting on first error.
        let mut merged = GraphDiff::default();
        for result in per_file_diffs {
            merged.merge(result?);
        }

        // Record deleted files — the store removes their nodes.
        for deleted in to_delete {
            merged.removed_files.push(deleted.path().to_owned());
        }

        Ok((merged, head_sha))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn supported_extensions(&self) -> Vec<&'static str> {
        // v0.1: Rust only. v0.2 adds TypeScript + Python.
        vec!["rs"]
    }

    fn index_file(&self, repo_relative_path: &Path) -> Result<GraphDiff> {
        let abs_path = self.repo_root.join(repo_relative_path);

        // Check .gitcortex/ignore rules.
        if self.should_ignore(repo_relative_path) {
            return Ok(GraphDiff::default());
        }

        let source = fs::read_to_string(&abs_path).map_err(|e| GitCortexError::Parse {
            file: abs_path.clone(),
            message: e.to_string(),
        })?;

        // Skip files that exceed the configured size threshold (500 KB default).
        if source.len() > 512 * 1024 {
            return Ok(GraphDiff::default());
        }

        let parser = match parser_for_path(repo_relative_path) {
            Some(p) => p,
            None => return Ok(GraphDiff::default()),
        };

        let (nodes, edges) = parser.parse(repo_relative_path, &source)?;

        // Treat this file as a full replacement: remove the old version of all
        // its nodes, then add the newly parsed ones.
        let mut diff = GraphDiff::default();
        diff.removed_files.push(repo_relative_path.to_owned());
        diff.added_nodes = nodes;
        diff.added_edges = edges;
        Ok(diff)
    }

    fn should_ignore(&self, path: &Path) -> bool {
        self.ignorer
            .matched_path_or_any_parents(path, false)
            .is_ignore()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_ignorer(repo_root: &Path) -> Gitignore {
    let ignore_path = repo_root.join(".gitcortex/ignore");
    let mut builder = GitignoreBuilder::new(repo_root);
    if ignore_path.exists() {
        let _ = builder.add(ignore_path);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}
