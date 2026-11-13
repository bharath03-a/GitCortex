mod support;
use gitcortex_core::store::GraphStore;
use gitcortex_mcp::mcp::{readiness::inspect_commit, tree_jobs::apply_tree_index};
use gitcortex_store::kuzu::KuzuGraphStore;
use support::{fingerprint, index_hook_path, StoreCleanup, TestRepo};

const DEADLINE: std::time::Duration = std::time::Duration::from_secs(15);

#[test]
fn tree_index_matches_checkout_index() {
    branch_index_equivalence("feat/x");
}
#[test]
fn tree_index_handles_unicode_branch() {
    branch_index_equivalence("feat/ü-x");
}
fn branch_index_equivalence(branch: &str) {
    let repo = TestRepo::new();
    repo.write("src/lib.rs", "pub fn base() {}\n");
    repo.commit("base");
    repo.git(&["switch", "-c", branch]);
    repo.write(
        "src/lib.rs",
        "pub fn base() {}\npub fn extra() { base(); }\n",
    );
    let sha = repo.commit("feature");
    let copy = repo.snapshot_copy();
    let mut expected = KuzuGraphStore::open(copy.root()).unwrap();
    index_hook_path(&copy, &mut expected, branch);
    repo.git(&["switch", "main"]);
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, branch, &sha).unwrap();
    assert_eq!(fingerprint(&store, branch), fingerprint(&expected, branch));
    let state = inspect_commit(&store, branch, &sha);
    assert_eq!(state.state, "ready");
    assert!(!state.working_tree_verified);
    store.set_working_tree_dirty(branch, true).unwrap();
    assert_eq!(inspect_commit(&store, branch, &sha).state, "unverified");
}

#[test]
fn tree_index_incremental_from_sha() {
    let repo = TestRepo::new();
    repo.write("lib.rs", "pub fn old() {}\n");
    let first = repo.commit("first");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "feature", &first).unwrap();
    repo.remove("lib.rs");
    repo.write("new.rs", "pub fn new() {}\n");
    let second = repo.commit("second");
    apply_tree_index(repo.root(), &mut store, "feature", &second).unwrap();
    support::assert_matches_rebuild(&repo, &store, "feature");
    assert_eq!(inspect_commit(&store, "feature", &first).state, "stale");
    assert_eq!(inspect_commit(&store, "absent", &first).state, "missing");
}

#[test]
fn inspect_commit_ready_in_bare_repo() {
    commit_without_checkout(true);
}
#[test]
fn inspect_commit_ready_with_unborn_checkout() {
    commit_without_checkout(false);
}
fn commit_without_checkout(bare: bool) {
    let source = TestRepo::new();
    source.write("lib.rs", "pub fn fetched() {}\n");
    let sha = source.commit("source");
    let root = tempfile::tempdir().unwrap();
    let _cleanup = StoreCleanup::new(root.path());
    let mut init = std::process::Command::new("git");
    init.arg("init");
    if bare {
        init.arg("--bare");
    }
    assert!(init.arg(root.path()).status().unwrap().success());
    assert!(std::process::Command::new("git")
        .args(["fetch", source.root().to_str().unwrap(), &sha])
        .current_dir(root.path())
        .status()
        .unwrap()
        .success());
    let mut store = KuzuGraphStore::open(root.path()).unwrap();
    apply_tree_index(root.path(), &mut store, &sha, &sha).unwrap();
    let state = inspect_commit(&store, &sha, &sha);
    assert_eq!(state.state, "ready");
    assert!(state.checkout_revision.is_none());
}

#[test]
fn dirty_retry_drops_folders_of_vanished_directories() {
    let repo = TestRepo::new();
    repo.write("keep/a.rs", "pub fn original() {}\n");
    let sha = repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    repo.write("ghost_dir/nested/ghost.rs", "pub fn ghost() {}\n");
    let diff = gitcortex_indexer::IncrementalIndexer::new(repo.root())
        .unwrap()
        .index_files_from_disk(&[repo.root().join("ghost_dir/nested/ghost.rs")])
        .unwrap();
    store.apply_diff("main", &diff).unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    support::assert_matches_rebuild(&repo, &store, "main");
    assert_eq!(inspect_commit(&store, "main", &sha).state, "ready");
}

