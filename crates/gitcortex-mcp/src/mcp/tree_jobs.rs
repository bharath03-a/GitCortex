//! Shared synchronous commit indexing and bounded daemon scheduling.
use super::helpers::detect_current_branch;
use gitcortex_core::{
    graph::{GraphDiff, Node, NodeId},
    schema::NodeKind,
    store::GraphStore,
};
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::{branch::sanitize, kuzu::KuzuGraphStore};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
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
/// Full SHA of the local branch `name`; `None` when no such branch exists.
/// Unlike [`resolve_commit`], revision syntax such as `main~1` is not a branch.
pub fn resolve_local_branch(root: &Path, name: &str) -> anyhow::Result<Option<String>> {
    if name.contains('\0') {
        return Ok(None);
    }
    let repo = git2::Repository::open(root)?;
    let branch = match repo.find_branch(name, git2::BranchType::Local) {
        Ok(branch) => branch,
        Err(error)
            if matches!(
                error.code(),
                git2::ErrorCode::NotFound | git2::ErrorCode::InvalidSpec
            ) =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error.into()),
    };
    let sha = branch.get().peel_to_commit()?.id().to_string();
    Ok(Some(sha))
}

/// Explains why `name` has no graph, for a name that is not a local branch.
pub fn missing_branch_message(root: &Path, name: &str) -> String {
    let remote_only = !name.contains('\0')
        && git2::Repository::open(root)
            .is_ok_and(|repo| repo.find_branch(name, git2::BranchType::Remote).is_ok());
    if remote_only {
        format!(
            "{name} is a remote-only branch; remote-only branches are not indexed. Create a local branch from it, then retry."
        )
    } else {
        format!(
            "No local branch named {name}. Check the branch name; an empty index does not mean no dependencies."
        )
    }
}

pub struct TreeIndexOutcome {
    pub sha: String,
    pub nodes: usize,
    pub edges: usize,
    /// The key already held this commit's graph.
    pub unchanged: bool,
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
    previous: Option<String>,
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
        baseline: previous.clone().filter(|_| !dirty),
        previous,
    })
}

fn tree_diff(root: &Path, plan: &TreeApply) -> anyhow::Result<GraphDiff> {
    let (diff, _) = IncrementalIndexer::new(root)?.run_tree(&plan.sha, plan.baseline.as_deref())?;
    Ok(diff)
}

/// `apply_diff` keeps folder nodes across updates, so a folder with no source
/// left under it has to be removed by id.
fn stale_folders(persisted: &[Node], diff: &GraphDiff) -> Vec<NodeId> {
    let is_folder = |node: &&Node| node.kind == NodeKind::Folder;
    let removed: HashSet<&Path> = diff.removed_files.iter().map(PathBuf::as_path).collect();
    let live: HashSet<&Path> = persisted
        .iter()
        .filter(|node| !removed.contains(node.file.as_path()))
        .chain(&diff.added_nodes)
        .filter(|node| !is_folder(node))
        .flat_map(|node| node.file.ancestors().skip(1))
        .collect();
    persisted
        .iter()
        .filter(is_folder)
        .filter(|node| !live.contains(node.file.as_path()))
        .map(|node| node.id.clone())
        .collect()
}

fn finish_tree_apply(
    store: &mut dyn GraphStore,
    key: &str,
    plan: TreeApply,
    mut diff: GraphDiff,
) -> anyhow::Result<TreeIndexOutcome> {
    let rebuild = plan.baseline.is_none();
    if rebuild || diff.source_hashes.iter().any(|(_, hash)| hash.is_none()) {
        let persisted = store.list_all_nodes(key)?;
        if rebuild {
            // Full rebuild: drop files absent from the target tree too.
            let files: BTreeSet<PathBuf> = persisted.iter().map(|n| n.file.clone()).collect();
            diff.removed_files.extend(files);
        }
        let stale = stale_folders(&persisted, &diff);
        diff.removed_node_ids.extend(stale);
    }
    store.apply_diff(key, &diff)?;
    store.set_last_indexed_sha(key, &plan.sha)?;
    store.set_working_tree_dirty(key, false)?;
    Ok(TreeIndexOutcome {
        sha: plan.sha,
        nodes: diff.added_nodes.len(),
        edges: diff.added_edges.len(),
        unchanged: diff.is_empty(),
    })
}

