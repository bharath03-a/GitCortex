//! Conservative scope/readiness checks at the MCP boundary. Readiness describes
//! the available evidence, independently of whether a symbol has any callers.
use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use gitcortex_core::store::GraphStore;
use gitcortex_store::branch::{last_sha_path, sanitize, storage_repo_id};
use serde::{Deserialize, Serialize};

use super::helpers::detect_current_branch;

/// Maximum number of dirty in-scope source files that can be certified via an
/// overlay checkpoint. Larger dirty sets stay `unverified` so `inspect` remains
/// cheap.
pub const MAX_OVERLAY_FILES: usize = 64;
/// Maximum byte size of a single dirty source file that can be hashed or
/// certified. Matches the indexer's 512 KiB AST extraction cap.
pub const MAX_OVERLAY_FILE_BYTES: u64 = 512 * 1024;
/// Maximum aggregate bytes hashed across all dirty files during one `inspect`.
pub const MAX_OVERLAY_TOTAL_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum byte size of the serialized `.overlay` checkpoint JSON on disk.
pub const MAX_CHECKPOINT_JSON_BYTES: u64 = 256 * 1024;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayCheckpoint {
    pub head: String,
    pub branch_key: String,
    pub files: BTreeMap<PathBuf, Option<String>>,
    /// `.gitcortex/ignore` blob hash the overlay was indexed under.
    #[serde(default)]
    pub ignore: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexReadiness {
    pub state: &'static str,
    pub branch: String,
    pub checkout_branch: Option<String>,
    pub checkout_revision: Option<String>,
    pub target_revision: Option<String>,
    pub indexed_revision: Option<String>,
    pub working_tree_verified: bool,
    pub next_action: String,
}

/// Path to the persisted overlay checkpoint next to `<sanitize(branch)>.dirty`
/// in the repository's store data directory.
pub fn overlay_checkpoint_path(root: &Path, branch: &str) -> PathBuf {
    let repo_id = storage_repo_id(root);
    last_sha_path(&repo_id, branch).with_extension("overlay")
}

/// Remove any persisted overlay checkpoint for `branch`.
pub fn remove_overlay_checkpoint(root: &Path, branch: &str) -> anyhow::Result<()> {
    let path = overlay_checkpoint_path(root, branch);
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Atomically persist an overlay checkpoint (`temp + rename`), bounded by file
/// count and JSON payload size. If bounds are exceeded, any existing checkpoint
/// is removed so stale data cannot be trusted.
pub fn write_overlay_checkpoint(
    root: &Path,
    branch: &str,
    head: &str,
    files: &BTreeMap<PathBuf, Option<String>>,
) -> anyhow::Result<bool> {
    if files.is_empty() || files.len() > MAX_OVERLAY_FILES {
        remove_overlay_checkpoint(root, branch)?;
        return Ok(false);
    }
    let checkpoint = OverlayCheckpoint {
        head: head.to_owned(),
        branch_key: sanitize(branch),
        files: files.clone(),
        ignore: super::ignore_rules::rules_hash(root)?,
    };
    let bytes = serde_json::to_vec(&checkpoint)?;
    if bytes.len() as u64 > MAX_CHECKPOINT_JSON_BYTES {
        remove_overlay_checkpoint(root, branch)?;
        return Ok(false);
    }
    let target = overlay_checkpoint_path(root, branch);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = target.with_extension(format!("overlay.tmp.{}.{seq}", std::process::id()));
    if let Err(error) = std::fs::write(&tmp, &bytes) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(true)
}

fn read_overlay_checkpoint(root: &Path, branch: &str) -> Option<OverlayCheckpoint> {
    let path = overlay_checkpoint_path(root, branch);
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CHECKPOINT_JSON_BYTES {
        return None;
    }
    let mut file = std::fs::File::open(&path).ok()?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take(MAX_CHECKPOINT_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_CHECKPOINT_JSON_BYTES {
        return None;
    }
    let checkpoint: OverlayCheckpoint = serde_json::from_slice(&bytes).ok()?;
    if checkpoint.files.is_empty() || checkpoint.files.len() > MAX_OVERLAY_FILES {
        return None;
    }
    Some(checkpoint)
}

/// Whether `bytes` would be accepted for AST extraction by the indexer
/// (valid UTF-8, `<= 512 KiB`, and contains no NUL bytes).
pub fn is_indexable_source_bytes(bytes: &[u8]) -> bool {
    bytes.len() as u64 <= MAX_OVERLAY_FILE_BYTES
        && std::str::from_utf8(bytes)
            .ok()
            .is_some_and(|text| !text.contains('\0'))
}

/// Verify that the persisted overlay checkpoint for `branch` at `head` matches
/// the current dirty source set on disk. Returns the verified checkpoint when
/// valid so the watcher can also seed its in-memory snapshots on restart.
pub fn verify_overlay_checkpoint(
    root: &Path,
    branch: &str,
    head: &str,
    changed: &[PathBuf],
) -> Option<OverlayCheckpoint> {
    if changed.is_empty() || changed.len() > MAX_OVERLAY_FILES {
        return None;
    }
    let checkpoint = read_overlay_checkpoint(root, branch)?;
    if checkpoint.head != head || checkpoint.branch_key != sanitize(branch) {
        return None;
    }
    if super::ignore_rules::rules_hash(root).ok()? != checkpoint.ignore
        || super::ignore_rules::rules_modified(root, head).unwrap_or(true)
    {
        return None;
    }
    if checkpoint.files.len() != changed.len() {
        return None;
    }
    let mut total_bytes = 0u64;
    for rel in changed {
        let expected = checkpoint.files.get(rel)?;
        let full = root.join(rel);
        match std::fs::symlink_metadata(&full) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return None;
                }
                let len = metadata.len();
                if len > MAX_OVERLAY_FILE_BYTES {
                    return None;
                }
                total_bytes = total_bytes.saturating_add(len);
                if total_bytes > MAX_OVERLAY_TOTAL_BYTES {
                    return None;
                }
                let mut file = std::fs::File::open(&full).ok()?;
                let mut bytes = Vec::with_capacity(len as usize);
                file.by_ref()
                    .take(MAX_OVERLAY_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if !is_indexable_source_bytes(&bytes) {
                    return None;
                }
                let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)
                    .ok()?
                    .to_string();
                if expected.as_deref() != Some(oid.as_str()) {
                    return None;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if expected.is_some() {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    Some(checkpoint)
}

/// Dirty source paths, including both sides of a rename and untracked files.
/// Git-generated setup/configuration files and ignored/unsupported paths do not
/// make source evidence stale.
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
    inspect_with_pending(root, store, branch, false)
}

pub fn inspect_with_pending(
    root: &Path,
    store: &dyn GraphStore,
    branch: &str,
    events_pending: bool,
) -> IndexReadiness {
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
    let changed = changed_sources(root).ok();
    let clean = changed.as_ref().map(|paths| paths.is_empty());
    let checkout_scope = checkout_branch.as_deref() == Some(branch);
    let rules_modified = checkout_scope
        && checkout_revision.as_deref().map_or(true, |head| {
            super::ignore_rules::rules_modified(root, head).unwrap_or(true)
        });
    let overlay_verified = checkout_scope
        && !events_pending
        && clean == Some(false)
        && matches!(persisted_dirty, Ok(true))
        && match (&indexed, &checkout_revision, &changed) {
            (Ok(Some(idx)), Some(head), Some(paths)) if idx == head => {
                verify_overlay_checkpoint(root, branch, head, paths).is_some()
            }
            _ => false,
        };
    let state = match (&indexed, &target_revision, &checkout_revision, clean) {
        (Err(_), _, _, _) | (_, None, _, _) | (_, _, None, _) | (_, _, _, None) => "unavailable",
        (Ok(None), _, _, _) => "missing",
        (Ok(Some(indexed)), Some(target), _, _) if indexed != target => "stale",
        (_, _, _, Some(false)) if checkout_scope => {
            if overlay_verified {
                "ready"
            } else {
                "unverified"
            }
        }
        _ if persisted_dirty.is_err() => "unavailable",
        _ if rules_modified => "unverified",
        _ if persisted_dirty.as_ref().copied().unwrap_or(true) => "unverified",
        _ if checkout_scope && events_pending => "unverified",
        _ => "ready",
    };
    let working_tree_verified = state == "ready"
        && ((!checkout_scope && matches!(persisted_dirty, Ok(false)))
            || (checkout_scope
                && !events_pending
                && ((clean == Some(true) && matches!(persisted_dirty, Ok(false)))
                    || overlay_verified)));
    IndexReadiness {
        state,
        branch: branch.to_owned(),
        checkout_branch,
        checkout_revision,
        target_revision,
        indexed_revision: indexed.ok().flatten(),
        working_tree_verified,
        next_action: match state {
            "ready" => "Verify source and tests; ignored, oversized, unsupported, and dynamic code can be absent from the static graph.",
            "unverified" => "Working tree has edits; verify source and tests. Graph evidence cannot establish safety.",
            "missing" => "Run gcx init on the requested checkout. An empty index does not mean no dependencies.",
            "stale" => "Synchronize the requested checkout with gcx hook or gcx init, then retry.",
            _ => "Verify the repository, requested branch, and store health; fall back to source inspection.",
        }.to_owned(),
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

/// Readiness when no store or repository evidence could be consulted.
pub(crate) fn without_evidence(
    state: &'static str,
    branch: &str,
    next_action: String,
) -> IndexReadiness {
    IndexReadiness {
        state,
        branch: branch.to_owned(),
        checkout_branch: None,
        checkout_revision: None,
        target_revision: None,
        indexed_revision: None,
        working_tree_verified: false,
        next_action,
    }
}

/// Inspect commit evidence without consulting a checkout or HEAD.
pub fn inspect_commit(store: &dyn GraphStore, key: &str, sha: &str) -> IndexReadiness {
    let indexed = store.last_indexed_sha(key);
    let dirty = store.working_tree_dirty(key);
    let state = match (&indexed, &dirty) {
        (Err(_), _) | (_, Err(_)) => "unavailable",
        (_, Ok(true)) => "unverified",
        (Ok(None), _) => "missing",
        (Ok(Some(actual)), _) if actual != sha => "stale",
        _ => "ready",
    };
    IndexReadiness {
        state,
        branch: key.to_owned(),
        checkout_branch: None,
        checkout_revision: None,
        target_revision: Some(sha.to_owned()),
        indexed_revision: indexed.ok().flatten(),
        working_tree_verified: false,
        next_action: match state {
            "ready" => "Verify source and tests; static graph coverage is limited.",
            "unverified" => {
                "Commit apply is incomplete; retry indexing before relying on graph evidence."
            }
            _ => "Index the requested commit and retry; fall back to source inspection.",
        }
        .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::test_support::{commit_file, TempRepo, STORE_LOCK};
    use crate::mcp::tree_jobs::apply_tree_index;
    use gitcortex_store::kuzu::KuzuGraphStore;
    use std::sync::{Arc, Mutex};

    fn init_repo() -> TempRepo {
        TempRepo::init()
    }

    #[test]
    fn inspect_commit_ready_in_bare_repo() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        commit_without_checkout(true);
    }

    #[test]
    fn inspect_commit_ready_with_unborn_checkout() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        commit_without_checkout(false);
    }

    fn commit_without_checkout(bare: bool) {
        let source = init_repo();
        let sha = commit_file(source.path(), "lib.rs", "pub fn fetched() {}\n", "source");
        let root = TempRepo::bare_tempdir();
        let mut init = Command::new("git");
        init.args(["init", "-b", "main"]);
        if bare {
            init.arg("--bare");
        }
        assert!(init.arg(root.path()).status().expect("git init").success());
        assert!(Command::new("git")
            .args(["fetch", source.path().to_str().expect("source path"), &sha,])
            .current_dir(root.path())
            .status()
            .expect("git fetch")
            .success());
        let mut store = KuzuGraphStore::open(root.path()).expect("store");
        apply_tree_index(root.path(), &mut store, &sha, &sha).expect("apply_tree_index");
        let state = inspect_commit(&store, &sha, &sha);
        assert_eq!(state.state, "ready");
        assert!(state.checkout_branch.is_none());
        assert!(state.checkout_revision.is_none());
        assert_eq!(state.target_revision.as_deref(), Some(sha.as_str()));
        assert_eq!(state.indexed_revision.as_deref(), Some(sha.as_str()));
        assert!(!state.working_tree_verified);
    }

    #[test]
    fn inspect_commit_wrong_sha_is_stale() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let first = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "first");
        let second = commit_file(repo.path(), "a.rs", "pub fn updated() {}\n", "second");
        let mut store = KuzuGraphStore::open(repo.path()).expect("store");
        apply_tree_index(repo.path(), &mut store, "main", &first).expect("apply_tree_index");
        let state = inspect_commit(&store, "main", &second);
        assert_eq!(state.state, "stale");
        assert_eq!(state.indexed_revision.as_deref(), Some(first.as_str()));
        assert_eq!(state.target_revision.as_deref(), Some(second.as_str()));
    }

    #[test]
    fn inspect_commit_dirty_marker_is_unverified() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "first");
        let mut store = KuzuGraphStore::open(repo.path()).expect("store");
        store
            .set_working_tree_dirty("main", true)
            .expect("set dirty");
        assert_eq!(inspect_commit(&store, "main", &sha).state, "unverified");
        store.set_last_indexed_sha("main", "0000").expect("set sha");
        assert_eq!(inspect_commit(&store, "main", &sha).state, "unverified");
    }

    #[test]
    fn inspect_commit_missing_key_is_missing() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "first");
        let store = KuzuGraphStore::open(repo.path()).expect("store");
        let state = inspect_commit(&store, "never-indexed", &sha);
        assert_eq!(state.state, "missing");
        assert_eq!(state.checkout_branch, None);
        assert!(!state.working_tree_verified);
    }

    #[test]
    fn inspect_checkout_semantics_unchanged() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let mut store = KuzuGraphStore::open(repo.path()).expect("store");
        assert_eq!(inspect(repo.path(), &store, "main").state, "missing");
        apply_tree_index(repo.path(), &mut store, "main", &sha).expect("index");
        assert_eq!(inspect(repo.path(), &store, "main").state, "ready");
        std::fs::write(repo.path().join("a.rs"), "pub fn updated() {}\n").expect("write");
        assert_eq!(inspect(repo.path(), &store, "main").state, "unverified");
        commit_file(repo.path(), "a.rs", "pub fn updated() {}\n", "updated");
        assert_eq!(inspect(repo.path(), &store, "main").state, "stale");
    }

    #[test]
    fn edit_then_settle_reports_ready_and_verified() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        std::fs::create_dir_all(repo.path().join(".gitcortex")).expect("mkdir");
        std::fs::write(repo.path().join(".gitcortex/ignore"), "ignored.rs\n").expect("ignore");
        std::fs::write(repo.path().join("ignored.rs"), "pub fn skipped() {}\n").expect("ignored");
        std::fs::write(repo.path().join("notes.txt"), "unsupported\n").expect("unsupported");
        std::fs::write(repo.path().join("a.rs"), "pub fn edited() {}\n").expect("edit");

        let before = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(before.state, "unverified");
        assert!(!before.working_tree_verified);

        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[
                repo.path().join("a.rs"),
                repo.path().join("ignored.rs"),
                repo.path().join("notes.txt"),
            ],
            &revision,
            &mut snapshots,
        )
        .expect("apply_disk_batch");

        let store_guard = store.lock().expect("lock");
        assert!(store_guard
            .working_tree_dirty("main")
            .expect("dirty stays set"));
        let after = inspect(repo.path(), &*store_guard, "main");
        assert_eq!(after.state, "ready");
        assert!(after.working_tree_verified);
        assert!(!store_guard
            .lookup_symbol("main", "edited", false)
            .expect("lookup")
            .is_empty());
        assert!(store_guard
            .lookup_symbol("main", "skipped", false)
            .expect("lookup")
            .is_empty());
    }

    #[test]
    fn further_edit_after_checkpoint_returns_unverified_until_settled() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        std::fs::write(repo.path().join("a.rs"), "pub fn v1() {}\n").expect("edit v1");
        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle v1");

        let settled_v1 = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(settled_v1.state, "ready");
        assert!(settled_v1.working_tree_verified);

        // Pending watcher events alone block certification even before bytes change.
        let with_pending =
            inspect_with_pending(repo.path(), &*store.lock().expect("lock"), "main", true);
        assert_eq!(with_pending.state, "unverified");
        assert!(!with_pending.working_tree_verified);

        // Further edit on disk invalidates the checkpoint until settled.
        std::fs::write(repo.path().join("a.rs"), "pub fn v2() {}\n").expect("edit v2");
        let unsettled = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(unsettled.state, "unverified");
        assert!(!unsettled.working_tree_verified);

        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle v2");
        let settled_v2 = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(settled_v2.state, "ready");
        assert!(settled_v2.working_tree_verified);
    }

    #[test]
    fn checkpoint_for_other_head_is_not_trusted() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha1 = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "c1");
        let sha2 = commit_file(repo.path(), "b.rs", "pub fn second() {}\n", "c2");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(
            repo.path(),
            &mut *store.lock().expect("lock"),
            "main",
            &sha2,
        )
        .expect("index sha2");

        std::fs::write(repo.path().join("a.rs"), "pub fn edited() {}\n").expect("edit");
        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha2,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle at sha2");
        assert_eq!(
            inspect(repo.path(), &*store.lock().expect("lock"), "main").state,
            "ready"
        );

        // Overwrite checkpoint with another commit's HEAD while keeping store indexed_sha == sha2.
        let mut files = std::collections::BTreeMap::new();
        for (k, v) in &snapshots {
            files.insert(k.clone(), v.clone());
        }
        write_overlay_checkpoint(repo.path(), "main", &sha1, &files).expect("write other head");
        let state = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(state.state, "unverified");
        assert!(!state.working_tree_verified);
    }

    #[test]
    fn changed_file_set_mismatch_is_not_certified() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        std::fs::write(repo.path().join("a.rs"), "pub fn edited() {}\n").expect("edit a");
        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle a");
        assert_eq!(
            inspect(repo.path(), &*store.lock().expect("lock"), "main").state,
            "ready"
        );

        // Case 1: Extra dirty file on disk not in checkpoint.
        std::fs::write(repo.path().join("b.rs"), "pub fn extra() {}\n").expect("write b");
        let extra = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(extra.state, "unverified");
        assert!(!extra.working_tree_verified);
        std::fs::remove_file(repo.path().join("b.rs")).expect("remove b");
        assert_eq!(
            inspect(repo.path(), &*store.lock().expect("lock"), "main").state,
            "ready"
        );

        // Case 2: Checkpoint has extra path that is not dirty on disk.
        let mut superset = std::collections::BTreeMap::new();
        for (k, v) in &snapshots {
            superset.insert(k.clone(), v.clone());
        }
        superset.insert(PathBuf::from("ghost.rs"), None);
        write_overlay_checkpoint(repo.path(), "main", &sha, &superset).expect("write superset");
        let mismatch = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(mismatch.state, "unverified");
        assert!(!mismatch.working_tree_verified);
    }

    #[test]
    fn deleted_file_overlay_certifies_after_settle() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        commit_file(repo.path(), "a.rs", "pub fn keep() {}\n", "add a");
        let sha = commit_file(repo.path(), "b.rs", "pub fn gone() {}\n", "add b");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        std::fs::remove_file(repo.path().join("b.rs")).expect("delete b.rs");
        let before = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(before.state, "unverified");
        assert!(!before.working_tree_verified);

        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("b.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle delete");

        let store_guard = store.lock().expect("lock");
        let after = inspect(repo.path(), &*store_guard, "main");
        assert_eq!(after.state, "ready");
        assert!(after.working_tree_verified);
        assert!(store_guard
            .lookup_symbol("main", "gone", false)
            .expect("lookup gone")
            .is_empty());
    }

    #[test]
    fn restart_with_valid_checkpoint_recertifies_without_reindex() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        std::fs::write(repo.path().join("a.rs"), "pub fn edited() {}\n").expect("edit");
        let revision = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle before restart");
        drop(store);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            // Reopen store as on daemon restart; graph_revision starts at 0.
            let reopened = Arc::new(Mutex::new(
                KuzuGraphStore::open(repo.path()).expect("reopen"),
            ));
            let branch = Arc::new(Mutex::new("main".to_owned()));
            let restart_rev = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let unhealthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let watcher = crate::mcp::watcher::spawn_file_watcher_with_pending(
                repo.path().to_owned(),
                reopened.clone(),
                branch,
                restart_rev.clone(),
                unhealthy.clone(),
                pending.clone(),
            );

            let started = std::time::Instant::now();
            loop {
                if !unhealthy.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(5),
                    "watcher did not become healthy"
                );
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            // Recertified from the persisted checkpoint without running a reindex batch.
            assert_eq!(restart_rev.load(std::sync::atomic::Ordering::Acquire), 0);
            let state = inspect_with_pending(
                repo.path(),
                &*reopened.lock().expect("lock"),
                "main",
                pending.load(std::sync::atomic::Ordering::Acquire),
            );
            assert_eq!(state.state, "ready");
            assert!(state.working_tree_verified);
            watcher.abort();
            let _ = watcher.await;
        });
    }

    #[test]
    fn restart_with_tampered_or_corrupt_checkpoint_stays_unverified() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        std::fs::write(repo.path().join("a.rs"), "pub fn edited() {}\n").expect("edit");
        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("settle");
        assert_eq!(
            inspect(repo.path(), &*store.lock().expect("lock"), "main").state,
            "ready"
        );

        let cp_path = overlay_checkpoint_path(repo.path(), "main");

        // 1. Tampered blob hash in checkpoint JSON.
        let tampered = serde_json::json!({
            "head": sha,
            "branch_key": gitcortex_store::branch::sanitize("main"),
            "files": {
                "a.rs": "0000000000000000000000000000000000000000"
            }
        });
        std::fs::write(&cp_path, tampered.to_string()).expect("write tampered");
        let state_tampered = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(state_tampered.state, "unverified");
        assert!(!state_tampered.working_tree_verified);

        // 2. Tampered branch_key in checkpoint JSON.
        let wrong_branch = serde_json::json!({
            "head": sha,
            "branch_key": "other_branch",
            "files": snapshots,
        });
        std::fs::write(&cp_path, wrong_branch.to_string()).expect("write wrong branch");
        let state_branch = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(state_branch.state, "unverified");
        assert!(!state_branch.working_tree_verified);

        // 3. Corrupt (non-JSON) checkpoint file.
        std::fs::write(&cp_path, b"{not valid json").expect("write corrupt");
        let state_corrupt = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(state_corrupt.state, "unverified");
        assert!(!state_corrupt.working_tree_verified);
    }

    #[test]
    fn oversized_or_many_changed_files_stay_unverified() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let repo = init_repo();
        let sha = commit_file(repo.path(), "a.rs", "pub fn original() {}\n", "fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(repo.path()).expect("store"),
        ));
        apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", &sha)
            .expect("index");

        // Case 1: Oversized source file (> 512 KiB) skipped by AST parser stays unverified.
        let big = format!("pub fn big() {{}}\n{}", "// padding\n".repeat(60_000));
        assert!(big.len() > MAX_OVERLAY_FILE_BYTES as usize);
        std::fs::write(repo.path().join("a.rs"), &big).expect("write oversized");
        let revision = std::sync::atomic::AtomicU64::new(0);
        let mut snapshots = HashMap::new();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("apply oversized");
        let oversized_state = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(oversized_state.state, "unverified");
        assert!(!oversized_state.working_tree_verified);

        // Restore a.rs to a normal edit and verify it certifies.
        std::fs::write(repo.path().join("a.rs"), "pub fn normal() {}\n").expect("write normal");
        snapshots.clear();
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &[repo.path().join("a.rs")],
            &revision,
            &mut snapshots,
        )
        .expect("apply normal");
        assert_eq!(
            inspect(repo.path(), &*store.lock().expect("lock"), "main").state,
            "ready"
        );

        // Case 2: Too many changed files (> MAX_OVERLAY_FILES) stay unverified.
        let mut batch = Vec::new();
        for idx in 0..=MAX_OVERLAY_FILES {
            let p = repo.path().join(format!("many_{idx}.rs"));
            std::fs::write(&p, format!("pub fn f_{idx}() {{}}\n")).expect("write many");
            batch.push(p);
        }
        crate::mcp::watcher::apply_disk_batch(
            repo.path(),
            &store,
            "main",
            &sha,
            &batch,
            &revision,
            &mut snapshots,
        )
        .expect("apply many");
        let many_state = inspect(repo.path(), &*store.lock().expect("lock"), "main");
        assert_eq!(many_state.state, "unverified");
        assert!(!many_state.working_tree_verified);
    }
}