#[test]
fn tree_index_incremental_drops_folders_of_vanished_directories() {
    let repo = TestRepo::new();
    repo.write("keep/a.rs", "pub fn kept() {}\n");
    repo.write("gone/nested/b.rs", "pub fn removed() {}\n");
    let first = repo.commit("first");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &first).unwrap();
    std::fs::remove_dir_all(repo.root().join("gone")).unwrap();
    let second = repo.commit("second");
    apply_tree_index(repo.root(), &mut store, "main", &second).unwrap();
    support::assert_matches_rebuild(&repo, &store, "main");
}

#[test]
fn dotted_directory_sibling_survives_incremental() {
    let repo = TestRepo::new();
    repo.write("v1.2/a.rs", "pub fn alpha() {}\n");
    repo.write("v1.2/b.rs", "pub fn beta() {}\n");
    let first = repo.commit("first");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &first).unwrap();
    repo.write("v1.2/a.rs", "pub fn alpha2() {}\n");
    let second = repo.commit("second");
    apply_tree_index(repo.root(), &mut store, "main", &second).unwrap();
    support::assert_matches_rebuild(&repo, &store, "main");
}

#[test]
fn dirty_retry_rebuilds_complete_tree() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let sha = repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    repo.write("ghost.rs", "pub fn ghost() {}\n");
    let diff = gitcortex_indexer::IncrementalIndexer::new(repo.root())
        .unwrap()
        .index_files_from_disk(&[repo.root().join("ghost.rs")])
        .unwrap();
    store.apply_diff("main", &diff).unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    assert!(store
        .lookup_symbol("main", "ghost", false)
        .unwrap()
        .is_empty());
    assert_eq!(inspect_commit(&store, "main", &sha).state, "ready");
}

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node},
    schema::NodeKind,
    store::{CallersDeep, SubGraph, SymbolContext},
};
use std::path::Path;
struct FaultStore {
    inner: KuzuGraphStore,
    fail: bool,
    fail_sha_read: bool,
    fail_dirty_read: bool,
    root: std::path::PathBuf,
}
impl GraphStore for FaultStore {
    fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()> {
        assert!(self.inner.working_tree_dirty(branch)?);
        let target = gitcortex_mcp::mcp::tree_jobs::resolve_commit(&self.root, "HEAD").unwrap();
        assert_ne!(
            self.inner.last_indexed_sha(branch)?.as_deref(),
            Some(target.as_str())
        );
        assert!(
            !gitcortex_mcp::mcp::readiness::overlay_checkpoint_path(&self.root, branch).exists(),
            "an overlay beside a partial apply could certify the job's bytes"
        );
        if self.fail {
            return Err(GitCortexError::Store("injected apply failure".into()));
        }
        self.inner.apply_diff(branch, diff)
    }
    fn lookup_symbol(&self, branch: &str, name: &str, fuzzy: bool) -> Result<Vec<Node>> {
        self.inner.lookup_symbol(branch, name, fuzzy)
    }
    fn find_callers(&self, branch: &str, function_name: &str) -> Result<Vec<Node>> {
        self.inner.find_callers(branch, function_name)
    }
    fn find_callers_deep(
        &self,
        branch: &str,
        function_name: &str,
        depth: u8,
    ) -> Result<CallersDeep> {
        self.inner.find_callers_deep(branch, function_name, depth)
    }
    fn symbol_context(&self, branch: &str, name: &str) -> Result<SymbolContext> {
        self.inner.symbol_context(branch, name)
    }
    fn list_definitions(&self, branch: &str, file: &Path) -> Result<Vec<Node>> {
        self.inner.list_definitions(branch, file)
    }
    fn list_all_nodes(&self, branch: &str) -> Result<Vec<Node>> {
        self.inner.list_all_nodes(branch)
    }
    fn list_all_edges(&self, branch: &str) -> Result<Vec<Edge>> {
        self.inner.list_all_edges(branch)
    }
    fn branch_diff(&self, from: &str, to: &str) -> Result<GraphDiff> {
        self.inner.branch_diff(from, to)
    }
    fn find_callees(&self, branch: &str, function_name: &str, depth: u8) -> Result<CallersDeep> {
        self.inner.find_callees(branch, function_name, depth)
    }
    fn find_implementors(&self, branch: &str, trait_or_interface_name: &str) -> Result<Vec<Node>> {
        self.inner
            .find_implementors(branch, trait_or_interface_name)
    }
    fn trace_path(&self, branch: &str, from: &str, to: &str) -> Result<Vec<Node>> {
        self.inner.trace_path(branch, from, to)
    }
    fn list_symbols_in_range(
        &self,
        branch: &str,
        file: &Path,
        start_line: u32,
        end_line: u32,
    ) -> Result<Vec<Node>> {
        self.inner
            .list_symbols_in_range(branch, file, start_line, end_line)
    }
    fn find_unused_symbols(&self, branch: &str, kind: Option<NodeKind>) -> Result<Vec<Node>> {
        self.inner.find_unused_symbols(branch, kind)
    }
    fn get_subgraph(
        &self,
        branch: &str,
        seed_name: &str,
        depth: u8,
        direction: &str,
    ) -> Result<SubGraph> {
        self.inner.get_subgraph(branch, seed_name, depth, direction)
    }
    fn last_indexed_sha(&self, branch: &str) -> Result<Option<String>> {
        if self.fail_sha_read {
            return Err(GitCortexError::Store("injected SHA read failure".into()));
        }
        self.inner.last_indexed_sha(branch)
    }
    fn set_last_indexed_sha(&mut self, branch: &str, sha: &str) -> Result<()> {
        self.inner.set_last_indexed_sha(branch, sha)
    }
    fn working_tree_dirty(&self, _branch: &str) -> Result<bool> {
        if self.fail_dirty_read {
            return Err(GitCortexError::Store("injected dirty read failure".into()));
        }
        self.inner.working_tree_dirty(_branch)
    }
    fn set_working_tree_dirty(&mut self, _branch: &str, _dirty: bool) -> Result<()> {
        self.inner.set_working_tree_dirty(_branch, _dirty)
    }
}
fn fault_store(repo: &TestRepo, fail: bool) -> FaultStore {
    FaultStore {
        inner: KuzuGraphStore::open(repo.root()).unwrap(),
        fail,
        fail_sha_read: false,
        fail_dirty_read: false,
        root: repo.root().to_owned(),
    }
}

