//! Watcher scope changes: `.gitcortex/ignore` rule edits and git-ignored files.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;

use super::readiness::{inspect, inspect_with_pending, IndexReadiness};
use super::test_support::{commit_file, git, lock_stores, TempRepo};
use super::tree_jobs::apply_tree_index;
use super::watcher::{apply_disk_batch, reconcile_disk, sync_commit};

type Store = Arc<Mutex<KuzuGraphStore>>;

fn indexed(repo: &TempRepo, sha: &str) -> Store {
    let store = Arc::new(Mutex::new(
        KuzuGraphStore::open(repo.path()).expect("store"),
    ));
    apply_tree_index(repo.path(), &mut *store.lock().expect("lock"), "main", sha).expect("index");
    store
}

fn batch(repo: &TempRepo, store: &Store, head: &str, rels: &[&str]) {
    let paths: Vec<PathBuf> = rels.iter().map(|rel| repo.path().join(rel)).collect();
    let mut snapshots = HashMap::new();
    apply_disk_batch(
        repo.path(),
        store,
        "main",
        head,
        &paths,
        &AtomicU64::new(0),
        &mut snapshots,
    )
    .expect("apply_disk_batch");
}

fn has(store: &Store, name: &str) -> bool {
    !store
        .lock()
        .expect("lock")
        .lookup_symbol("main", name, false)
        .expect("lookup")
        .is_empty()
}

fn files(store: &Store) -> Vec<PathBuf> {
    store
        .lock()
        .expect("lock")
        .list_all_nodes("main")
        .expect("nodes")
        .into_iter()
        .map(|node| node.file)
        .collect()
}

fn state(repo: &TempRepo, store: &Store) -> IndexReadiness {
    inspect(repo.path(), &*store.lock().expect("lock"), "main")
}

fn assert_uncertified(readiness: &IndexReadiness) {
    assert_ne!(readiness.state, "ready", "{readiness:?}");
    assert!(!readiness.working_tree_verified, "{readiness:?}");
}

fn assert_certified(readiness: &IndexReadiness) {
    assert_eq!(readiness.state, "ready", "{readiness:?}");
    assert!(readiness.working_tree_verified, "{readiness:?}");
}

/// a.rs and b.rs are indexed under a committed ignore that excludes neither.
fn tracked_ignore_fixture() -> (TempRepo, String) {
    let repo = TempRepo::init();
    commit_file(repo.path(), ".gitcortex/ignore", "unused.rs\n", "rules");
    commit_file(repo.path(), "b.rs", "pub fn beta() {}\n", "b");
    let head = commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
    (repo, head)
}

fn ignore_a(repo: &TempRepo) {
    std::fs::write(repo.path().join(".gitcortex/ignore"), "unused.rs\na.rs\n").expect("rules");
}

#[test]
fn newly_ignored_file_edited_on_clean_path_drops_nodes_and_is_not_certified() {
    let _stores = lock_stores();
    let (repo, head) = tracked_ignore_fixture();
    let store = indexed(&repo, &head);
    ignore_a(&repo);
    std::fs::write(repo.path().join("a.rs"), "pub fn alpha_edited() {}\n").expect("edit");
    batch(&repo, &store, &head, &["a.rs"]);
    assert!(
        !has(&store, "alpha"),
        "stale nodes of an ignored file remain"
    );
    assert!(!has(&store, "alpha_edited"), "ignored bytes were indexed");
    assert!(has(&store, "beta"));
    assert_uncertified(&state(&repo, &store));
}

#[test]
fn newly_ignored_file_deleted_on_clean_path_drops_nodes_and_is_not_certified() {
    let _stores = lock_stores();
    let (repo, head) = tracked_ignore_fixture();
    let store = indexed(&repo, &head);
    ignore_a(&repo);
    std::fs::remove_file(repo.path().join("a.rs")).expect("delete");
    batch(&repo, &store, &head, &["a.rs"]);
    assert!(
        !has(&store, "alpha"),
        "stale nodes of an ignored file remain"
    );
    assert_uncertified(&state(&repo, &store));
}

