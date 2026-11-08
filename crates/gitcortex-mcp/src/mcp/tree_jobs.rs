//! Shared synchronous commit indexing and bounded daemon scheduling.
use gitcortex_core::{graph::GraphDiff, store::GraphStore};
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::{branch::sanitize, kuzu::KuzuGraphStore};
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Failed jobs are reported for this long before a request retries them.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);

/// Resolve `rev^{commit}` to a full SHA without consulting HEAD or a checkout.
pub fn resolve_commit(root: &Path, rev: &str) -> anyhow::Result<String> {
    Ok(git2::Repository::open(root)?
        .revparse_single(rev)?
        .peel_to_commit()?
        .id()
        .to_string())
}
pub struct TreeIndexOutcome {
    pub sha: String,
    pub nodes: usize,
    pub edges: usize,
}
/// Index `rev`'s committed tree into `key`; certifies the key only on success.
pub fn apply_tree_index(
    root: &Path,
    store: &mut dyn GraphStore,
    key: &str,
    rev: &str,
) -> anyhow::Result<TreeIndexOutcome> {
    let sha = resolve_commit(root, rev)?;
    let plan = begin_tree_apply(store, key, sha)?;
    let diff = tree_diff(root, &plan)?;
    finish_tree_apply(store, key, plan, diff)
}

struct TreeApply {
    sha: String,
    baseline: Option<String>,
}

fn begin_tree_apply(
    store: &mut dyn GraphStore,
    key: &str,
    sha: String,
) -> anyhow::Result<TreeApply> {
    // A dirty key may hold a partial apply or a working-tree overlay, so its
    // SHA is not a trustworthy incremental baseline.
    let dirty = store.working_tree_dirty(key)?;
    let previous = store.last_indexed_sha(key)?;
    store.set_working_tree_dirty(key, true)?;
    Ok(TreeApply {
        sha,
        baseline: previous.filter(|_| !dirty),
    })
}

fn tree_diff(root: &Path, plan: &TreeApply) -> anyhow::Result<GraphDiff> {
    let (diff, _) = IncrementalIndexer::new(root)?.run_tree(&plan.sha, plan.baseline.as_deref())?;
    Ok(diff)
}

fn finish_tree_apply(
    store: &mut dyn GraphStore,
    key: &str,
    plan: TreeApply,
    mut diff: GraphDiff,
) -> anyhow::Result<TreeIndexOutcome> {
    if plan.baseline.is_none() {
        // Full rebuild: drop files absent from the target tree too.
        let persisted: BTreeSet<PathBuf> = store
            .list_all_nodes(key)?
            .into_iter()
            .map(|node| node.file)
            .collect();
        diff.removed_files.extend(persisted);
    }
    store.apply_diff(key, &diff)?;
    store.set_last_indexed_sha(key, &plan.sha)?;
    store.set_working_tree_dirty(key, false)?;
    Ok(TreeIndexOutcome {
        sha: plan.sha,
        nodes: diff.added_nodes.len(),
        edges: diff.added_edges.len(),
    })
}

