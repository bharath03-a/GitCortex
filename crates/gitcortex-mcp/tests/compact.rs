//! End-to-end tests for storage compaction (payload swap under a stable lock),
//! crash recovery, sidecar preservation, closed-snapshot contract, and lock
//! contention across every compaction phase.
mod support;

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use gitcortex_core::store::GraphStore;
use gitcortex_mcp::mcp::{readiness, tree_jobs::apply_tree_index};
use gitcortex_store::{
    branch::{self, RepositoryLock},
    kuzu::{CompactPhase, KuzuGraphStore},
    meta,
    usage::{data_dir_inventory, EntryClass},
};
use support::{fingerprint, TestRepo};

/// Temporarily overrides `GCX_STORE_PATH` while a `TestRepo` (which holds
/// `FIXTURE_LOCK`) is alive, restoring the previous value on drop.
struct EnvStoreGuard {
    prev: Option<std::ffi::OsString>,
}

impl EnvStoreGuard {
    fn set(path: &Path) -> Self {
        let prev = std::env::var_os("GCX_STORE_PATH");
        std::env::set_var("GCX_STORE_PATH", path);
        Self { prev }
    }
}

impl Drop for EnvStoreGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("GCX_STORE_PATH", v),
            None => std::env::remove_var("GCX_STORE_PATH"),
        }
    }
}

fn generate_rust_module(prefix: &str, file_idx: usize, funcs_per_file: usize) -> String {
    let mut src = String::new();
    for f in 0..funcs_per_file {
        let name = format!("{prefix}_f{file_idx}_{f}");
        let prev = if f > 0 {
            format!("{prefix}_f{file_idx}_{}() + ", f - 1)
        } else {
            String::new()
        };
        src.push_str(&format!(
            "/// Doc for `{name}`.\n\
             /// Line 2 with \"quotes\", 'single', and \\backslash.\n\
             pub fn {name}() -> usize {{\n\
                 let payload = \"{pad}\";\n\
                 {prev}payload.len()\n\
             }}\n\n",
            pad = "x".repeat(400),
        ));
    }
    src
}

fn seed_bulk_branch(repo: &TestRepo, prefix: &str, files: usize, funcs_per_file: usize) -> String {
    for i in 0..files {
        repo.write(
            &format!("src/{prefix}/mod_{i:02}.rs"),
            &generate_rust_module(prefix, i, funcs_per_file),
        );
    }
    repo.commit(&format!("seed {prefix}"))
}

fn snapshot_durable_bytes(repo_id: &str) -> BTreeMap<PathBuf, Vec<u8>> {
    let inv = data_dir_inventory(repo_id).expect("inventory");
    assert!(
        !inv.has_unknown(),
        "unexpected unknown entries: {:?}",
        inv.unknown_entries()
    );
    inv.entries
        .into_iter()
        .filter(|(_, class)| *class == EntryClass::Durable)
        .map(|(path, _)| {
            let bytes = fs::read(&path).expect("read durable file");
            (path, bytes)
        })
        .collect()
}

fn serve_lock_ino(repo_id: &str) -> u64 {
    fs::metadata(branch::data_dir(repo_id).join("serve.lock"))
        .expect("serve.lock metadata")
        .ino()
}

fn locate_gcx_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Integration test binaries live in `<target>/<profile>/deps/<bin>`, while
    // `gcx` lives in `<target>/<profile>/gcx`.
    let bin = exe
        .parent()?
        .parent()?
        .join(format!("gcx{}", std::env::consts::EXE_SUFFIX));
    bin.is_file().then_some(bin)
}