#[test]
fn failed_first_apply_is_unverified_without_sha() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let first = repo.commit("first");
    let mut store = fault_store(&repo, true);
    assert!(apply_tree_index(repo.root(), &mut store, "main", &first).is_err());
    assert_eq!(store.last_indexed_sha("main").unwrap(), None);
    assert!(store.working_tree_dirty("main").unwrap());
    assert_eq!(inspect_commit(&store, "main", &first).state, "unverified");
}

#[test]
fn failed_apply_leaves_no_overlay_to_recertify() {
    use gitcortex_mcp::mcp::readiness::{overlay_checkpoint_path, write_overlay_checkpoint};
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let first = repo.commit("first");
    let mut store = fault_store(&repo, false);
    apply_tree_index(repo.root(), &mut store, "feature", &first).unwrap();
    repo.write("a.rs", "pub fn updated() {}\n");
    let second = repo.commit("second");
    let files = std::collections::BTreeMap::from([(std::path::PathBuf::from("a.rs"), None)]);
    assert!(write_overlay_checkpoint(repo.root(), "feature", &first, &files).unwrap());
    store.fail = true;
    assert!(apply_tree_index(repo.root(), &mut store, "feature", &second).is_err());
    assert!(!overlay_checkpoint_path(repo.root(), "feature").exists());
    assert!(store.working_tree_dirty("feature").unwrap());
    assert_eq!(
        inspect_commit(&store, "feature", &second).state,
        "unverified"
    );
}