#[test]
fn newly_ignored_file_on_dirty_path_drops_nodes_and_is_not_certified() {
    let _stores = lock_stores();
    let (repo, head) = tracked_ignore_fixture();
    let store = indexed(&repo, &head);
    ignore_a(&repo);
    std::fs::write(repo.path().join("a.rs"), "pub fn alpha_edited() {}\n").expect("edit a");
    std::fs::write(repo.path().join("b.rs"), "pub fn beta_edited() {}\n").expect("edit b");
    batch(&repo, &store, &head, &["a.rs", "b.rs"]);
    assert!(
        !has(&store, "alpha"),
        "stale nodes of an ignored file remain"
    );
    assert!(!has(&store, "alpha_edited"));
    assert!(has(&store, "beta_edited"));
    assert_uncertified(&state(&repo, &store));
    std::fs::remove_file(repo.path().join("a.rs")).expect("delete a");
    batch(&repo, &store, &head, &["a.rs"]);
    assert!(!has(&store, "alpha"));
    assert_uncertified(&state(&repo, &store));
}

#[test]
fn modified_tracked_ignore_blocks_a_clean_checkout() {
    let _stores = lock_stores();
    let (repo, head) = tracked_ignore_fixture();
    let store = indexed(&repo, &head);
    assert_certified(&state(&repo, &store));
    ignore_a(&repo);
    assert_uncertified(&state(&repo, &store));
    batch(&repo, &store, &head, &[]);
    assert_uncertified(&state(&repo, &store));
    std::fs::write(repo.path().join(".gitcortex/ignore"), "unused.rs\n").expect("revert");
    batch(&repo, &store, &head, &[]);
    assert_certified(&state(&repo, &store));
}

#[test]
fn ignore_edited_after_an_overlay_checkpoint_is_not_certified() {
    let _stores = lock_stores();
    let repo = TempRepo::init();
    commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
    let head = commit_file(repo.path(), "b.rs", "pub fn beta() {}\n", "b");
    let store = indexed(&repo, &head);
    std::fs::write(repo.path().join("b.rs"), "pub fn beta_edited() {}\n").expect("edit");
    batch(&repo, &store, &head, &["b.rs"]);
    assert_certified(&state(&repo, &store));

    // An untracked rules file, as `gcx init` writes, that now excludes a.rs.
    std::fs::create_dir_all(repo.path().join(".gitcortex")).expect("mkdir");
    std::fs::write(repo.path().join(".gitcortex/ignore"), "a.rs\n").expect("rules");
    assert_uncertified(&state(&repo, &store));
    let changed = super::readiness::changed_sources(repo.path()).expect("changed");
    assert!(
        super::readiness::verify_overlay_checkpoint(repo.path(), "main", &head, &changed).is_none(),
        "restart must not recertify a checkpoint taken under other rules"
    );
}

#[test]
fn committed_rule_change_drops_newly_ignored_files_on_sync() {
    let _stores = lock_stores();
    let (repo, _) = tracked_ignore_fixture();
    let base = commit_file(repo.path(), "c.rs", "pub fn gamma() {}\n", "c");
    let store = indexed(&repo, &base);
    let head = commit_file(
        repo.path(),
        ".gitcortex/ignore",
        "unused.rs\na.rs\nc.rs\n",
        "ignore a and c",
    );
    let mut snapshots = HashMap::new();
    sync_commit(repo.path(), &store, "main", &head, true, &mut snapshots).expect("sync");
    apply_disk_batch(
        repo.path(),
        &store,
        "main",
        &head,
        &[],
        &AtomicU64::new(0),
        &mut snapshots,
    )
    .expect("batch");
    assert!(!has(&store, "alpha"), "committed rules left a.rs nodes");
    assert!(!has(&store, "gamma"), "committed rules left c.rs nodes");
    assert!(has(&store, "beta"));
    assert_certified(&state(&repo, &store));
}