#[test]
fn compact_reduces_size_after_branch_drop() {
    let repo = TestRepo::new();
    seed_bulk_branch(&repo, "main", 8, 25);

    repo.git(&["checkout", "-b", "feat_a"]);
    seed_bulk_branch(&repo, "feat_a", 24, 30);

    repo.git(&["checkout", "main"]);
    repo.git(&["checkout", "-b", "feat_b"]);
    seed_bulk_branch(&repo, "feat_b", 24, 30);

    repo.git(&["checkout", "main"]);

    {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
        apply_tree_index(repo.root(), &mut store, "feat_a", "feat_a").expect("index feat_a");
        apply_tree_index(repo.root(), &mut store, "feat_b", "feat_b").expect("index feat_b");

        let gc = store
            .prune_branches(&|entry| entry.name == "main")
            .expect("prune_branches");
        assert_eq!(gc.dropped.len(), 2);
        assert_eq!(gc.kept, 1);
    }

    let report = KuzuGraphStore::compact(repo.root()).expect("compact");
    assert!(
        report.before > 0 && report.after > 0,
        "expected non-zero payload sizes: {report:?}"
    );
    assert!(
        report.after < report.before * 7 / 10,
        "expected compaction to shrink payload below 70%: before={} after={}",
        report.before,
        report.after
    );

    let store = KuzuGraphStore::open(repo.root()).expect("reopen after compact");
    assert!(
        !store.list_all_nodes("main").expect("list main").is_empty(),
        "main branch nodes must survive compaction"
    );
}

#[test]
fn compact_preserves_fingerprints() {
    let repo = TestRepo::new();
    repo.write(
        "src/lib.rs",
        "/// Main docstring with \"quotes\" and \\backslash\n\
         pub fn alpha() -> i32 { beta() + 1 }\n\
         pub fn beta() -> i32 { 41 }\n",
    );
    let main_sha = repo.commit("initial main");

    repo.git(&["checkout", "-b", "feat/x"]);
    repo.write(
        "src/extra.rs",
        "pub struct Widget { pub id: u64 }\n\
         pub fn build_widget() -> Widget { Widget { id: crate::alpha() as u64 } }\n",
    );
    let feat_sha = repo.commit("feature x");
    repo.git(&["checkout", "main"]);

    let (fp_main_before, fp_feat_before, fp_commit_before) = {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
        apply_tree_index(repo.root(), &mut store, "feat/x", "feat/x").expect("index feat/x");
        apply_tree_index(repo.root(), &mut store, &feat_sha, &feat_sha).expect("index commit key");
        assert_eq!(
            store.last_indexed_sha("main").unwrap().as_deref(),
            Some(main_sha.as_str())
        );
        (
            fingerprint(&store, "main"),
            fingerprint(&store, "feat/x"),
            fingerprint(&store, &feat_sha),
        )
    };

    KuzuGraphStore::compact(repo.root()).expect("compact");

    let store = KuzuGraphStore::open(repo.root()).expect("reopen");
    for (label, before) in [
        ("main", &fp_main_before),
        ("feat/x", &fp_feat_before),
        (feat_sha.as_str(), &fp_commit_before),
    ] {
        let after = fingerprint(&store, label);
        assert_eq!(
            before.diff(&after),
            None,
            "fingerprint diverged after compact for {label}"
        );
    }
}

