#![cfg(feature = "kuzu-backend")]

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use fs2::FileExt;
use gitcortex_store::{
    branch::IDENTITY_VERSION,
    meta::{write_repo_meta_in_dir, RepoMeta, META_FORMAT_VERSION},
    usage::{apply_prune, plan_prune, PrunePolicy},
};
use tempfile::TempDir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct StoreRootGuard {
    _lock: MutexGuard<'static, ()>,
    _dir: TempDir,
    prev: Option<std::ffi::OsString>,
    root: PathBuf,
}

impl StoreRootGuard {
    fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("temp store root");
        let root = dir.path().to_path_buf();
        let prev = std::env::var_os("GCX_STORE_PATH");
        std::env::set_var("GCX_STORE_PATH", &root);
        Self {
            _lock: lock,
            _dir: dir,
            prev,
            root,
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for StoreRootGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("GCX_STORE_PATH", v),
            None => std::env::remove_var("GCX_STORE_PATH"),
        }
    }
}

fn create_store_with_meta(
    root: &Path,
    id: &str,
    worktree_root: PathBuf,
    payload: &[u8],
) -> PathBuf {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("graph.kuzu"), payload).unwrap();
    write_repo_meta_in_dir(
        &dir,
        &RepoMeta {
            format: META_FORMAT_VERSION,
            identity_version: IDENTITY_VERSION,
            worktree_root: worktree_root.clone(),
            git_common_dir: worktree_root.join(".git"),
            created_at: 100,
            last_opened_at: 100,
        },
    )
    .unwrap();
    dir
}

#[test]
fn dry_run_deletes_nothing() {
    let guard = StoreRootGuard::new();
    let parent = tempfile::tempdir().unwrap();
    let deleted = parent.path().join("gone-repo");
    let orphan_dir = create_store_with_meta(guard.root(), "orphan1", deleted, b"orphan-data");
    let backup_dir = guard.root().join("abc.bak-1787394447");
    fs::create_dir_all(&backup_dir).unwrap();
    fs::write(backup_dir.join("graph.kuzu"), b"backup-data").unwrap();

    let plan = plan_prune(guard.root(), &PrunePolicy::default()).expect("plan_prune");
    assert_eq!(plan.items.len(), 2);
    assert!(plan.bytes > 0);
    // Dry run must leave everything intact on disk.
    assert!(orphan_dir.exists());
    assert!(backup_dir.exists());
}

#[test]
fn apply_removes_orphaned_only() {
    let guard = StoreRootGuard::new();
    let active_repo = tempfile::tempdir().unwrap();
    let active_dir = create_store_with_meta(
        guard.root(),
        "active1",
        active_repo.path().to_path_buf(),
        b"active-data",
    );

    let parent = tempfile::tempdir().unwrap();
    let orphan_dir = create_store_with_meta(
        guard.root(),
        "orphan1",
        parent.path().join("removed-repo"),
        b"orphan-data",
    );

    let backup_dir = guard.root().join("old.bak-1787394447");
    fs::create_dir_all(&backup_dir).unwrap();
    fs::write(backup_dir.join("graph.kuzu"), b"backup-data").unwrap();

    let policy = PrunePolicy {
        orphaned: true,
        backups: false,
        unknown_older_than_days: None,
    };
    let plan = plan_prune(guard.root(), &policy).expect("plan");
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].0.repo_id, "orphan1");
    assert_eq!(plan.items[0].1, "orphaned");

    let report = apply_prune(&plan).expect("apply");
    assert_eq!(report.removed, plan.bytes);
    assert!(report.skipped_locked.is_empty());
    assert!(!orphan_dir.exists());
    assert!(active_dir.exists());
    assert!(backup_dir.exists());
}

#[test]
fn backups_removed_with_flag() {
    let guard = StoreRootGuard::new();
    let backup_dir = guard.root().join("repo1.bak-1787394447");
    fs::create_dir_all(&backup_dir).unwrap();
    fs::write(backup_dir.join("graph.kuzu"), b"backup-bytes").unwrap();

    let no_backups = plan_prune(
        guard.root(),
        &PrunePolicy {
            orphaned: true,
            backups: false,
            unknown_older_than_days: None,
        },
    )
    .unwrap();
    assert!(no_backups.items.is_empty());

    let with_backups = plan_prune(
        guard.root(),
        &PrunePolicy {
            orphaned: false,
            backups: true,
            unknown_older_than_days: None,
        },
    )
    .unwrap();
    assert_eq!(with_backups.items.len(), 1);
    assert_eq!(with_backups.items[0].1, "backup");

    let report = apply_prune(&with_backups).unwrap();
    assert_eq!(report.removed, b"backup-bytes".len() as u64);
    assert!(!backup_dir.exists());
}