#[test]
fn watcher_reconciles_when_ignore_rules_change() {
    super::test_support::run_async(|repo| async move {
        commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
        let head = commit_file(repo.path(), "c.rs", "pub fn gamma() {}\n", "c");
        let store = indexed(&repo, &head);
        let unhealthy = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicBool::new(false));
        let watcher = super::watcher::spawn_file_watcher_with_pending(
            repo.path().to_owned(),
            store.clone(),
            Arc::new(Mutex::new("main".to_owned())),
            Arc::new(AtomicU64::new(0)),
            unhealthy.clone(),
            pending.clone(),
        );
        wait_until(Duration::from_secs(10), || {
            !unhealthy.load(Ordering::Acquire) && !pending.load(Ordering::Acquire)
        })
        .await;
        std::fs::create_dir_all(repo.path().join(".gitcortex")).expect("mkdir");
        std::fs::write(repo.path().join(".gitcortex/ignore"), "c.rs\n").expect("rules");
        wait_until(Duration::from_secs(10), || !has(&store, "gamma")).await;
        assert!(has(&store, "alpha"));
        watcher.abort();
        let _ = watcher.await;
    });
}

async fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < limit,
            "condition not reached in {limit:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn gitignored_fixture() -> (TempRepo, String) {
    let repo = TempRepo::init();
    commit_file(repo.path(), ".gitignore", ".venv/\ngen/\n", "gitignore");
    let head = commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
    std::fs::create_dir_all(repo.path().join(".venv/lib")).expect("venv");
    std::fs::write(
        repo.path().join(".venv/lib/site.py"),
        "def vendored():\n    pass\n",
    )
    .expect("vendored");
    (repo, head)
}

fn seed_gitignored_nodes(repo: &TempRepo, store: &Store, rel: &str) {
    // What a daemon without the git-ignore filter persisted.
    let diff = gitcortex_indexer::IncrementalIndexer::new(repo.path())
        .expect("indexer")
        .index_files_from_disk(&[repo.path().join(rel)])
        .expect("index");
    let mut guard = store.lock().expect("lock");
    guard.set_working_tree_dirty("main", true).expect("dirty");
    guard.apply_diff("main", &diff).expect("apply");
}

#[test]
fn gitignored_file_does_not_block_a_dirty_checkout() {
    let _stores = lock_stores();
    let (repo, head) = gitignored_fixture();
    let store = indexed(&repo, &head);
    std::fs::write(repo.path().join("a.rs"), "pub fn alpha_edited() {}\n").expect("edit");
    batch(&repo, &store, &head, &["a.rs", ".venv/lib/site.py"]);
    assert!(!has(&store, "vendored"), "git-ignored file was indexed");
    assert!(has(&store, "alpha_edited"));
    assert_certified(&state(&repo, &store));
}

#[test]
fn gitignored_file_does_not_block_a_clean_checkout() {
    let _stores = lock_stores();
    let (repo, head) = gitignored_fixture();
    let store = indexed(&repo, &head);
    batch(&repo, &store, &head, &[".venv/lib/site.py"]);
    assert!(!has(&store, "vendored"), "git-ignored file was indexed");
    assert_certified(&state(&repo, &store));
}

#[test]
fn tracked_file_under_a_gitignored_directory_is_still_indexed() {
    let _stores = lock_stores();
    let (repo, _) = gitignored_fixture();
    std::fs::create_dir_all(repo.path().join("gen")).expect("gen");
    std::fs::write(repo.path().join("gen/keep.rs"), "pub fn kept() {}\n").expect("keep");
    git(repo.path(), &["add", "-f", "gen/keep.rs"]);
    git(repo.path(), &["commit", "-m", "forced"]);
    let head = git(repo.path(), &["rev-parse", "HEAD"]);
    let store = indexed(&repo, &head);
    std::fs::write(repo.path().join("gen/keep.rs"), "pub fn kept_edited() {}\n").expect("edit");
    batch(&repo, &store, &head, &["gen/keep.rs"]);
    assert!(has(&store, "kept_edited"));
    assert!(!has(&store, "kept"));
    assert_certified(&state(&repo, &store));
}

#[test]
fn reconcile_drops_gitignored_nodes_left_by_an_older_daemon() {
    let _stores = lock_stores();
    let (repo, head) = gitignored_fixture();
    let store = indexed(&repo, &head);
    seed_gitignored_nodes(&repo, &store, ".venv/lib/site.py");
    assert!(has(&store, "vendored"));
    let mut snapshots = HashMap::new();
    reconcile_disk(
        repo.path(),
        &store,
        "main",
        &head,
        &AtomicU64::new(0),
        &mut snapshots,
    )
    .expect("reconcile");
    assert!(!has(&store, "vendored"));
    assert!(files(&store)
        .iter()
        .all(|file| !file.starts_with(Path::new(".venv"))));
    assert_certified(&state(&repo, &store));
}