#[test]
fn tree_apply_publishes_sha_only_after_success() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let sha = repo.commit("first");
    // FaultStore::apply_diff asserts the key is dirty and uncertified mid-apply.
    let mut store = fault_store(&repo, false);
    let outcome = apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    assert_eq!(outcome.sha, sha);
    assert!(outcome.nodes > 0);
    assert_eq!(store.last_indexed_sha("main").unwrap(), Some(sha.clone()));
    assert!(!store.working_tree_dirty("main").unwrap());
    assert_eq!(inspect_commit(&store, "main", &sha).state, "ready");
}

#[test]
fn inspect_commit_wrong_sha_is_stale() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let first = repo.commit("first");
    repo.write("a.rs", "pub fn updated() {}\n");
    let second = repo.commit("second");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &first).unwrap();
    let state = inspect_commit(&store, "main", &second);
    assert_eq!(state.state, "stale");
    assert_eq!(state.indexed_revision.as_deref(), Some(first.as_str()));
    assert_eq!(state.target_revision.as_deref(), Some(second.as_str()));
}

#[test]
fn inspect_commit_dirty_marker_is_unverified() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let sha = repo.commit("first");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    // Dirty wins over missing and mismatched SHAs.
    assert_eq!(inspect_commit(&store, "main", &sha).state, "unverified");
    store.set_last_indexed_sha("main", "0000").unwrap();
    assert_eq!(inspect_commit(&store, "main", &sha).state, "unverified");
}

#[test]
fn inspect_commit_missing_key_is_missing() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let sha = repo.commit("first");
    let store = KuzuGraphStore::open(repo.root()).unwrap();
    let state = inspect_commit(&store, "never-indexed", &sha);
    assert_eq!(state.state, "missing");
    assert_eq!(state.checkout_branch, None);
    assert!(!state.working_tree_verified);
}

#[test]
fn failed_tree_apply_stays_unverified() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    let first = repo.commit("first");
    let mut store = fault_store(&repo, false);
    apply_tree_index(repo.root(), &mut store, "main", &first).unwrap();
    repo.write("a.rs", "pub fn updated() {}\n");
    let second = repo.commit("second");
    store.fail = true;
    assert!(apply_tree_index(repo.root(), &mut store, "main", &second).is_err());
    assert_eq!(
        store.last_indexed_sha("main").unwrap().as_deref(),
        Some(first.as_str())
    );
    assert_eq!(inspect_commit(&store, "main", &second).state, "unverified");
    store.fail = false;
    apply_tree_index(repo.root(), &mut store, "main", &second).unwrap();
    assert_eq!(inspect_commit(&store, "main", &second).state, "ready");
}

