#![cfg(feature = "kuzu-backend")]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use gitcortex_core::store::GraphStore;
use gitcortex_store::{
    branch::{self, BranchKind, IDENTITY_VERSION},
    kuzu::KuzuGraphStore,
    meta::{
        self, read_branch_registry, read_repo_meta, record_repo_open_at, write_repo_meta, RepoMeta,
        LAST_OPENED_THROTTLE_SECS, META_FORMAT_VERSION,
    },
    usage::{data_dir_inventory, scan_store_root, EntryClass, RepoStatus},
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

#[test]
fn meta_written_on_open() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().expect("repo dir");
    fs::create_dir_all(repo.path().join(".git")).expect("create .git dir");

    let store = KuzuGraphStore::open(repo.path()).expect("open store");
    let repo_id = store.repo_id().to_owned();
    drop(store);

    let meta = read_repo_meta(&repo_id)
        .expect("read repo.json")
        .expect("repo.json should exist");
    let expected_root = fs::canonicalize(repo.path()).expect("canonical repo root");
    let expected_common = fs::canonicalize(repo.path().join(".git")).expect("canonical .git");

    assert_eq!(meta.format, META_FORMAT_VERSION);
    assert_eq!(meta.identity_version, IDENTITY_VERSION);
    assert_eq!(meta.worktree_root, expected_root);
    assert_eq!(meta.git_common_dir, expected_common);
    assert!(meta.created_at > 0);
    assert_eq!(meta.created_at, meta.last_opened_at);
}

#[test]
fn meta_last_opened_throttled() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().expect("repo dir");
    let repo_id = branch::storage_repo_id(repo.path());

    let t0 = 1_700_000_000u64;
    let first = record_repo_open_at(&repo_id, repo.path(), t0).expect("first open");
    assert_eq!(first.created_at, t0);
    assert_eq!(first.last_opened_at, t0);

    // Within 3600s window: must NOT rewrite last_opened_at.
    let within = record_repo_open_at(&repo_id, repo.path(), t0 + LAST_OPENED_THROTTLE_SECS - 1)
        .expect("throttled open");
    assert_eq!(within.last_opened_at, t0);
    assert_eq!(within.created_at, t0);

    // At or past 3600s window: updates last_opened_at while preserving created_at.
    let t2 = t0 + LAST_OPENED_THROTTLE_SECS;
    let refreshed = record_repo_open_at(&repo_id, repo.path(), t2).expect("refreshed open");
    assert_eq!(refreshed.created_at, t0);
    assert_eq!(refreshed.last_opened_at, t2);
}

#[test]
fn registry_records_branch_and_detached() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().expect("repo dir");

    let mut store = KuzuGraphStore::open(repo.path()).expect("open store");
    let repo_id = store.repo_id().to_owned();

    // Calling graph_stats exercises ensure_branch; set_last_indexed_sha exercises indexing path.
    let _ = store.graph_stats("main").expect("ensure main");
    store
        .set_last_indexed_sha("feat/auth", "deadbeef1234567890")
        .expect("record feat/auth");
    store
        .set_last_indexed_sha("a1b2c3d", "a1b2c3d4e5f67890")
        .expect("record short-sha detached");
    store
        .set_last_indexed_sha("detached_01234567_a1b2c3d", "a1b2c3d4e5f67890")
        .expect("record worktree detached");
    drop(store);

    let entries = read_branch_registry(&repo_id).expect("read branches.json");
    let by_name: HashMap<&str, &meta::BranchEntry> =
        entries.iter().map(|e| (e.name.as_str(), e)).collect();

    assert_eq!(by_name["main"].kind, BranchKind::Branch);
    assert_eq!(by_name["main"].key, "main");
    assert_eq!(by_name["main"].worktree_id, None);

    assert_eq!(by_name["feat/auth"].kind, BranchKind::Branch);
    assert_eq!(by_name["feat/auth"].key, branch::sanitize("feat/auth"));
    assert_eq!(by_name["feat/auth"].worktree_id, None);

    assert_eq!(by_name["a1b2c3d"].kind, BranchKind::Detached);
    assert_eq!(by_name["a1b2c3d"].worktree_id, None);

    assert_eq!(
        by_name["detached_01234567_a1b2c3d"].kind,
        BranchKind::Detached
    );
    assert_eq!(
        by_name["detached_01234567_a1b2c3d"].worktree_id.as_deref(),
        Some("01234567")
    );
}