/// The watcher owns the checked-out key, and a tree job must never write it.
fn checkout_owns_key(root: &Path, key: &str) -> anyhow::Result<bool> {
    let checkout = detect_current_branch(root)
        .ok_or_else(|| anyhow::anyhow!("could not determine the current checkout"))?;
    Ok(checkout == key)
}

/// Same sequence as [`apply_tree_index`], parsing blobs with the shared store
/// released. The final list and apply still hold the store mutex, so readiness
/// reports `indexing` instead of queuing behind them (see `before_finish`).
///
/// Returns `None`, leaving the key untouched, when the key is checked out or
/// its SHA moved after `begin`: the watcher wrote it, and this job's commit or
/// baseline no longer describes it. A checkout that lands after the final check
/// is safe because the watcher reconciles every source on a branch change, and
/// waits on this lock to do so.
fn apply_tree_index_shared<G>(
    root: &Path,
    store: &Mutex<KuzuGraphStore>,
    key: &str,
    rev: &str,
    before_finish: impl FnOnce() -> G,
) -> anyhow::Result<Option<TreeIndexOutcome>> {
    let lock = || {
        store
            .lock()
            .map_err(|_| anyhow::anyhow!("store mutex poisoned"))
    };
    let sha = resolve_commit(root, rev)?;
    let plan = {
        let mut store = lock()?;
        if checkout_owns_key(root, key)? {
            return Ok(None);
        }
        begin_tree_apply(&mut *store, key, sha)?
    };
    let diff = tree_diff(root, &plan)?;
    let _finishing = before_finish();
    let mut store = lock()?;
    if checkout_owns_key(root, key)? || store.last_indexed_sha(key)? != plan.previous {
        return Ok(None);
    }
    finish_tree_apply(&mut *store, key, plan, diff).map(Some)
}

