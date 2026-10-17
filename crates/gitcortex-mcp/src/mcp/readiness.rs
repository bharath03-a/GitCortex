//! Conservative scope/readiness checks at the MCP boundary. Readiness describes
//! the available evidence, independently of whether a symbol has any callers.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

use gitcortex_core::store::GraphStore;
use serde::Serialize;

use super::helpers::detect_current_branch;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexReadiness {
    pub state: &'static str,
    pub branch: String,
    pub checkout_branch: Option<String>,
    pub checkout_revision: Option<String>,
    pub target_revision: Option<String>,
    pub indexed_revision: Option<String>,
    pub working_tree_verified: bool,
    pub next_action: &'static str,
}

/// Dirty source paths, including both sides of a rename and untracked files.
/// Git-generated setup/configuration files do not make source evidence stale.
pub fn changed_sources(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    use std::os::unix::ffi::OsStrExt;
    let indexer = gitcortex_indexer::IncrementalIndexer::new(root)?;
    let repo = git2::Repository::open(root)?;
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut options))?;
    let mut paths = std::collections::HashSet::new();
    for entry in statuses.iter() {
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(entry.path_bytes()));
        if indexer.includes_path(&path) {
            paths.insert(path);
        }
        for delta in [entry.head_to_index(), entry.index_to_workdir()]
            .into_iter()
            .flatten()
        {
            for path in [delta.old_file().path(), delta.new_file().path()]
                .into_iter()
                .flatten()
            {
                if indexer.includes_path(path) {
                    paths.insert(path.to_owned());
                }
            }
        }
    }
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    Ok(paths)
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

pub fn inspect(root: &Path, store: &dyn GraphStore, branch: &str) -> IndexReadiness {
    let checkout_branch = detect_current_branch(root);
    let checkout_revision = git(root, &["rev-parse", "--verify", "HEAD"]);
    let target_revision = if checkout_branch.as_deref() == Some(branch) {
        checkout_revision.clone()
    } else {
        git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{branch}^{{commit}}"),
            ],
        )
    };
    let indexed = store.last_indexed_sha(branch);
    let persisted_dirty = store.working_tree_dirty(branch);
    // Until the watcher has a verified working-tree checkpoint, a matching HEAD
    // alone cannot certify uncommitted contents. Do not silently claim otherwise.
    let clean = changed_sources(root).ok().map(|paths| paths.is_empty());
    let checkout_scope = checkout_branch.as_deref() == Some(branch);
    let state = match (&indexed, &target_revision, &checkout_revision, clean) {
        (Err(_), _, _, _) | (_, None, _, _) | (_, _, None, _) | (_, _, _, None) => "unavailable",
        (Ok(None), _, _, _) => "missing",
        (Ok(Some(indexed)), Some(target), _, _) if indexed != target => "stale",
        (_, _, _, Some(false)) if checkout_scope => "unverified",
        _ if persisted_dirty.is_err() => "unavailable",
        _ if persisted_dirty.as_ref().copied().unwrap_or(true) => "unverified",
        _ => "ready",
    };
    IndexReadiness {
        state,
        branch: branch.to_owned(),
        checkout_branch,
        checkout_revision,
        target_revision,
        indexed_revision: indexed.ok().flatten(),
        working_tree_verified: (!checkout_scope || clean == Some(true)) && matches!(persisted_dirty, Ok(false)),
        next_action: match state {
            "ready" => "Verify source and tests; ignored, oversized, unsupported, and dynamic code can be absent from the static graph.",
            "unverified" => "Working tree has edits; verify source and tests. Graph evidence cannot establish safety.",
            "missing" => "Run gcx init on the requested checkout. An empty index does not mean no dependencies.",
            "stale" => "Synchronize the requested checkout with gcx hook or gcx init, then retry.",
            _ => "Verify the repository, requested branch, and store health; fall back to source inspection.",
        },
    }
}

/// Verify the exact source bytes used by a batch against a pinned commit.
pub fn snapshots_match_commit(
    root: &Path,
    head: &str,
    snapshots: &HashMap<PathBuf, Option<String>>,
) -> anyhow::Result<bool> {
    let repo = git2::Repository::open(root)?;
    let tree = repo.find_commit(git2::Oid::from_str(head)?)?.tree()?;
    for (path, expected) in snapshots {
        let actual = match tree.get_path(path) {
            Ok(entry) => Some(entry.id().to_string()),
            Err(error) if error.code() == git2::ErrorCode::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if &actual != expected {
            return Ok(false);
        }
    }
    Ok(true)
}
