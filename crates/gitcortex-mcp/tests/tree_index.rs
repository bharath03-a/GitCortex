mod support;
use gitcortex_core::store::GraphStore;
use gitcortex_mcp::mcp::{readiness::inspect_commit, tree_jobs::apply_tree_index};
use gitcortex_store::kuzu::KuzuGraphStore;
use support::{fingerprint, index_hook_path, TestRepo};

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

#[tokio::test]
async fn duplicate_requests_spawn_one_job() {
    use gitcortex_mcp::mcp::tree_jobs::{JobStatus, TreeIndexJobs};
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn target() {}\n");
    let sha = repo.commit("fixture");
    let store = std::sync::Arc::new(std::sync::Mutex::new(
        KuzuGraphStore::open(repo.root()).unwrap(),
    ));
    let jobs = TreeIndexJobs::new(repo.root(), store.clone());
    // Hold the shared store so the job cannot finish while requests race.
    let statuses = {
        let _store = store.lock().unwrap();
        let barrier = std::sync::Barrier::new(5);
        let runtime = tokio::runtime::Handle::current();
        std::thread::scope(|scope| {
            let threads: Vec<_> = (0..5)
                .map(|_| {
                    let jobs = &jobs;
                    let sha = &sha;
                    let barrier = &barrier;
                    let runtime = &runtime;
                    scope.spawn(move || {
                        let _runtime = runtime.enter();
                        barrier.wait();
                        jobs.request("feat/x", sha)
                    })
                })
                .collect();
            threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        statuses
            .iter()
            .filter(|s| matches!(s, JobStatus::Started))
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| matches!(s, JobStatus::AlreadyRunning))
            .count(),
        4
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if inspect_commit(&*store.lock().unwrap(), "feat/x", &sha).state == "ready" {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
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