#[test]
fn pre_compact_leftover_removed_with_backups() {
    let guard = StoreRootGuard::new();
    let active_repo = tempfile::tempdir().unwrap();
    let active_dir = create_store_with_meta(
        guard.root(),
        "active1",
        active_repo.path().to_path_buf(),
        b"live-db",
    );
    let leftover = active_dir.join("graph.kuzu.pre-compact-1700000000");
    let leftover_wal = active_dir.join("graph.kuzu.pre-compact-1700000000.wal");
    fs::write(&leftover, b"old-payload-bytes").unwrap();
    fs::write(&leftover_wal, b"old-wal").unwrap();

    let plan = plan_prune(
        guard.root(),
        &PrunePolicy {
            orphaned: false,
            backups: true,
            unknown_older_than_days: None,
        },
    )
    .unwrap();
    assert_eq!(plan.items.len(), 2);
    assert_eq!(
        plan.bytes,
        (b"old-payload-bytes".len() + b"old-wal".len()) as u64
    );

    let report = apply_prune(&plan).unwrap();
    assert_eq!(report.removed, plan.bytes);
    assert!(report.skipped_locked.is_empty());
    // Leftovers gone, live repo store intact!
    assert!(!leftover.exists());
    assert!(!leftover_wal.exists());
    assert!(active_dir.join("graph.kuzu").exists());
    assert!(active_dir.join("repo.json").exists());
}

#[test]
fn unknown_dirs_not_pruned_without_flag() {
    let guard = StoreRootGuard::new();
    let unknown_dir = guard.root().join("upgraded-user-repo");
    fs::create_dir_all(&unknown_dir).unwrap();
    fs::write(unknown_dir.join("graph.kuzu"), b"legacy-db").unwrap();

    let plan = plan_prune(guard.root(), &PrunePolicy::default()).unwrap();
    assert!(plan.items.is_empty());

    let report = apply_prune(&plan).unwrap();
    assert_eq!(report.removed, 0);
    assert!(unknown_dir.exists());
}

#[test]
fn unknown_older_than_uses_graph_mtime() {
    let guard = StoreRootGuard::new();
    let old_dir = guard.root().join("unknown-old");
    let fresh_dir = guard.root().join("unknown-fresh");
    fs::create_dir_all(&old_dir).unwrap();
    fs::create_dir_all(&fresh_dir).unwrap();

    let old_graph = old_dir.join("graph.kuzu");
    let fresh_graph = fresh_dir.join("graph.kuzu");
    fs::write(&old_graph, b"old-unknown-graph").unwrap();
    fs::write(&fresh_graph, b"fresh-unknown-graph").unwrap();

    // Set old_graph mtime to 45 days ago.
    let old_time = SystemTime::now() - Duration::from_secs(45 * 86_400);
    let f = File::options().write(true).open(&old_graph).unwrap();
    f.set_modified(old_time).unwrap();
    drop(f);

    let plan = plan_prune(
        guard.root(),
        &PrunePolicy {
            orphaned: false,
            backups: false,
            unknown_older_than_days: Some(30),
        },
    )
    .unwrap();
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].0.repo_id, "unknown-old");

    let report = apply_prune(&plan).unwrap();
    assert_eq!(report.removed, b"old-unknown-graph".len() as u64);
    assert!(!old_dir.exists());
    assert!(fresh_dir.exists());
}

#[test]
fn locked_dir_skipped() {
    let guard = StoreRootGuard::new();
    let parent = tempfile::tempdir().unwrap();
    let orphan_dir = create_store_with_meta(
        guard.root(),
        "locked-orphan",
        parent.path().join("gone-repo"),
        b"locked-data",
    );

    // Hold exclusive fs2 lock on serve.lock.
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(orphan_dir.join("serve.lock"))
        .unwrap();
    lock_file.try_lock_exclusive().expect("acquire serve.lock");

    let plan = plan_prune(guard.root(), &PrunePolicy::default()).unwrap();
    assert_eq!(plan.items.len(), 1);

    let report = apply_prune(&plan).unwrap();
    assert_eq!(report.removed, 0);
    assert_eq!(report.skipped_locked, vec![orphan_dir.clone()]);
    assert!(orphan_dir.exists());

    // Releasing the lock allows pruning to succeed.
    drop(lock_file);
    let report2 = apply_prune(&plan).unwrap();
    assert_eq!(report2.removed, plan.bytes);
    assert!(report2.skipped_locked.is_empty());
    assert!(!orphan_dir.exists());
}

#[test]
fn unreachable_never_pruned() {
    let guard = StoreRootGuard::new();
    let base = tempfile::tempdir().unwrap();
    let unmounted_repo = base.path().join("unmounted-drive").join("project");
    let unreach_dir =
        create_store_with_meta(guard.root(), "unreach1", unmounted_repo, b"unreach-data");

    let plan = plan_prune(
        guard.root(),
        &PrunePolicy {
            orphaned: true,
            backups: true,
            unknown_older_than_days: Some(0),
        },
    )
    .unwrap();
    assert!(plan.items.is_empty());

    let report = apply_prune(&plan).unwrap();
    assert_eq!(report.removed, 0);
    assert!(unreach_dir.exists());
}
