#![cfg(feature = "kuzu-backend")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use gitcortex_core::{
    graph::{GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{NodeKind, Visibility},
    store::GraphStore,
};
use gitcortex_store::{
    branch::{self, BranchKind},
    kuzu::KuzuGraphStore,
    meta::{
        branch_gc_due, read_branch_registry, write_branch_registry, write_last_branch_gc,
        BRANCH_GC_INTERVAL_SECS,
    },
};
use tempfile::TempDir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct StoreRootGuard {
    _lock: MutexGuard<'static, ()>,
    _dir: TempDir,
    prev: Option<std::ffi::OsString>,
}

impl StoreRootGuard {
    fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("temp store root");
        let prev = std::env::var_os("GCX_STORE_PATH");
        std::env::set_var("GCX_STORE_PATH", dir.path());
        Self {
            _lock: lock,
            _dir: dir,
            prev,
        }
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

fn seed_branch(store: &mut KuzuGraphStore, branch_name: &str, symbol_name: &str, sha: &str) {
    let diff = GraphDiff {
        added_nodes: vec![Node {
            id: NodeId::new(),
            kind: NodeKind::Function,
            name: symbol_name.to_owned(),
            qualified_name: format!("crate::{symbol_name}"),
            file: PathBuf::from("src/lib.rs"),
            span: Span {
                start_line: 1,
                end_line: 5,
            },
            metadata: NodeMetadata {
                loc: 5,
                visibility: Visibility::Pub,
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    store.apply_diff(branch_name, &diff).expect("apply diff");
    store
        .set_last_indexed_sha(branch_name, sha)
        .expect("set sha");
}

fn create_branch_sidecars(repo_id: &str, branch_name: &str) -> [PathBuf; 3] {
    let dir = branch::data_dir(repo_id);
    let key = branch::sanitize(branch_name);
    let sha = dir.join(format!("{key}.sha"));
    let dirty = dir.join(format!("{key}.dirty"));
    let emb = dir.join(format!("embeddings_{key}.bin"));
    fs::write(&dirty, b"dirty").unwrap();
    fs::write(&emb, b"embeddings").unwrap();
    [sha, dirty, emb]
}

#[test]
fn deleted_branch_tables_dropped() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let mut store = KuzuGraphStore::open(repo.path()).unwrap();
    let repo_id = store.repo_id().to_owned();

    seed_branch(
        &mut store,
        "main",
        "fn_main",
        "1111111111111111111111111111111111111111",
    );
    seed_branch(
        &mut store,
        "a",
        "fn_a",
        "2222222222222222222222222222222222222222",
    );
    seed_branch(
        &mut store,
        "b",
        "fn_b",
        "3333333333333333333333333333333333333333",
    );
    let b_sidecars = create_branch_sidecars(&repo_id, "b");
    assert!(b_sidecars.iter().all(|p| p.exists()));

    // Simulate branch `b` deleted in git: only `main` and `a` remain.
    let report = store
        .prune_branches(&|entry| matches!(entry.name.as_str(), "main" | "a"))
        .expect("prune_branches");
    assert_eq!(report.dropped, vec!["b".to_owned()]);
    assert_eq!(report.kept, 2);

    // `b`'s tables, sidecars, and registry entry are gone.
    let tables = store.list_branch_tables().unwrap();
    assert!(!tables.contains(&"b".to_owned()));
    assert!(tables.contains(&"main".to_owned()));
    assert!(tables.contains(&"a".to_owned()));
    assert!(b_sidecars.iter().all(|p| !p.exists()));

    let reg = read_branch_registry(&repo_id).unwrap();
    assert!(!reg.iter().any(|e| e.name == "b"));

    // `main` and `a` queries are completely unchanged.
    assert_eq!(
        store.lookup_symbol("main", "fn_main", false).unwrap().len(),
        1
    );
    assert_eq!(store.lookup_symbol("a", "fn_a", false).unwrap().len(), 1);
}

#[test]
fn stale_detached_dropped_current_kept() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let mut store = KuzuGraphStore::open(repo.path()).unwrap();

    seed_branch(
        &mut store,
        "a1b2c3d",
        "stale_sym",
        "a1b2c3d000000000000000000000000000000000",
    );
    seed_branch(
        &mut store,
        "d4e5f60",
        "cur_sym",
        "d4e5f60000000000000000000000000000000000",
    );

    // Current detached HEAD is d4e5f60; a1b2c3d is stale.
    let report = store
        .prune_branches(&|entry| entry.kind == BranchKind::Detached && entry.name == "d4e5f60")
        .unwrap();
    assert_eq!(report.dropped, vec!["a1b2c3d".to_owned()]);
    assert_eq!(report.kept, 1);

    let tables = store.list_branch_tables().unwrap();
    assert!(!tables.contains(&branch::sanitize("a1b2c3d")));
    assert!(tables.contains(&branch::sanitize("d4e5f60")));
    assert_eq!(
        store
            .lookup_symbol("d4e5f60", "cur_sym", false)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn commit_keys_survive_default_gc() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let mut store = KuzuGraphStore::open(repo.path()).unwrap();
    let repo_id = store.repo_id().to_owned();

    let commit_sha = "0123456789abcdef0123456789abcdef01234567";
    seed_branch(&mut store, "main", "main_fn", commit_sha);
    seed_branch(&mut store, commit_sha, "commit_fn", commit_sha);

    // Even a predicate that returns `false` for everything MUST retain Commit keys.
    let report = store.prune_branches(&|_| false).unwrap();
    assert_eq!(report.dropped, vec!["main".to_owned()]);
    assert_eq!(report.kept, 1);

    let reg = read_branch_registry(&repo_id).unwrap();
    assert_eq!(reg.len(), 1);
    assert_eq!(reg[0].name, commit_sha);
    assert_eq!(reg[0].kind, BranchKind::Commit);
    assert_eq!(
        store
            .lookup_symbol(commit_sha, "commit_fn", false)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn commit_key_is_not_detached() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let mut store = KuzuGraphStore::open(repo.path()).unwrap();
    let repo_id = store.repo_id().to_owned();

    let full_sha = "abcdef0123456789abcdef0123456789abcdef01";
    let short_sha = &full_sha[..7];
    seed_branch(&mut store, full_sha, "commit_sym", full_sha);
    seed_branch(&mut store, short_sha, "detached_sym", full_sha);

    let reg_before = read_branch_registry(&repo_id).unwrap();
    let commit_entry = reg_before.iter().find(|e| e.name == full_sha).unwrap();
    let detached_entry = reg_before.iter().find(|e| e.name == short_sha).unwrap();
    assert_eq!(commit_entry.kind, BranchKind::Commit);
    assert_eq!(detached_entry.kind, BranchKind::Detached);

    // Drop the detached entry (simulating moving away from that detached HEAD).
    let report = store.prune_branches(&|_| false).unwrap();
    assert_eq!(report.dropped, vec![short_sha.to_owned()]);
    assert_eq!(report.kept, 1);

    // Commit key at the exact same SHA is untouched.
    assert_eq!(
        store
            .lookup_symbol(full_sha, "commit_sym", false)
            .unwrap()
            .len(),
        1
    );
    assert!(!store
        .list_branch_tables()
        .unwrap()
        .contains(&branch::sanitize(short_sha)));
}

#[test]
fn active_comparison_keys_survive_gc_between_subprocesses() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let base_sha = "1111111111111111111111111111111111111111";
    let head_sha = "2222222222222222222222222222222222222222";

    // Subprocess 1: index base and head commit keys, then release the store lock.
    {
        let mut store = KuzuGraphStore::open(repo.path()).unwrap();
        seed_branch(&mut store, base_sha, "base_sym", base_sha);
        seed_branch(&mut store, head_sha, "head_sym", head_sha);
    }

    // Intervening explicit GC runs while the lock is free.
    {
        let mut store = KuzuGraphStore::open(repo.path()).unwrap();
        let report = store.prune_branches(&|_| false).unwrap();
        assert!(report.dropped.is_empty());
        assert_eq!(report.kept, 2);
    }

    // Subprocess 2: reopen and verify both comparison keys are still intact.
    {
        let store = KuzuGraphStore::open(repo.path()).unwrap();
        assert_eq!(
            store
                .lookup_symbol(base_sha, "base_sym", false)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .lookup_symbol(head_sha, "head_sym", false)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn gc_idempotent() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let mut store = KuzuGraphStore::open(repo.path()).unwrap();

    seed_branch(
        &mut store,
        "main",
        "main_fn",
        "1111111111111111111111111111111111111111",
    );
    seed_branch(
        &mut store,
        "stale",
        "stale_fn",
        "2222222222222222222222222222222222222222",
    );

    let first = store.prune_branches(&|e| e.name == "main").unwrap();
    assert_eq!(first.dropped, vec!["stale".to_owned()]);
    assert_eq!(first.kept, 1);

    let second = store.prune_branches(&|e| e.name == "main").unwrap();
    assert!(second.dropped.is_empty());
    assert_eq!(second.kept, 1);
}

#[test]
fn gc_skips_when_locked() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();

    // Another owner holds the repository lock.
    let _active_owner = KuzuGraphStore::open(repo.path()).unwrap();

    // Non-blocking GC open returns None rather than failing or contending.
    let maybe = KuzuGraphStore::try_open_if_unlocked(repo.path(), "storage gc").unwrap();
    assert!(maybe.is_none());
}

#[test]
fn hook_gc_runs_at_most_daily() {
    let _guard = StoreRootGuard::new();
    let repo_id = "dailygc000000001";
    let t0 = 1_700_000_000u64;

    // Never run before → due.
    assert!(branch_gc_due(repo_id, t0));

    write_last_branch_gc(repo_id, t0).unwrap();
    // Immediately after and up to 86 399 s later → NOT due.
    assert!(!branch_gc_due(repo_id, t0));
    assert!(!branch_gc_due(repo_id, t0 + BRANCH_GC_INTERVAL_SECS - 1));
    // At 86 400 s → due again.
    assert!(branch_gc_due(repo_id, t0 + BRANCH_GC_INTERVAL_SECS));
}

#[test]
fn unregistered_tables_left_alone() {
    let _guard = StoreRootGuard::new();
    let repo = tempfile::tempdir().unwrap();
    let mut store = KuzuGraphStore::open(repo.path()).unwrap();
    let repo_id = store.repo_id().to_owned();

    seed_branch(
        &mut store,
        "main",
        "main_fn",
        "1111111111111111111111111111111111111111",
    );
    seed_branch(
        &mut store,
        "legacy_unreg",
        "legacy_fn",
        "2222222222222222222222222222222222222222",
    );

    // Simulate pre-registry state: remove `legacy_unreg` from branches.json only.
    let reg: Vec<_> = read_branch_registry(&repo_id)
        .unwrap()
        .into_iter()
        .filter(|e| e.name != "legacy_unreg")
        .collect();
    write_branch_registry(&repo_id, &reg).unwrap();

    // Plan and apply GC: `legacy_unreg` must be reported in `unregistered` and NOT dropped.
    let plan = store.plan_prune_branches(&|e| e.name == "main").unwrap();
    assert!(plan.dropped.is_empty());
    assert_eq!(plan.unregistered, vec!["legacy_unreg".to_owned()]);

    let report = store.prune_branches(&|e| e.name == "main").unwrap();
    assert!(report.dropped.is_empty());
    assert_eq!(report.unregistered, vec!["legacy_unreg".to_owned()]);
    assert!(store
        .list_branch_tables()
        .unwrap()
        .contains(&"legacy_unreg".to_owned()));
    assert_eq!(
        store
            .lookup_symbol("legacy_unreg", "legacy_fn", false)
            .unwrap()
            .len(),
        1
    );
}

#[allow(dead_code)]
fn _assert_path_import(_: &Path) {}