/// Same sequence as [`apply_tree_index`], releasing the shared store while
/// blobs are parsed so queries and the watcher are not stalled.
fn apply_tree_index_shared(
    root: &Path,
    store: &Mutex<KuzuGraphStore>,
    key: &str,
    rev: &str,
) -> anyhow::Result<TreeIndexOutcome> {
    let lock = || {
        store
            .lock()
            .map_err(|_| anyhow::anyhow!("store mutex poisoned"))
    };
    let sha = resolve_commit(root, rev)?;
    let plan = begin_tree_apply(&mut *lock()?, key, sha)?;
    let diff = tree_diff(root, &plan)?;
    finish_tree_apply(&mut *lock()?, key, plan, diff)
}
#[derive(Clone, Debug)]
pub enum JobStatus {
    Started,
    AlreadyRunning,
    Failed { error: String, retry_after: Instant },
}
enum JobEntry {
    Running,
    Failed {
        sha: String,
        error: String,
        until: Instant,
    },
}
#[derive(Clone)]
pub struct TreeIndexJobs {
    root: PathBuf,
    store: Arc<Mutex<KuzuGraphStore>>,
    entries: Arc<Mutex<HashMap<String, JobEntry>>>,
    permit: Arc<tokio::sync::Semaphore>,
}
struct RunningGuard {
    entries: Arc<Mutex<HashMap<String, JobEntry>>>,
    key: String,
}
impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut entries) = self.entries.lock() {
            if matches!(entries.get(&self.key), Some(JobEntry::Running)) {
                entries.remove(&self.key);
            }
        }
    }
}
impl TreeIndexJobs {
    pub fn new(root: &Path, store: Arc<Mutex<KuzuGraphStore>>) -> Self {
        Self {
            root: root.to_owned(),
            store,
            entries: Arc::default(),
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }
    pub fn request(&self, branch: &str, rev_sha: &str) -> JobStatus {
        let key = sanitize(branch);
        let Ok(mut entries) = self.entries.lock() else {
            return JobStatus::Failed {
                error: "tree job mutex poisoned".into(),
                retry_after: Instant::now() + RETRY_AFTER,
            };
        };
        match entries.get(&key) {
            Some(JobEntry::Running) => return JobStatus::AlreadyRunning,
            Some(JobEntry::Failed { sha, error, until })
                if sha == rev_sha && Instant::now() < *until =>
            {
                return JobStatus::Failed {
                    error: error.clone(),
                    retry_after: *until,
                }
            }
            _ => {}
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return JobStatus::Failed {
                error: "tree indexing requires the daemon runtime".into(),
                retry_after: Instant::now() + RETRY_AFTER,
            };
        };
        entries.insert(key.clone(), JobEntry::Running);
        drop(entries);
        let jobs = self.clone();
        let branch = branch.to_owned();
        let sha = rev_sha.to_owned();
        let guard = RunningGuard {
            entries: self.entries.clone(),
            key: key.clone(),
        };
        runtime.spawn(async move {
            let _guard = guard;
            let Ok(permit) = jobs.permit.clone().acquire_owned().await else {
                return;
            };
            let root = jobs.root.clone();
            let store = jobs.store.clone();
            let worker_sha = sha.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                apply_tree_index_shared(&root, &store, &branch, &worker_sha)
            })
            .await;
            let error = match result {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => Some(format!("{e:#}")),
                Err(e) => Some(e.to_string()),
            };
            if let Some(error) = error {
                tracing::warn!(key = %key, %error, "tree index failed");
                if let Ok(mut entries) = jobs.entries.lock() {
                    entries.insert(
                        key,
                        JobEntry::Failed {
                            sha,
                            error,
                            until: Instant::now() + RETRY_AFTER,
                        },
                    );
                }
            }
        });
        JobStatus::Started
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn running_guard_cleans_up_after_panic() {
        let entries = Arc::new(Mutex::new(HashMap::from([(
            "feature".into(),
            JobEntry::Running,
        )])));
        let shared = entries.clone();
        let result = std::panic::catch_unwind(move || {
            let _guard = RunningGuard {
                entries: shared,
                key: "feature".into(),
            };
            panic!("injected worker panic");
        });
        assert!(result.is_err());
        assert!(entries.lock().expect("entries").is_empty());
    }

    #[tokio::test]
    async fn failed_tree_index_reports_error_and_allows_retry() {
        let root = tempfile::tempdir().expect("fixture");
        let repo = git2::Repository::init(root.path()).expect("repo");
        let blob = repo.blob(b"pub fn target() {}\n").expect("blob");
        let mut tree = repo.treebuilder(None).expect("tree builder");
        tree.insert("lib.rs", blob, 0o100644).expect("entry");
        let tree = repo.find_tree(tree.write().expect("tree")).expect("tree");
        let sig = git2::Signature::now("Test", "test@example.test").expect("sig");
        let sha = repo
            .commit(Some("HEAD"), &sig, &sig, "fixture", &tree, &[])
            .expect("commit")
            .to_string();
        let blob_id = blob.to_string();
        let blob_path = repo
            .path()
            .join("objects")
            .join(&blob_id[..2])
            .join(&blob_id[2..]);
        let bytes = std::fs::read(&blob_path).expect("loose blob");
        std::fs::remove_file(&blob_path).expect("corrupt fixture");
        let store = Arc::new(Mutex::new(
            KuzuGraphStore::open(root.path()).expect("store"),
        ));
        let jobs = TreeIndexJobs::new(root.path(), store.clone());
        assert!(matches!(jobs.request("feature", &sha), JobStatus::Started));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let failed = matches!(
                jobs.entries
                    .lock()
                    .expect("entries")
                    .get(&sanitize("feature")),
                Some(JobEntry::Failed { .. })
            );
            if failed {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        match jobs.request("feature", &sha) {
            JobStatus::Failed { error, retry_after } => {
                assert!(!error.is_empty());
                assert!(retry_after > Instant::now());
            }
            other => panic!("expected failure: {other:?}"),
        }
        std::fs::write(blob_path, bytes).expect("repair blob");
        jobs.entries.lock().expect("entries").insert(
            sanitize("feature"),
            JobEntry::Failed {
                sha: sha.clone(),
                error: "previous".into(),
                until: Instant::now() - Duration::from_secs(1),
            },
        );
        assert!(matches!(jobs.request("feature", &sha), JobStatus::Started));
        // Keep fixture alive until the repaired commit has been applied.
        loop {
            if super::super::readiness::inspect_commit(
                &*store.lock().expect("store"),
                "feature",
                &sha,
            )
            .state
                == "ready"
            {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