#[test]
fn registry_records_commit_key() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().expect("repo dir");

    let mut store = KuzuGraphStore::open(repo.path()).expect("open store");
    let repo_id = store.repo_id().to_owned();
    let full_sha = "0123456789abcdef0123456789abcdef01234567";
    store
        .set_last_indexed_sha(full_sha, full_sha)
        .expect("record commit key");
    drop(store);

    let entries = read_branch_registry(&repo_id).expect("read branches.json");
    let entry = entries
        .iter()
        .find(|e| e.name == full_sha)
        .expect("commit entry recorded");
    assert_eq!(entry.kind, BranchKind::Commit);
    assert_eq!(entry.worktree_id, None);
    assert_eq!(entry.key, branch::sanitize(full_sha));
    assert!(entry.last_indexed_at > 0);
}

#[test]
fn key_kind_distinguishes_commit_detached_branch() {
    let sha1 = "0123456789abcdef0123456789abcdef01234567";
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    assert_eq!(branch::key_kind(sha1), BranchKind::Commit);
    assert_eq!(branch::key_kind(sha256), BranchKind::Commit);

    assert_eq!(branch::key_kind("a1b2c3d"), BranchKind::Detached);
    assert_eq!(branch::key_kind("0123456789ab"), BranchKind::Detached);
    assert_eq!(
        branch::key_kind("detached_deadbeef_a1b2c3d"),
        BranchKind::Detached
    );

    assert_eq!(branch::key_kind("main"), BranchKind::Branch);
    assert_eq!(branch::key_kind("feat/auth"), BranchKind::Branch);
    assert_eq!(branch::key_kind("release/v1.0"), BranchKind::Branch);
    // Short English hex words must remain Branch, not Detached.
    assert_eq!(branch::key_kind("cafe"), BranchKind::Branch);
    assert_eq!(branch::key_kind("dead"), BranchKind::Branch);
    // Uppercase hex is not a git SHA key.
    assert_eq!(branch::key_kind("ABCDEF12"), BranchKind::Branch);
}

#[test]
fn inventory_classifies_every_known_entry() {
    let _guard = StoreRootGuard::new();
    let repo_id = "testrepo00000001";
    let dir = branch::data_dir(repo_id);
    fs::create_dir_all(dir.join("worktrees/wt01")).expect("create worktrees dir");

    // Payload
    fs::write(dir.join("graph.kuzu"), b"db").unwrap();
    fs::write(dir.join("graph.kuzu.wal"), b"wal").unwrap();
    // Durable
    for name in [
        "schema_version",
        "main.sha",
        "main.dirty",
        "main.overlay",
        "main.ignore",
        "main.epoch",
        "embeddings_main.bin",
        "editor_prompted",
        "repo.json",
        "branches.json",
        "last_branch_gc",
    ] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    // Runtime
    for name in [
        "serve.lock",
        "mcp.sock",
        "daemon.log",
        "calls.jsonl",
        "calls.jsonl.1",
        "worktrees/wt01/state",
    ] {
        fs::write(dir.join(name), b"r").unwrap();
    }

    let inv = data_dir_inventory(repo_id).expect("inventory");
    assert!(
        !inv.has_unknown(),
        "unexpected unknown entries: {:?}",
        inv.unknown_entries()
    );

    let by_rel: HashMap<PathBuf, EntryClass> = inv
        .entries
        .iter()
        .map(|(p, c)| (p.strip_prefix(&dir).unwrap().to_path_buf(), *c))
        .collect();

    assert_eq!(by_rel[Path::new("graph.kuzu")], EntryClass::Payload);
    assert_eq!(by_rel[Path::new("graph.kuzu.wal")], EntryClass::Payload);

    for name in [
        "schema_version",
        "main.sha",
        "main.dirty",
        "main.overlay",
        "main.ignore",
        "main.epoch",
        "embeddings_main.bin",
        "editor_prompted",
        "repo.json",
        "branches.json",
        "last_branch_gc",
    ] {
        assert_eq!(by_rel[Path::new(name)], EntryClass::Durable, "for {name}");
    }

    for name in [
        "serve.lock",
        "mcp.sock",
        "daemon.log",
        "calls.jsonl",
        "calls.jsonl.1",
        "worktrees/wt01/state",
    ] {
        assert_eq!(by_rel[Path::new(name)], EntryClass::Runtime, "for {name}");
    }

    // Closed snapshot contract: returns only Payload + Durable, never Runtime.
    let snap = inv.snapshot_entries().expect("snapshot entries");
    assert_eq!(snap.len(), 13);
    for p in snap {
        let rel = p.strip_prefix(&dir).unwrap();
        assert_ne!(by_rel[rel], EntryClass::Runtime);
    }
}