// The store is held, so the job sits at `begin` across the switch and commit.
#[tokio::test(flavor = "multi_thread")]
async fn queued_job_cannot_overwrite_a_key_that_became_checked_out() {
    use gitcortex_mcp::mcp::{
        readiness::inspect,
        tree_jobs::{JobStatus, TreeIndexJobs},
    };
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn base() {}\n");
    repo.commit("s0");
    repo.git(&["switch", "-c", "feat/x"]);
    repo.write("a.rs", "pub fn job_bytes() {}\n");
    let s1 = repo.commit("s1");
    repo.git(&["switch", "main"]);
    let store = std::sync::Arc::new(std::sync::Mutex::new(
        KuzuGraphStore::open(repo.root()).unwrap(),
    ));
    let jobs = TreeIndexJobs::new(repo.root(), store.clone());
    {
        let mut held = store.lock().unwrap();
        assert!(matches!(jobs.request("feat/x", &s1), JobStatus::Started));
        repo.git(&["switch", "feat/x"]);
        repo.write("a.rs", "pub fn user_bytes() {}\n");
        let s2 = repo.commit("s2");
        apply_tree_index(repo.root(), &mut *held, "feat/x", &s2).unwrap();
    }
    let deadline = tokio::time::Instant::now() + DEADLINE;
    while jobs.is_active() {
        assert!(tokio::time::Instant::now() < deadline, "job still active");
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let store = store.lock().unwrap();
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert_eq!(store.last_indexed_sha("feat/x").unwrap(), Some(head));
    assert!(!store.working_tree_dirty("feat/x").unwrap());
    assert!(!store
        .lookup_symbol("feat/x", "user_bytes", false)
        .unwrap()
        .is_empty());
    assert!(store
        .lookup_symbol("feat/x", "job_bytes", false)
        .unwrap()
        .is_empty());
    let state = inspect(repo.root(), &*store, "feat/x");
    assert_eq!(state.state, "ready");
    assert!(state.working_tree_verified);
}

#[test]
fn inspect_checkout_semantics_unchanged() {
    use gitcortex_mcp::mcp::readiness::inspect;
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn original() {}\n");
    repo.commit("fixture");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    assert_eq!(inspect(repo.root(), &store, "main").state, "missing");
    index_hook_path(&repo, &mut store, "main");
    assert_eq!(inspect(repo.root(), &store, "main").state, "ready");
    repo.write("a.rs", "pub fn updated() {}\n");
    assert_eq!(inspect(repo.root(), &store, "main").state, "unverified");
    repo.commit("updated");
    assert_eq!(inspect(repo.root(), &store, "main").state, "stale");
}

#[test]
fn tree_index_tracks_supported_files_in_build_directories() {
    let repo = TestRepo::new();
    repo.write("target/generated.rs", "pub fn generated() {}\n");
    repo.write("node_modules/package/lib.rs", "pub fn vendored() {}\n");
    let sha = repo.commit("tracked sources");
    let copy = repo.snapshot_copy();
    let mut expected = KuzuGraphStore::open(copy.root()).unwrap();
    index_hook_path(&copy, &mut expected, "main");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    assert_eq!(fingerprint(&store, "main"), fingerprint(&expected, "main"));
}

#[test]
fn tree_index_incremental_ignore_change_matches_rebuild() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn alpha() {}\n");
    repo.write("b.rs", "pub fn beta() {}\n");
    repo.write("removed.rs", "pub fn removed() {}\n");
    repo.write(".gitcortex/ignore", "a.rs\n");
    let first = repo.commit("first");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &first).unwrap();
    repo.write(".gitcortex/ignore", "b.rs\n");
    repo.remove("removed.rs");
    let second = repo.commit("ignore changed");
    apply_tree_index(repo.root(), &mut store, "main", &second).unwrap();
    support::assert_matches_rebuild(&repo, &store, "main");
}

/// Serializes a tree by hand: `TreeBuilder` refuses legacy modes and odd names.
fn raw_tree(repo: &git2::Repository, entries: &[(&[u8], u32, &[u8])]) -> git2::Oid {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|(name, _, _)| name.to_vec());
    let mut bytes = Vec::new();
    for (name, mode, content) in sorted {
        let id = if mode == 0o160000 {
            git2::Oid::from_bytes(&[7; 20]).unwrap()
        } else {
            repo.blob(content).unwrap()
        };
        bytes.extend_from_slice(format!("{mode:o} ").as_bytes());
        bytes.extend_from_slice(name);
        bytes.push(0);
        bytes.extend_from_slice(id.as_bytes());
    }
    repo.odb()
        .unwrap()
        .write(git2::ObjectType::Tree, &bytes)
        .unwrap()
}