#[test]
fn gitignored_nodes_from_before_a_restart_do_not_block_certification() {
    super::test_support::run_async(|repo| async move {
        commit_file(repo.path(), ".gitignore", ".venv/\n", "gitignore");
        let head = commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
        std::fs::create_dir_all(repo.path().join(".venv")).expect("venv");
        std::fs::write(
            repo.path().join(".venv/site.py"),
            "def vendored():\n    pass\n",
        )
        .expect("vendored");
        let store = indexed(&repo, &head);
        seed_gitignored_nodes(&repo, &store, ".venv/site.py");
        let unhealthy = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicBool::new(false));
        let watcher = super::watcher::spawn_file_watcher_with_pending(
            repo.path().to_owned(),
            store.clone(),
            Arc::new(Mutex::new("main".to_owned())),
            Arc::new(AtomicU64::new(0)),
            unhealthy.clone(),
            pending.clone(),
        );
        wait_until(Duration::from_secs(10), || {
            let pending = pending.load(Ordering::Acquire);
            !unhealthy.load(Ordering::Acquire)
                && inspect_with_pending(repo.path(), &*store.lock().expect("lock"), "main", pending)
                    .working_tree_verified
        })
        .await;
        assert!(!has(&store, "vendored"));
        watcher.abort();
        let _ = watcher.await;
    });
}

#[test]
fn checkpoint_removal_failure_aborts_before_the_graph_is_mutated() {
    let _stores = lock_stores();
    let repo = TempRepo::init();
    let head = commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
    let store = indexed(&repo, &head);
    let epoch = store
        .lock()
        .expect("lock")
        .mutation_epoch("main")
        .expect("epoch");
    // A non-empty directory where the checkpoint lives cannot be removed.
    let checkpoint = super::readiness::overlay_checkpoint_path(repo.path(), "main");
    std::fs::create_dir_all(checkpoint.join("blocker")).expect("blocker");
    std::fs::write(repo.path().join("a.rs"), "pub fn alpha_edited() {}\n").expect("edit");
    let result = apply_disk_batch(
        repo.path(),
        &store,
        "main",
        &head,
        &[repo.path().join("a.rs")],
        &AtomicU64::new(0),
        &mut HashMap::new(),
    );
    std::fs::remove_dir_all(&checkpoint).expect("cleanup");
    assert!(result.is_err(), "a stale checkpoint must not be ignored");
    assert!(has(&store, "alpha"), "graph mutated after a failed removal");
    assert_eq!(
        store
            .lock()
            .expect("lock")
            .mutation_epoch("main")
            .expect("epoch"),
        epoch
    );
}

#[test]
fn overlay_over_the_total_byte_budget_writes_no_checkpoint() {
    let _stores = lock_stores();
    let repo = TempRepo::init();
    let head = commit_file(repo.path(), "a.rs", "pub fn alpha() {}\n", "a");
    let store = indexed(&repo, &head);
    let per_file = (super::readiness::MAX_OVERLAY_FILE_BYTES - 1024) as usize;
    let count = (super::readiness::MAX_OVERLAY_TOTAL_BYTES as usize / per_file) + 1;
    let mut rels = Vec::new();
    for index in 0..count {
        let rel = format!("big_{index}.rs");
        let body = format!("pub fn big_{index}() {{}}\n");
        let padding = "// padding\n".repeat((per_file - body.len()) / 11);
        std::fs::write(repo.path().join(&rel), format!("{body}{padding}")).expect("write");
        rels.push(rel);
    }
    let rels: Vec<&str> = rels.iter().map(String::as_str).collect();
    batch(&repo, &store, &head, &rels);
    assert!(
        !super::readiness::overlay_checkpoint_path(repo.path(), "main").exists(),
        "a checkpoint over the byte budget can never verify"
    );
    assert_uncertified(&state(&repo, &store));
}