#[test]
fn compact_preserves_sidecars() {
    let repo = TestRepo::new();
    repo.write("src/lib.rs", "pub fn greet() -> &'static str { \"hi\" }\n");
    let main_sha = repo.commit("init");

    repo.git(&["checkout", "-b", "scratch"]);
    repo.write("src/scratch.rs", "pub fn tmp() {}\n");
    let scratch_sha = repo.commit("scratch");
    repo.git(&["checkout", "main"]);

    let repo_id = branch::storage_repo_id(repo.root());
    let (main_readiness_before, commit_readiness_before, registry_before, schema_before) = {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
        apply_tree_index(repo.root(), &mut store, &scratch_sha, &scratch_sha)
            .expect("index commit key");
        apply_tree_index(repo.root(), &mut store, "scratch", "scratch").expect("index scratch");

        // Populate every Durable sidecar class entry so we verify none are touched.
        store
            .set_working_tree_dirty("scratch", true)
            .expect("set dirty");
        readiness::write_overlay_checkpoint(repo.root(), "scratch", &scratch_sha, &BTreeMap::new())
            .expect("write overlay");
        // Write a non-empty overlay file directly as well (empty map removes it).
        let mut overlay_files = BTreeMap::new();
        overlay_files.insert(PathBuf::from("src/scratch.rs"), Some("abc".to_owned()));
        readiness::write_overlay_checkpoint(repo.root(), "scratch", &scratch_sha, &overlay_files)
            .expect("write non-empty overlay");
        branch::mark_editor_prompted(&repo_id).expect("mark editor_prompted");
        meta::mark_branch_gc_now(&repo_id).expect("mark last_branch_gc");
        fs::write(
            branch::data_dir(&repo_id).join("embeddings_main.bin"),
            b"fake-embeddings-v1",
        )
        .expect("write embeddings");

        let main_r = readiness::inspect(repo.root(), &store, "main");
        assert_eq!(main_r.state, "ready");
        let commit_r = readiness::inspect_commit(&store, &scratch_sha, &scratch_sha);
        assert_eq!(commit_r.state, "ready");
        assert_eq!(
            store.last_indexed_sha("main").unwrap().as_deref(),
            Some(main_sha.as_str())
        );

        let reg = meta::read_branch_registry(&repo_id).expect("registry");
        let schema = branch::read_schema_version(&repo_id);
        (main_r, commit_r, reg, schema)
    };

    let durable_before = snapshot_durable_bytes(&repo_id);
    let ino_before = serve_lock_ino(&repo_id);

    KuzuGraphStore::compact(repo.root()).expect("compact");

    // Lock inode never moves.
    assert_eq!(
        serve_lock_ino(&repo_id),
        ino_before,
        "serve.lock inode must remain unchanged across compaction"
    );

    // No Unknown entries left behind, and every Durable file is byte-identical.
    let inv_after = data_dir_inventory(&repo_id).expect("inventory after");
    assert!(
        !inv_after.has_unknown(),
        "unexpected unknown entries after compact: {:?}",
        inv_after.unknown_entries()
    );
    let durable_after = snapshot_durable_bytes(&repo_id);
    assert_eq!(
        durable_after, durable_before,
        "every Durable sidecar must be byte-identical after compaction"
    );

    // Reopen and verify readiness (branch + Commit key), registry, and schema version.
    let store = KuzuGraphStore::open(repo.root()).expect("reopen");
    assert_eq!(
        readiness::inspect(repo.root(), &store, "main"),
        main_readiness_before
    );
    assert_eq!(
        readiness::inspect_commit(&store, &scratch_sha, &scratch_sha),
        commit_readiness_before
    );
    assert_eq!(
        meta::read_branch_registry(&repo_id).expect("registry after"),
        registry_before
    );
    assert_eq!(branch::read_schema_version(&repo_id), schema_before);
}

#[test]
fn closed_snapshot_reopens_ready() {
    let repo = TestRepo::new();
    repo.write(
        "src/lib.rs",
        "pub fn compute(x: i32) -> i32 { helper(x) }\nfn helper(x: i32) -> i32 { x * 2 }\n",
    );
    let head_sha = repo.commit("init");
    let repo_id = branch::storage_repo_id(repo.root());

    let (fp_main, fp_commit, readiness_main, readiness_commit) = {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
        apply_tree_index(repo.root(), &mut store, &head_sha, &head_sha).expect("index commit");
        let rm = readiness::inspect_commit(&store, "main", &head_sha);
        let rc = readiness::inspect_commit(&store, &head_sha, &head_sha);
        assert_eq!(rm.state, "ready");
        assert_eq!(rc.state, "ready");
        (
            fingerprint(&store, "main"),
            fingerprint(&store, &head_sha),
            rm,
            rc,
        )
    };

    // Closed snapshot contract: hold RepositoryLock with no open store, copy
    // strictly Payload + Durable entries to a fresh data root, never Runtime.
    let fresh_root = tempfile::tempdir().expect("fresh store root");
    let fresh_repo_dir = fresh_root.path().join(&repo_id);
    fs::create_dir_all(&fresh_repo_dir).expect("create fresh repo dir");

    {
        let _lock = RepositoryLock::try_acquire(repo.root())
            .expect("try_acquire")
            .expect("lock must be free after store drop");
        let inv = data_dir_inventory(&repo_id).expect("inventory");
        let entries = inv.snapshot_entries().expect("snapshot entries");
        assert!(
            entries
                .iter()
                .all(|p| p.file_name().and_then(|n| n.to_str()) != Some("serve.lock")),
            "closed snapshot must never include serve.lock"
        );
        for src in entries {
            let name = src.file_name().expect("file name");
            fs::copy(src, fresh_repo_dir.join(name)).expect("copy snapshot entry");
        }
    }

    // Reopen from the fresh data root and verify every registered key is Ready
    // with identical fingerprints.
    {
        let _env_guard = EnvStoreGuard::set(fresh_root.path());
        let restored = KuzuGraphStore::open(repo.root()).expect("open restored snapshot");
        assert_eq!(
            readiness::inspect_commit(&restored, "main", &head_sha),
            readiness_main
        );
        assert_eq!(
            readiness::inspect_commit(&restored, &head_sha, &head_sha),
            readiness_commit
        );
        assert_eq!(fingerprint(&restored, "main").diff(&fp_main), None);
        assert_eq!(fingerprint(&restored, &head_sha).diff(&fp_commit), None);
    }
}

