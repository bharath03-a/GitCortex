use std::path::{Path, PathBuf};

use git2::{Delta, DiffOptions, Repository};
use gitcortex_core::error::{GitCortexError, Result};

// ── Types ─────────────────────────────────────────────────────────────────────

/// What happened to a file between two commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Added(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
}

impl FileChange {
    pub fn path(&self) -> &Path {
        match self {
            FileChange::Added(p) | FileChange::Modified(p) | FileChange::Deleted(p) => p,
        }
    }
}

// ── Differ ────────────────────────────────────────────────────────────────────

/// Wraps a `git2::Repository` and computes file-level change sets between commits.
pub struct Differ {
    repo: Repository,
}

impl Differ {
    /// Open the repository at `repo_path` (or any parent that is a git repo).
    pub fn open(repo_path: &Path) -> Result<Self> {
        let repo = Repository::discover(repo_path)
            .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;
        Ok(Self { repo })
    }

    /// Hex SHA of the current HEAD commit.
    pub fn head_sha(&self) -> Result<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;
        let commit = head
            .peel_to_commit()
            .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;
        Ok(commit.id().to_string())
    }

    /// Compute which files changed between `from_sha` (exclusive) and HEAD.
    ///
    /// - `from_sha = None` — diff the empty tree against HEAD (first-time index).
    /// - `from_sha = Some(sha)` — diff that commit against HEAD.
    ///
    /// Only files whose extension is in `supported_exts` are returned.
    pub fn changed_files(
        &self,
        from_sha: Option<&str>,
        supported_exts: &[&str],
    ) -> Result<Vec<FileChange>> {
        let head_tree = self
            .repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .and_then(|c| c.tree())
            .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;

        let from_tree = match from_sha {
            None => None,
            Some(sha) => {
                let oid = git2::Oid::from_str(sha)
                    .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;
                let commit = self
                    .repo
                    .find_commit(oid)
                    .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;
                let tree = commit
                    .tree()
                    .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;
                Some(tree)
            }
        };

        let mut opts = DiffOptions::new();
        opts.ignore_whitespace(false);

        let diff = self
            .repo
            .diff_tree_to_tree(from_tree.as_ref(), Some(&head_tree), Some(&mut opts))
            .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;

        let mut changes: Vec<FileChange> = Vec::new();

        diff.foreach(
            &mut |delta, _progress| {
                let change = match delta.status() {
                    Delta::Added | Delta::Copied | Delta::Renamed => delta
                        .new_file()
                        .path()
                        .map(|p| FileChange::Added(p.to_owned())),
                    Delta::Modified => delta
                        .new_file()
                        .path()
                        .map(|p| FileChange::Modified(p.to_owned())),
                    Delta::Deleted => delta
                        .old_file()
                        .path()
                        .map(|p| FileChange::Deleted(p.to_owned())),
                    _ => None,
                };

                if let Some(c) = change {
                    let ext = c.path().extension().and_then(|e| e.to_str());
                    if ext.map(|e| supported_exts.contains(&e)).unwrap_or(false) {
                        changes.push(c);
                    }
                }
                true
            },
            None,
            None,
            None,
        )
        .map_err(|e| GitCortexError::Git(e.message().to_owned()))?;

        Ok(changes)
    }
}