#[test]
fn unknown_entry_is_reported_not_copied() {
    let _guard = StoreRootGuard::new();
    let repo_id = "testrepo00000002";
    let dir = branch::data_dir(repo_id);
    fs::create_dir_all(dir.join("compact-1700000000")).unwrap();
    fs::write(dir.join("graph.kuzu"), b"db").unwrap();
    fs::write(dir.join("graph.kuzu.pre-compact-1700000000"), b"old").unwrap();
    fs::write(dir.join("stray.tmp"), b"junk").unwrap();

    let inv = data_dir_inventory(repo_id).expect("inventory");
    assert!(inv.has_unknown());
    let unknowns: Vec<PathBuf> = inv
        .unknown_entries()
        .into_iter()
        .map(|p| p.strip_prefix(&dir).unwrap().to_path_buf())
        .collect();
    assert!(unknowns.contains(&PathBuf::from("compact-1700000000")));
    assert!(unknowns.contains(&PathBuf::from("graph.kuzu.pre-compact-1700000000")));
    assert!(unknowns.contains(&PathBuf::from("stray.tmp")));

    // Closed snapshot must refuse while any Unknown entry exists.
    let err = inv
        .snapshot_entries()
        .expect_err("snapshot must refuse when unknown entries exist");
    assert!(err.to_string().contains("unknown entries"));
}

#[test]
fn scan_classifies_active() {
    let guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().expect("active repo");
    let store = KuzuGraphStore::open(repo.path()).expect("open store");
    let repo_id = store.repo_id().to_owned();
    drop(store);

    let usages = scan_store_root(guard.root()).expect("scan");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].repo_id, repo_id);
    assert_eq!(usages[0].status, RepoStatus::Active);
    assert!(usages[0].bytes > 0);
    assert!(usages[0].meta.is_some());
}

#[test]
fn scan_classifies_orphaned() {
    let guard = StoreRootGuard::new();
    let parent = tempfile::tempdir().expect("existing parent");
    let deleted_repo = parent.path().join("deleted-repo");
    fs::create_dir_all(&deleted_repo).unwrap();

    let repo_id = "orphan0000000001";
    write_repo_meta(
        repo_id,
        &RepoMeta {
            format: META_FORMAT_VERSION,
            identity_version: IDENTITY_VERSION,
            worktree_root: deleted_repo.clone(),
            git_common_dir: deleted_repo.join(".git"),
            created_at: 100,
            last_opened_at: 100,
        },
    )
    .expect("write meta");
    fs::remove_dir_all(&deleted_repo).unwrap();

    let usages = scan_store_root(guard.root()).expect("scan");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].repo_id, repo_id);
    assert_eq!(usages[0].status, RepoStatus::Orphaned);
}

#[test]
fn missing_parent_is_unreachable() {
    let guard = StoreRootGuard::new();
    let base = tempfile::tempdir().expect("temp base");
    let unmounted_repo = base.path().join("unmounted-volume").join("my-repo");
    // Neither unmounted-volume nor my-repo exists.

    let repo_id = "unreach000000001";
    write_repo_meta(
        repo_id,
        &RepoMeta {
            format: META_FORMAT_VERSION,
            identity_version: IDENTITY_VERSION,
            worktree_root: unmounted_repo.clone(),
            git_common_dir: unmounted_repo.join(".git"),
            created_at: 100,
            last_opened_at: 100,
        },
    )
    .expect("write meta");

    let usages = scan_store_root(guard.root()).expect("scan");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].repo_id, repo_id);
    assert_eq!(usages[0].status, RepoStatus::Unreachable);
}

#[test]
fn scan_unknown_without_meta() {
    let guard = StoreRootGuard::new();
    let dir = guard.root().join("legacy0000000001");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("graph.kuzu"), b"legacy-payload").unwrap();

    let usages = scan_store_root(guard.root()).expect("scan");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].repo_id, "legacy0000000001");
    assert_eq!(usages[0].status, RepoStatus::Unknown);
    assert!(usages[0].meta.is_none());
    assert_eq!(usages[0].bytes, b"legacy-payload".len() as u64);
}

#[test]
fn scan_backup_suffix() {
    let guard = StoreRootGuard::new();
    let dir = guard.root().join("x.bak-1787394447");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("graph.kuzu"), b"backup-bytes").unwrap();

    let usages = scan_store_root(guard.root()).expect("scan");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].repo_id, "x.bak-1787394447");
    assert_eq!(usages[0].status, RepoStatus::Backup);
    assert_eq!(usages[0].bytes, b"backup-bytes".len() as u64);
}