#[test]
fn compact_failure_restores_original() {
    let repo = TestRepo::new();
    repo.write(
        "src/lib.rs",
        "pub fn one() -> i32 { two() }\npub fn two() -> i32 { 2 }\n",
    );
    repo.commit("main commit");
    repo.git(&["checkout", "-b", "other"]);
    repo.write("src/other.rs", "pub fn three() -> i32 { 3 }\n");
    repo.commit("other commit");
    repo.git(&["checkout", "main"]);

    let repo_id = branch::storage_repo_id(repo.root());
    let (fp_main, fp_other) = {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
        apply_tree_index(repo.root(), &mut store, "other", "other").expect("index other");
        (fingerprint(&store, "main"), fingerprint(&store, "other"))
    };

    let durable_before = snapshot_durable_bytes(&repo_id);
    let ino_before = serve_lock_ino(&repo_id);

    for phase in [
        CompactPhase::Exported,
        CompactPhase::Imported,
        CompactPhase::OldMovedAside,
        CompactPhase::NewInPlace,
        CompactPhase::Verified,
    ] {
        let err = KuzuGraphStore::compact_with(repo.root(), Some(phase))
            .expect_err("injected failure must return Err");
        assert!(
            err.to_string().contains("injected compaction failure"),
            "unexpected error for {phase:?}: {err}"
        );

        let inv = data_dir_inventory(&repo_id).expect("inventory");
        assert!(
            !inv.has_unknown(),
            "leftover staging/backup artifacts after failure at {phase:?}: {:?}",
            inv.unknown_entries()
        );
        assert_eq!(
            snapshot_durable_bytes(&repo_id),
            durable_before,
            "sidecars modified after failure at {phase:?}"
        );
        assert_eq!(
            serve_lock_ino(&repo_id),
            ino_before,
            "serve.lock inode changed after failure at {phase:?}"
        );

        let store = KuzuGraphStore::open(repo.root())
            .unwrap_or_else(|e| panic!("reopen failed after {phase:?}: {e}"));
        assert_eq!(
            fingerprint(&store, "main").diff(&fp_main),
            None,
            "main fingerprint corrupted after {phase:?}"
        );
        assert_eq!(
            fingerprint(&store, "other").diff(&fp_other),
            None,
            "other fingerprint corrupted after {phase:?}"
        );
    }
}