fn raw_commit(
    repo: &git2::Repository,
    parent: Option<&str>,
    entries: &[(&[u8], u32, &[u8])],
) -> String {
    let tree = repo.find_tree(raw_tree(repo, entries)).unwrap();
    let signature = git2::Signature::now("Test", "test@example.test").unwrap();
    let parents: Vec<git2::Commit<'_>> = parent
        .map(|sha| repo.revparse_single(sha).unwrap().peel_to_commit().unwrap())
        .into_iter()
        .collect();
    let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
    repo.commit(None, &signature, &signature, "fixture", &tree, &parents)
        .unwrap()
        .to_string()
}

// Entries a checkout-based test cannot create on every filesystem, through the
// store, so non-UTF-8 paths and removals are covered past the indexer.
#[test]
fn unusual_tree_entries_survive_the_full_apply_path() {
    let repo = TestRepo::new();
    let git = git2::Repository::open(repo.root()).unwrap();
    let first = raw_commit(
        &git,
        None,
        &[
            (b"legacy.rs", 0o100664, b"pub fn legacy_one() {}\n"),
            (b"private.rs", 0o100600, b"pub fn private_mode() {}\n"),
            (b"caf\xff.rs", 0o100644, b"pub fn non_utf8() {}\n"),
            (b"link.rs", 0o120000, b"legacy.rs"),
            (b"module.rs", 0o160000, b""),
            (b"typechange.rs", 0o100644, b"pub fn was_regular() {}\n"),
        ],
    );
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    let has = |store: &KuzuGraphStore, name: &str| {
        !store.lookup_symbol("feat", name, false).unwrap().is_empty()
    };
    apply_tree_index(repo.root(), &mut store, "feat", &first).unwrap();
    assert_eq!(inspect_commit(&store, "feat", &first).state, "ready");
    for name in ["legacy_one", "private_mode", "non_utf8", "was_regular"] {
        assert!(has(&store, name), "{name}");
    }
    let linked = store
        .list_all_nodes("feat")
        .unwrap()
        .into_iter()
        .filter(|n| matches!(n.file.to_str(), Some("link.rs" | "module.rs")))
        .count();
    assert_eq!(linked, 0);

    let second = raw_commit(
        &git,
        Some(&first),
        &[
            (b"legacy.rs", 0o100664, b"pub fn legacy_two() {}\n"),
            (b"typechange.rs", 0o120000, b"legacy.rs"),
        ],
    );
    apply_tree_index(repo.root(), &mut store, "feat", &second).unwrap();
    assert_eq!(inspect_commit(&store, "feat", &second).state, "ready");
    assert!(has(&store, "legacy_two"));
    for name in ["legacy_one", "private_mode", "non_utf8", "was_regular"] {
        assert!(!has(&store, name), "{name} must be gone");
    }
}

#[test]
fn inspect_commit_read_errors_take_precedence() {
    let repo = TestRepo::new();
    repo.write("lib.rs", "pub fn original() {}\n");
    let sha = repo.commit("fixture");
    let mut store = FaultStore {
        inner: KuzuGraphStore::open(repo.root()).unwrap(),
        fail: false,
        fail_sha_read: true,
        fail_dirty_read: false,
        root: repo.root().to_owned(),
    };
    store.set_working_tree_dirty("main", true).unwrap();
    assert_eq!(inspect_commit(&store, "main", &sha).state, "unavailable");
    store.fail_sha_read = false;
    store.fail_dirty_read = true;
    assert_eq!(inspect_commit(&store, "main", &sha).state, "unavailable");
    store.fail_dirty_read = false;
    assert_eq!(inspect_commit(&store, "main", &sha).state, "unverified");
}

#[test]
fn invalid_revision_does_not_mutate_readiness() {
    let repo = TestRepo::new();
    repo.write("lib.rs", "pub fn original() {}\n");
    let sha = repo.commit("fixture");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    apply_tree_index(repo.root(), &mut store, "main", &sha).unwrap();
    assert!(apply_tree_index(repo.root(), &mut store, "main", "invalid-ref").is_err());
    assert_eq!(inspect_commit(&store, "main", &sha).state, "ready");
}