/// Set while a job applies or waits to apply, so readiness can answer without
/// queuing on the store mutex behind a long apply.
pub(crate) struct ApplyingGuard(Arc<AtomicBool>);
impl ApplyingGuard {
    fn new(flag: Arc<AtomicBool>) -> Self {
        flag.store(true, Ordering::Release);
        Self(flag)
    }
}
impl Drop for ApplyingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
    applying: Arc<AtomicBool>,
    #[cfg(test)]
    started: Arc<std::sync::atomic::AtomicUsize>,
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
            applying: Arc::default(),
            #[cfg(test)]
            started: Arc::default(),
        }
    }
    #[cfg(test)]
    pub(crate) fn started(&self) -> usize {
        self.started.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub(crate) async fn hold_permit(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.permit
            .clone()
            .acquire_owned()
            .await
            .expect("permit open")
    }
    #[cfg(test)]
    pub(crate) fn mark_applying(&self) -> ApplyingGuard {
        ApplyingGuard::new(self.applying.clone())
    }
    pub(crate) fn is_applying(&self) -> bool {
        self.applying.load(Ordering::Acquire)
    }
    /// Whether any job is queued or running.
    pub fn is_active(&self) -> bool {
        self.entries
            .lock()
            .map(|entries| {
                entries
                    .values()
                    .any(|entry| matches!(entry, JobEntry::Running))
            })
            .unwrap_or(false)
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
        #[cfg(test)]
        self.started.fetch_add(1, Ordering::AcqRel);
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
            let applying = jobs.applying.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                apply_tree_index_shared(&root, &store, &branch, &worker_sha, || {
                    ApplyingGuard::new(applying)
                })
            })
            .await;
            let error = match result {
                Ok(Ok(Some(_))) => None,
                Ok(Ok(None)) => {
                    tracing::info!(key = %key, "tree index skipped: checkout owns the key");
                    None
                }
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
    use crate::mcp::test_support::{commit_file, git, TempRepo, STORE_LOCK};
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

    struct KeyRace {
        repo: TempRepo,
        store: Mutex<KuzuGraphStore>,
        s1: String,
    }

    // main at S0 checked out, branch X at S1 never indexed.
    fn key_race() -> KeyRace {
        let repo = TempRepo::init();
        commit_file(repo.path(), "a.rs", "pub fn base() {}\n", "s0");
        git(repo.path(), &["switch", "-c", "X"]);
        let s1 = commit_file(repo.path(), "a.rs", "pub fn job_bytes() {}\n", "s1");
        git(repo.path(), &["switch", "main"]);
        let store = Mutex::new(KuzuGraphStore::open(repo.path()).expect("store"));
        KeyRace { repo, store, s1 }
    }

    impl KeyRace {
        // What the watcher leaves after syncing X to its new commit.
        fn commit_on_x_and_sync(&self) -> String {
            git(self.repo.path(), &["switch", "X"]);
            let s2 = commit_file(self.repo.path(), "a.rs", "pub fn user_bytes() {}\n", "s2");
            let mut store = self.store.lock().expect("store");
            apply_tree_index(self.repo.path(), &mut *store, "X", &s2).expect("sync");
            s2
        }
        fn assert_untouched_at(&self, s2: &str) {
            let store = self.store.lock().expect("store");
            assert_eq!(
                store.last_indexed_sha("X").expect("sha").as_deref(),
                Some(s2)
            );
            assert!(!store.working_tree_dirty("X").expect("dirty"));
            let has = |name: &str| !store.lookup_symbol("X", name, false).expect("q").is_empty();
            assert!(has("user_bytes"), "checkout content lost");
            assert!(!has("job_bytes"), "tree job content applied");
        }
    }

    #[test]
    fn finish_aborts_when_key_became_checked_out() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let race = key_race();
        let mut s2 = String::new();
        let outcome = apply_tree_index_shared(race.repo.path(), &race.store, "X", &race.s1, || {
            s2 = race.commit_on_x_and_sync();
        })
        .expect("job");
        assert!(outcome.is_none());
        race.assert_untouched_at(&s2);
    }

    #[test]
    fn finish_aborts_when_baseline_moved_even_after_leaving_checkout() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let race = key_race();
        let mut s2 = String::new();
        let outcome = apply_tree_index_shared(race.repo.path(), &race.store, "X", &race.s1, || {
            s2 = race.commit_on_x_and_sync();
            git(race.repo.path(), &["switch", "main"]);
        })
        .expect("job");
        assert!(outcome.is_none());
        race.assert_untouched_at(&s2);
    }

    #[test]
    fn begin_aborts_for_checked_out_key_without_touching_markers() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let race = key_race();
        let s2 = race.commit_on_x_and_sync();
        let outcome = apply_tree_index_shared(race.repo.path(), &race.store, "X", &race.s1, || {
            panic!("a checked-out key must not reach parsing");
        })
        .expect("job");
        assert!(outcome.is_none());
        race.assert_untouched_at(&s2);
    }

    #[test]
    fn begin_preserves_overlay_dirty_marker_of_checked_out_key() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let race = key_race();
        race.commit_on_x_and_sync();
        race.store
            .lock()
            .expect("store")
            .set_working_tree_dirty("X", true)
            .expect("overlay marker");
        let outcome = apply_tree_index_shared(race.repo.path(), &race.store, "X", &race.s1, || {})
            .expect("job");
        assert!(outcome.is_none());
        assert!(race
            .store
            .lock()
            .expect("store")
            .working_tree_dirty("X")
            .expect("dirty"));
    }

    #[test]
    fn job_still_applies_when_key_is_not_checked_out() {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let race = key_race();
        let outcome = apply_tree_index_shared(race.repo.path(), &race.store, "X", &race.s1, || {})
            .expect("job");
        assert_eq!(outcome.expect("applied").sha, race.s1);
        let store = race.store.lock().expect("store");
        assert_eq!(
            super::super::readiness::inspect_commit(&*store, "X", &race.s1).state,
            "ready"
        );
    }

    #[test]
    fn resolve_local_branch_rejects_revision_syntax() {
        let repo = TempRepo::init();
        commit_file(repo.path(), "a.rs", "pub fn a() {}\n", "one");
        let head = commit_file(repo.path(), "a.rs", "pub fn b() {}\n", "two");
        git(repo.path(), &["branch", "feat/x"]);
        for branch in ["main", "feat/x"] {
            assert_eq!(
                resolve_local_branch(repo.path(), branch).expect("lookup"),
                Some(head.clone())
            );
        }
        for not_branch in [
            "main~1",
            "main^",
            "main@{0}",
            "main^{/a}",
            "main^{commit}",
            "HEAD",
            "@",
            "refs/heads/main",
            "../main",
            "",
            "no-such",
            "main\0x",
            head.as_str(),
        ] {
            assert_eq!(
                resolve_local_branch(repo.path(), not_branch).expect("lookup"),
                None,
                "{not_branch:?}"
            );
        }
    }

    #[tokio::test]
    async fn failed_tree_index_reports_error_and_allows_retry() {
        let root = TempRepo::bare_tempdir();
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