#[test]
fn crash_between_renames_recovers_on_open() {
    let repo = TestRepo::new();
    repo.write(
        "src/lib.rs",
        "pub fn survivor() -> &'static str { \"recovered\" }\n",
    );
    repo.commit("init");
    let repo_id = branch::storage_repo_id(repo.root());

    let fp_before = {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
        fingerprint(&store, "main")
    };

    // Simulate process death immediately after CompactPhase::OldMovedAside
    // (before NewInPlace and without rollback): `graph.kuzu` is missing while
    // `graph.kuzu.pre-compact-<ts>{,.wal}` and `compact-<ts>/` exist on disk.
    let dir = branch::data_dir(&repo_id);
    let ts = 1_700_000_000u64;
    let [live_db, live_wal] = branch::payload_files_in_dir(&dir);
    let [bak_db, bak_wal] = branch::pre_compact_payload_files_in_dir(&dir, ts);
    fs::rename(&live_db, &bak_db).expect("move live db aside");
    if live_wal.exists() {
        fs::rename(&live_wal, &bak_wal).expect("move live wal aside");
    }
    let staging = branch::staging_dir(&repo_id, ts);
    fs::create_dir_all(&staging).expect("create staging dir");
    fs::write(staging.join("graph.kuzu"), b"uncommitted-staged-db").expect("write staged");

    assert!(!live_db.exists(), "live db must be absent before recovery");

    // Next open must recover the pre-compact payload transparently.
    let store = KuzuGraphStore::open(repo.root()).expect("open must recover from pre-compact");
    assert!(live_db.exists(), "live db must be restored");
    let inv = data_dir_inventory(&repo_id).expect("inventory after recovery");
    assert!(
        !inv.has_unknown(),
        "unexpected leftover compaction files after open recovery: {:?}",
        inv.unknown_entries()
    );
    assert_eq!(fingerprint(&store, "main").diff(&fp_before), None);
}

/// Helper subprocess entry point used as a fallback if the `gcx` CLI binary is
/// not present in the target directory when running `-p gitcortex-mcp` alone.
#[test]
fn open_during_compact_child() {
    let Some(root) = std::env::var_os("GCX_COMPACT_CHILD_ROOT") else {
        return;
    };
    let res = KuzuGraphStore::open(Path::new(&root));
    match res {
        Ok(_) => {
            eprintln!("UNEXPECTED_OPEN_SUCCESS");
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    }
}

#[test]
fn open_refused_during_each_compact_phase() {
    let repo = TestRepo::new();
    repo.write(
        "src/lib.rs",
        "pub fn compact_target_symbol() -> i32 { 42 }\n",
    );
    repo.commit("init");

    {
        let mut store = KuzuGraphStore::open(repo.root()).expect("open");
        apply_tree_index(repo.root(), &mut store, "main", "main").expect("index main");
    }

    let gcx_bin = locate_gcx_binary();
    let store_path = std::env::var_os("GCX_STORE_PATH").expect("GCX_STORE_PATH");
    let mut visited = Vec::new();

    KuzuGraphStore::compact_with_hook(repo.root(), |phase| {
        visited.push(phase);

        let output = if let Some(ref gcx) = gcx_bin {
            Command::new(gcx)
                .args(["query", "lookup-symbol", "compact_target_symbol"])
                .current_dir(repo.root())
                .env("GCX_STORE_PATH", &store_path)
                .env("NO_COLOR", "1")
                .stdin(Stdio::null())
                .output()
                .expect("spawn gcx query lookup-symbol")
        } else {
            Command::new(std::env::current_exe().expect("current_exe"))
                .args(["--exact", "open_during_compact_child", "--nocapture"])
                .env("GCX_COMPACT_CHILD_ROOT", repo.root())
                .env("GCX_STORE_PATH", &store_path)
                .stdin(Stdio::null())
                .output()
                .expect("spawn child opener")
        };

        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.status.success(),
            "child open/query must be refused during {phase:?}; output:\n{combined}"
        );
        assert!(
            combined.contains("repository graph is busy") && combined.contains("(storage compact)"),
            "expected lock-held busy error during {phase:?}, got:\n{combined}"
        );
        Ok(())
    })
    .expect("compaction should complete cleanly");

    assert_eq!(
        visited,
        vec![
            CompactPhase::Exported,
            CompactPhase::Imported,
            CompactPhase::OldMovedAside,
            CompactPhase::NewInPlace,
            CompactPhase::Verified,
        ]
    );

    // After compaction completes, the store opens cleanly and still resolves the symbol.
    let store = KuzuGraphStore::open(repo.root()).expect("reopen after compact");
    let hits = store
        .lookup_symbol("main", "compact_target_symbol", false)
        .expect("lookup symbol");
    assert_eq!(hits.len(), 1);
}
