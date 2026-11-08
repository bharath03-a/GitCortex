mod support;
use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_mcp::mcp::server::serve_daemon;
use gitcortex_store::{
    branch::{last_sha_path, repo_id, storage_repo_id},
    kuzu::KuzuGraphStore,
};
use rmcp::{model::CallToolRequestParams, service::RunningService, RoleClient, ServiceExt};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
use support::*;
use tokio::io::AsyncWriteExt;

const DEADLINE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(25);
const ORIGINAL: &str = "pub fn original() {}\npub fn caller() { original(); }";

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// Re-invoked as a child process by `spawn_daemon`; a no-op in normal runs.
#[tokio::test(flavor = "multi_thread")]
async fn daemon_child() {
    if let Some(root) = std::env::var_os("GCX_TRUST_CHILD_ROOT") {
        serve_daemon(root.into()).await.unwrap();
    }
}

// Field order is drop order: client and daemon go before the repo, whose Drop
// removes the store the daemon was using.
struct Live {
    client: RunningService<RoleClient, ()>,
    compact: bool,
    _daemon: Daemon,
    _runtime: tempfile::TempDir,
    repo: TestRepo,
}

fn state(data: &Value) -> &str {
    data["index"]["state"].as_str().unwrap_or("")
}
// `index_changed` echoes the pre-query index and carries no evidence.
fn is_ready(data: &Value) -> bool {
    state(data) == "ready" && data["status"] != "index_changed"
}
fn items(data: &Value) -> &[Value] {
    data["items"].as_array().map_or(&[], Vec::as_slice)
}
fn assert_gated(data: &Value) {
    assert!(
        is_ready(data) || items(data).is_empty(),
        "items served while not ready: {data}"
    );
}

fn seed_repo(case: &str) -> TestRepo {
    let repo = TestRepo::new();
    repo.write("a.rs", ORIGINAL);
    repo.commit("initial");
    if case == "detached" {
        repo.write("b.rs", "pub fn main_only() {}");
        repo.commit("main only");
    }
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    drop(store);
    match case {
        "restart" => persist_stale_overlay(&repo),
        "omitted" | "tree" | "tree_gate" | "unknown" | "tree_watcher" => {
            repo.git(&["checkout", "-b", "feat/x"]);
            repo.write("a.rs", "pub fn feature_only() {}");
            repo.commit("feature");
            if case != "omitted" {
                repo.git(&["checkout", "main"]);
            }
        }
        _ => {}
    }
    repo
}

// Leaves the store holding an overlay (`overlay`, no `original`) flagged dirty
// while the disk has been restored to the committed bytes.
fn persist_stale_overlay(repo: &TestRepo) {
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    repo.write("a.rs", "pub fn overlay() {}");
    let diff = IncrementalIndexer::new(repo.root())
        .unwrap()
        .index_files_from_disk(&[repo.root().join("a.rs")])
        .unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    store.apply_diff("main", &diff).unwrap();
    repo.write("a.rs", ORIGINAL);
}

fn spawn_daemon(repo: &TestRepo) -> (Daemon, tempfile::TempDir, PathBuf) {
    // A short alias keeps Unix socket names below macOS sockaddr limits.
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let data = runtime.path().join("store");
    std::os::unix::fs::symlink(std::env::var_os("GCX_STORE_PATH").unwrap(), &data).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "daemon_child", "--nocapture"])
        .env("GCX_TRUST_CHILD_ROOT", repo.root())
        .env("GCX_STORE_PATH", &data)
        .env("GCX_CACHE_PATH", runtime.path().join("cache"))
        .env("GCX_DISABLE_SEMANTIC", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    // Coupled to `mcp::server::daemon_socket_path` (data_dir/<repo id>/mcp.sock),
    // rebuilt here under the child's short data root; update both together.
    assert_eq!(storage_repo_id(repo.root()), repo_id(repo.root()));
    let socket = data.join(repo_id(repo.root())).join("mcp.sock");
    (Daemon(child), runtime, socket)
}

async fn connect(socket: &Path, compact: bool) -> RunningService<RoleClient, ()> {
    let deadline = tokio::time::Instant::now() + DEADLINE;
    let mut stream = loop {
        if let Ok(stream) = tokio::net::UnixStream::connect(socket).await {
            break stream;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "daemon startup at {}",
            socket.display()
        );
        tokio::time::sleep(POLL).await;
    };
    stream
        .write_all(&[if compact { 0 } else { 1 }])
        .await
        .unwrap();
    ().serve(stream).await.unwrap()
}

impl Live {
    async fn start(case: &str, compact: bool) -> Self {
        let repo = seed_repo(case);
        let (daemon, runtime, socket) = spawn_daemon(&repo);
        let client = connect(&socket, compact).await;
        Self {
            client,
            compact,
            _daemon: daemon,
            _runtime: runtime,
            repo,
        }
    }
    fn request(&self, action: &str, params: Value) -> CallToolRequestParams {
        let (name, arguments) = if self.compact {
            ("gcx", json!({"action": action, "params": params}))
        } else {
            (action, params)
        };
        CallToolRequestParams::new(name.to_owned())
            .with_arguments(arguments.as_object().unwrap().clone())
    }
    async fn call(&self, action: &str, params: Value) -> Value {
        self.client
            .call_tool(self.request(action, params))
            .await
            .unwrap()
            .structured_content
            .unwrap()
    }
    /// Asks again while the graph changes mid-query, as the response instructs.
    async fn settled(&self, action: &str, params: Value) -> Value {
        let deadline = tokio::time::Instant::now() + DEADLINE;
        loop {
            let data = self.call(action, params.clone()).await;
            if data["status"] != "index_changed" {
                return data;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "index kept changing: {data}"
            );
            tokio::time::sleep(POLL).await;
        }
    }
    async fn lookup(&self, name: &str) -> Value {
        self.call("lookup_symbol", json!({"name": name})).await
    }
    async fn wait_for(&self, name: &str, what: &str, accept: impl Fn(&Value) -> bool) -> Value {
        let deadline = tokio::time::Instant::now() + DEADLINE;
        loop {
            let data = self.lookup(name).await;
            if accept(&data) {
                return data;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{what} deadline: {data}"
            );
            tokio::time::sleep(POLL).await;
        }
    }
    async fn wait_ready(&self) -> Value {
        self.wait_for("original", "startup readiness", is_ready)
            .await
    }
}

async fn tree_branch_scenarios(live: &Live, case: &str) {
    live.wait_ready().await;
    let branch = if case == "unknown" {
        "typo/no-such-branch"
    } else {
        "feat/x"
    };
    if case == "tree_watcher" {
        live.repo.write("a.rs", "pub fn watcher_edit() {}\n");
    }
    let first = live.call("graph_stats", json!({"branch": branch})).await;
    assert_eq!(
        state(&first),
        if case == "unknown" {
            "missing"
        } else {
            "indexing"
        },
        "{first}"
    );
    let deadline = tokio::time::Instant::now() + DEADLINE;
    loop {
        let data = live.call("graph_stats", json!({"branch": branch})).await;
        if case == "unknown" {
            assert_eq!(state(&data), "missing", "{data}");
            assert!(
                data["index"]["next_action"]
                    .as_str()
                    .unwrap_or("")
                    .contains(branch),
                "{data}"
            );
            // A job would have written the dirty marker before anything else.
            tokio::time::sleep(POLL * 10).await;
            let again = live.call("graph_stats", json!({"branch": branch})).await;
            assert_eq!(state(&again), "missing", "{again}");
            let sha = last_sha_path(&storage_repo_id(live.repo.root()), branch);
            assert!(!sha.exists(), "{}", sha.display());
            assert!(!sha.with_extension("dirty").exists(), "{}", sha.display());
            break;
        }
        if is_ready(&data) {
            let symbols = live
                .settled(
                    "lookup_symbol",
                    json!({"name": "feature_only", "branch": branch}),
                )
                .await;
            assert!(!items(&symbols).is_empty(), "{symbols}");
            assert_eq!(symbols["index"]["working_tree_verified"], false);
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "tree index: {data}");
        tokio::time::sleep(POLL).await;
    }
    if case == "tree_watcher" {
        let deadline = tokio::time::Instant::now() + DEADLINE;
        loop {
            let data = live
                .call(
                    "lookup_symbol",
                    json!({"name": "watcher_edit", "branch": "main"}),
                )
                .await;
            if !items(&data).is_empty() && state(&data) != "degraded" {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "watcher: {data}");
            tokio::time::sleep(POLL).await;
        }
        let feature = live
            .settled(
                "lookup_symbol",
                json!({"name": "feature_only", "branch": "feat/x"}),
            )
            .await;
        assert!(is_ready(&feature), "{feature}");
        assert!(!items(&feature).is_empty(), "{feature}");
        let main = live
            .settled(
                "lookup_symbol",
                json!({"name": "feature_only", "branch": "main"}),
            )
            .await;
        assert!(items(&main).is_empty(), "{main}");
    }
}

// `graph_stats` bypasses the gate, so scenarios polling it cannot observe it.
// The first request for a branch reports `indexing` however fast the job runs.
async fn indexing_gate_withholds_items(live: &Live) {
    live.wait_ready().await;
    let params = json!({"name": "feature_only", "branch": "feat/x"});
    let first = live.call("lookup_symbol", params.clone()).await;
    assert_eq!(first["status"], "index_not_ready", "{first}");
    assert_eq!(state(&first), "indexing", "{first}");
    assert!(first.get("items").is_none(), "{first}");
    let deadline = tokio::time::Instant::now() + DEADLINE;
    loop {
        let data = live.call("lookup_symbol", params.clone()).await;
        if is_ready(&data) {
            assert!(!items(&data).is_empty(), "{data}");
            break;
        }
        assert!(
            data.get("items").is_none(),
            "served while not ready: {data}"
        );
        assert!(tokio::time::Instant::now() < deadline, "gate: {data}");
        tokio::time::sleep(POLL).await;
    }
}

async fn clean_checkout_is_ready(live: &Live) {
    let data = live.wait_ready().await;
    assert_eq!(data["index"]["working_tree_verified"], true, "{data}");
    assert!(!items(&data).is_empty(), "{data}");
}

async fn restart_reconciles_stale_overlay(live: &Live) {
    let first = live.lookup("overlay").await;
    if is_ready(&first) {
        assert_eq!(first["index"]["working_tree_verified"], true, "{first}");
    }
    let reconciled = live
        .wait_for("overlay", "overlay reconciliation", |data| {
            assert!(
                items(data).is_empty() || !is_ready(data),
                "stale overlay certified as ready: {data}"
            );
            is_ready(data) && items(data).is_empty()
        })
        .await;
    assert_eq!(
        reconciled["index"]["working_tree_verified"], true,
        "{reconciled}"
    );
    let restored = live
        .settled("lookup_symbol", json!({"name": "original"}))
        .await;
    assert!(is_ready(&restored), "{restored}");
    assert!(!items(&restored).is_empty(), "{restored}");
}

async fn edit_becomes_visible(live: &Live) {
    live.wait_ready().await;
    live.repo.write("a.rs", "pub fn live_added() {}");
    live.wait_for("live_added", "debounce", |data| {
        !items(data).is_empty()
            && state(data) != "degraded"
            && data["index"]["working_tree_verified"] == true
    })
    .await;
}

async fn delete_removes_symbols(live: &Live) {
    let before = live.wait_ready().await;
    assert!(!items(&before).is_empty(), "precondition: {before}");
    live.repo.remove("a.rs");
    live.wait_for("original", "debounce", |data| {
        items(data).is_empty() && state(data) != "degraded"
    })
    .await;
}

async fn branch_scoped_never_leaks_main(live: &Live, case: &str) {
    if case == "switch" {
        live.wait_ready().await;
        live.repo.git(&["checkout", "-b", "feat/x"]);
        live.repo.write("a.rs", "pub fn feature_only() {}");
        live.repo.commit("feature");
    }
    if case == "detached" {
        live.wait_ready().await;
        live.repo.git(&["checkout", "--detach", "HEAD~1"]);
    }
    let expected = if case == "detached" {
        live.repo.git(&["rev-parse", "--short", "HEAD"])
    } else {
        "feat/x".to_owned()
    };
    let leaked = if case == "detached" {
        "main_only"
    } else {
        "original"
    };
    let first = live.lookup(leaked).await;
    assert_eq!(first["index"]["branch"], expected, "{first}");
    assert_gated(&first);
    let ready = live
        .wait_for(leaked, "branch readiness", |data| {
            assert_eq!(data["index"]["branch"], expected, "{data}");
            assert_gated(data);
            is_ready(data)
        })
        .await;
    assert!(items(&ready).is_empty(), "main data leaked: {ready}");
}

async fn schema_matches_output(live: &Live) {
    live.wait_ready().await;
    let tools = live.client.list_all_tools().await.unwrap();
    for (action, params, key) in [
        ("list_definitions", json!({"file": "a.rs"}), "items"),
        (
            "find_callers",
            json!({"function_name": "original"}),
            "evidence",
        ),
    ] {
        let result = live
            .client
            .call_tool(live.request(action, params))
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true), "{result:?}");
        let data = result.structured_content.unwrap();
        let name = if live.compact { "gcx" } else { action };
        let tool = tools.iter().find(|t| t.name == name).unwrap();
        // MCP requires object structuredContent even when no outputSchema is
        // advertised. Validate the advertised schema whenever present.
        let schema = tool
            .output_schema
            .as_ref()
            .map(|s| Value::Object((**s).clone()))
            .unwrap_or_else(|| json!({"type": "object"}));
        assert!(
            jsonschema::is_valid(&schema, &data),
            "schema {schema}: {data}"
        );
        let found = data[key]
            .as_array()
            .unwrap_or_else(|| panic!("{action}: {data}"));
        assert!(!found.is_empty(), "{action}: {data}");
        assert_eq!(data["index"]["branch"], "main");
    }
}

async fn run(case: &str, compact: bool) {
    let live = Live::start(case, compact).await;
    match case {
        "clean" => clean_checkout_is_ready(&live).await,
        "edit" => edit_becomes_visible(&live).await,
        "delete" => delete_removes_symbols(&live).await,
        "restart" => restart_reconciles_stale_overlay(&live).await,
        "schema" => schema_matches_output(&live).await,
        "tree_gate" => indexing_gate_withholds_items(&live).await,
        "tree" | "unknown" | "tree_watcher" => tree_branch_scenarios(&live, case).await,
        _ => branch_scoped_never_leaks_main(&live, case).await,
    }
    live.client.cancel().await.unwrap();
}

macro_rules! live {
    ($name:ident, $case:literal) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            for compact in [false, true] {
                run($case, compact).await;
            }
        }
    };
}
live!(uncommitted_edit_visible_after_debounce, "edit");
live!(clean_checkout_reports_ready_and_verified, "clean");
live!(deleted_file_symbols_disappear, "delete");
live!(branch_omitted_answers_current_checkout, "omitted");
live!(branch_switch_while_running_updates_index_branch, "switch");
live!(
    restart_with_dirty_overlay_not_ready_until_verified,
    "restart"
);
live!(detached_head_never_answers_main, "detached");
live!(array_results_validate_against_output_schema, "schema");

live!(requested_unindexed_branch_starts_tree_index, "tree");
live!(indexing_branch_gates_queries, "tree_gate");
live!(unknown_branch_reports_missing_without_indexing, "unknown");
live!(tree_index_concurrent_with_watcher, "tree_watcher");

#[tokio::test(flavor = "multi_thread")]
async fn edit_then_settle_reports_ready_and_verified() {
    let live = Live::start("clean", true).await;
    live.wait_ready().await;

    // Ignored (.gitcortex/ignore) and unsupported (.txt) files must not block
    // certification or be claimed as indexed.
    live.repo.write(".gitcortex/ignore", "ignored.rs\n");
    live.repo.write("ignored.rs", "pub fn ignored_sym() {}\n");
    live.repo.write("notes.txt", "unsupported file\n");
    live.repo.write("a.rs", "pub fn settled_edit() {}\n");

    let settled = live
        .wait_for("settled_edit", "edit then settle", |data| {
            !items(data).is_empty()
                && is_ready(data)
                && data["index"]["working_tree_verified"] == true
        })
        .await;
    assert_eq!(settled["index"]["state"], "ready", "{settled}");
    assert_eq!(settled["index"]["working_tree_verified"], true, "{settled}");

    let ignored = live
        .settled("lookup_symbol", json!({"name": "ignored_sym"}))
        .await;
    assert!(items(&ignored).is_empty(), "{ignored}");
    assert!(is_ready(&ignored), "{ignored}");

    let dirty_path = gitcortex_store::branch::last_sha_path(
        &storage_repo_id(live.repo.root()),
        "main",
    )
    .with_extension("dirty");
    assert!(
        dirty_path.exists(),
        ".dirty must remain set while uncommitted edits exist"
    );
    live.client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn further_edit_after_checkpoint_returns_unverified_until_settled() {
    let live = Live::start("clean", true).await;
    live.wait_ready().await;

    live.repo.write("a.rs", "pub fn first_edit() {}\n");
    live.wait_for("first_edit", "first edit settle", |data| {
        !items(data).is_empty()
            && is_ready(data)
            && data["index"]["working_tree_verified"] == true
    })
    .await;

    // Further edit invalidates the checkpoint immediately on the next inspect
    // before the 500ms debounce window elapses.
    live.repo.write("a.rs", "pub fn second_edit() {}\n");
    let immediate = live.lookup("first_edit").await;
    assert_eq!(state(&immediate), "unverified", "{immediate}");
    assert_eq!(
        immediate["index"]["working_tree_verified"], false,
        "{immediate}"
    );

    let resettled = live
        .wait_for("second_edit", "second edit settle", |data| {
            !items(data).is_empty()
                && is_ready(data)
                && data["index"]["working_tree_verified"] == true
        })
        .await;
    assert_eq!(resettled["index"]["state"], "ready", "{resettled}");
    assert_eq!(
        resettled["index"]["working_tree_verified"], true,
        "{resettled}"
    );
    live.client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_for_other_head_is_not_trusted() {
    use gitcortex_mcp::mcp::readiness::{inspect, write_overlay_checkpoint};
    use std::collections::BTreeMap;

    let repo = TestRepo::new();
    repo.write("a.rs", ORIGINAL);
    let sha1 = repo.commit("c1");
    repo.write("b.rs", "pub fn second() {}\n");
    let sha2 = repo.commit("c2");

    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    assert_eq!(store.last_indexed_sha("main").unwrap().as_deref(), Some(sha2.as_str()));

    repo.write("a.rs", "pub fn dirty_on_sha2() {}\n");
    let diff = IncrementalIndexer::new(repo.root())
        .unwrap()
        .index_files_from_disk(&[repo.root().join("a.rs")])
        .unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    store.apply_diff("main", &diff).unwrap();
    let files: BTreeMap<_, _> = diff.source_hashes.into_iter().collect();

    // Checkpoint written with sha1 while HEAD and indexed_revision are sha2.
    write_overlay_checkpoint(repo.root(), "main", &sha1, &files).unwrap();
    let readiness = inspect(repo.root(), &store, "main");
    assert_eq!(readiness.state, "unverified");
    assert!(!readiness.working_tree_verified);

    // Matching sha2 certifies.
    write_overlay_checkpoint(repo.root(), "main", &sha2, &files).unwrap();
    let certified = inspect(repo.root(), &store, "main");
    assert_eq!(certified.state, "ready");
    assert!(certified.working_tree_verified);
}

#[tokio::test(flavor = "multi_thread")]
async fn changed_file_set_mismatch_is_not_certified() {
    use gitcortex_mcp::mcp::readiness::{inspect, write_overlay_checkpoint};
    use std::collections::BTreeMap;

    let repo = TestRepo::new();
    repo.write("a.rs", ORIGINAL);
    let sha = repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");

    repo.write("a.rs", "pub fn dirty_a() {}\n");
    let diff = IncrementalIndexer::new(repo.root())
        .unwrap()
        .index_files_from_disk(&[repo.root().join("a.rs")])
        .unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    store.apply_diff("main", &diff).unwrap();
    let mut files: BTreeMap<_, _> = diff.source_hashes.into_iter().collect();
    write_overlay_checkpoint(repo.root(), "main", &sha, &files).unwrap();
    assert_eq!(inspect(repo.root(), &store, "main").state, "ready");

    // Extra dirty file on disk not present in checkpoint.
    repo.write("b.rs", "pub fn unindexed_b() {}\n");
    let state_subset = inspect(repo.root(), &store, "main");
    assert_eq!(state_subset.state, "unverified");
    assert!(!state_subset.working_tree_verified);
    repo.remove("b.rs");
    assert_eq!(inspect(repo.root(), &store, "main").state, "ready");

    // Extra path in checkpoint not dirty on disk.
    files.insert(PathBuf::from("phantom.rs"), None);
    write_overlay_checkpoint(repo.root(), "main", &sha, &files).unwrap();
    let state_superset = inspect(repo.root(), &store, "main");
    assert_eq!(state_superset.state, "unverified");
    assert!(!state_superset.working_tree_verified);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_file_overlay_certifies_after_settle() {
    let live = Live::start("clean", true).await;
    let before = live.wait_ready().await;
    assert!(!items(&before).is_empty(), "precondition: {before}");

    live.repo.remove("a.rs");
    let settled = live
        .wait_for("original", "deleted file settle", |data| {
            items(data).is_empty()
                && is_ready(data)
                && data["index"]["working_tree_verified"] == true
        })
        .await;
    assert_eq!(settled["index"]["state"], "ready", "{settled}");
    assert_eq!(settled["index"]["working_tree_verified"], true, "{settled}");
    live.client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_with_valid_checkpoint_recertifies_without_reindex() {
    let repo = seed_repo("clean");
    let (daemon1, runtime1, socket1) = spawn_daemon(&repo);
    let client1 = connect(&socket1, true).await;
    let live1 = Live {
        client: client1,
        compact: true,
        _daemon: daemon1,
        _runtime: runtime1,
        repo,
    };
    live1.wait_ready().await;
    live1.repo.write("a.rs", "pub fn persisted_across_restart() {}\n");
    live1
        .wait_for("persisted_across_restart", "settle before restart", |data| {
            !items(data).is_empty()
                && is_ready(data)
                && data["index"]["working_tree_verified"] == true
        })
        .await;
    let Live {
        client: client1,
        _daemon: daemon1,
        _runtime: runtime1,
        repo,
        ..
    } = live1;
    client1.cancel().await.unwrap();
    drop(daemon1);
    drop(runtime1);

    // Restart daemon over the same dirty working tree and valid checkpoint.
    let (daemon2, runtime2, socket2) = spawn_daemon(&repo);
    let client2 = connect(&socket2, true).await;
    let live2 = Live {
        client: client2,
        compact: true,
        _daemon: daemon2,
        _runtime: runtime2,
        repo,
    };
    let after = live2
        .wait_for("persisted_across_restart", "recertify after restart", |data| {
            !items(data).is_empty()
                && is_ready(data)
                && data["index"]["working_tree_verified"] == true
        })
        .await;
    // Recertified directly from the persisted checkpoint without running a
    // re-index batch (graph_generation remains 0).
    assert_eq!(after["graph_generation"], 0, "{after}");
    assert_eq!(after["index"]["state"], "ready", "{after}");
    assert_eq!(after["index"]["working_tree_verified"], true, "{after}");
    live2.client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_with_tampered_or_corrupt_checkpoint_stays_unverified() {
    use gitcortex_mcp::mcp::readiness::{inspect, overlay_checkpoint_path, write_overlay_checkpoint};
    use std::collections::BTreeMap;

    let repo = TestRepo::new();
    repo.write("a.rs", ORIGINAL);
    let sha = repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");

    repo.write("a.rs", "pub fn edited_before_tamper() {}\n");
    let diff = IncrementalIndexer::new(repo.root())
        .unwrap()
        .index_files_from_disk(&[repo.root().join("a.rs")])
        .unwrap();
    store.set_working_tree_dirty("main", true).unwrap();
    store.apply_diff("main", &diff).unwrap();
    let files: BTreeMap<_, _> = diff.source_hashes.into_iter().collect();
    write_overlay_checkpoint(repo.root(), "main", &sha, &files).unwrap();
    drop(store);

    let cp_path = overlay_checkpoint_path(repo.root(), "main");

    // Tamper with the stored blob hash before reopening the store.
    std::fs::write(
        &cp_path,
        serde_json::json!({
            "head": sha,
            "branch_key": "main",
            "files": { "a.rs": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef" }
        })
        .to_string(),
    )
    .unwrap();
    let reopened = KuzuGraphStore::open(repo.root()).unwrap();
    let tampered = inspect(repo.root(), &reopened, "main");
    assert_eq!(tampered.state, "unverified");
    assert!(!tampered.working_tree_verified);

    // Corrupt the checkpoint file with invalid JSON.
    std::fs::write(&cp_path, b"corrupt-json-payload").unwrap();
    let corrupt = inspect(repo.root(), &reopened, "main");
    assert_eq!(corrupt.state, "unverified");
    assert!(!corrupt.working_tree_verified);
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_or_many_changed_files_stay_unverified() {
    use gitcortex_mcp::mcp::readiness::{MAX_OVERLAY_FILES, MAX_OVERLAY_FILE_BYTES};

    let live = Live::start("clean", true).await;
    live.wait_ready().await;

    // 1. Oversized changed file (> 512 KiB) is skipped by AST extraction and
    // must never be certified as ready/verified.
    let oversized = format!(
        "pub fn oversized_fn() {{}}\n{}",
        "// filler line\n".repeat((MAX_OVERLAY_FILE_BYTES as usize / 14) + 100)
    );
    live.repo.write("a.rs", &oversized);
    live.repo.write("b.rs", "pub fn companion_small() {}\n");
    let after_oversized = live
        .wait_for("companion_small", "watcher batch with oversized file", |data| {
            !items(data).is_empty() && state(data) != "degraded"
        })
        .await;
    assert_eq!(state(&after_oversized), "unverified", "{after_oversized}");
    assert_eq!(
        after_oversized["index"]["working_tree_verified"], false,
        "{after_oversized}"
    );

    // Restore a.rs to a normal size and verify certification recovers.
    live.repo.write("a.rs", "pub fn normal_size() {}\n");
    live.wait_for("normal_size", "recover after oversized", |data| {
        !items(data).is_empty()
            && is_ready(data)
            && data["index"]["working_tree_verified"] == true
    })
    .await;

    // 2. Many changed files (> MAX_OVERLAY_FILES) exceed the inspect cap and
    // stay unverified even after the watcher settles.
    for i in 0..=MAX_OVERLAY_FILES {
        live.repo
            .write(&format!("bulk_{i}.rs"), &format!("pub fn bulk_{i}() {{}}\n"));
    }
    let last_sym = format!("bulk_{MAX_OVERLAY_FILES}");
    let after_many = live
        .wait_for(&last_sym, "watcher batch with many files", |data| {
            !items(data).is_empty() && state(data) != "degraded"
        })
        .await;
    assert_eq!(state(&after_many), "unverified", "{after_many}");
    assert_eq!(
        after_many["index"]["working_tree_verified"], false,
        "{after_many}"
    );

    live.client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_tree_index_reports_error_and_allows_retry() {
    for compact in [false, true] {
        failed_tree_index_scenario(compact).await;
    }
}

async fn failed_tree_index_scenario(compact: bool) {
    let live = Live::start("tree", compact).await;
    live.wait_ready().await;
    let git_repo = git2::Repository::open(live.repo.root()).unwrap();
    let commit = git_repo
        .revparse_single("refs/heads/feat/x")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let tree = commit.tree().unwrap();
    let entry = tree.get_name("a.rs").unwrap();
    let blob_id = entry.id().to_string();
    let blob_path = git_repo
        .path()
        .join("objects")
        .join(&blob_id[..2])
        .join(&blob_id[2..]);
    let bytes = std::fs::read(&blob_path).unwrap();
    std::fs::remove_file(&blob_path).unwrap();

    let first = live.call("graph_stats", json!({"branch": "feat/x"})).await;
    assert_eq!(state(&first), "indexing", "{first}");

    let deadline = tokio::time::Instant::now() + DEADLINE;
    let failed = loop {
        let data = live.call("graph_stats", json!({"branch": "feat/x"})).await;
        if state(&data) == "missing" {
            break data;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "expected failure: {data}"
        );
        tokio::time::sleep(POLL).await;
    };
    let next_action = failed["index"]["next_action"].as_str().unwrap_or("");
    assert!(
        next_action.contains("failed"),
        "expected failure message in next_action: {failed}"
    );

    // Repair the loose blob and advance the branch so a new request retries and reaches ready.
    std::fs::write(&blob_path, bytes).unwrap();
    live.repo.git(&["checkout", "feat/x"]);
    live.repo.write("b.rs", "pub fn after_repair() {}\n");
    live.repo.commit("repair");
    live.repo.git(&["checkout", "main"]);

    let deadline = tokio::time::Instant::now() + DEADLINE;
    loop {
        let data = live
            .call(
                "lookup_symbol",
                json!({"name": "after_repair", "branch": "feat/x"}),
            )
            .await;
        if is_ready(&data) {
            assert!(!items(&data).is_empty(), "{data}");
            assert_eq!(data["index"]["working_tree_verified"], false);
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "retry: {data}");
        tokio::time::sleep(POLL).await;
    }
    live.client.cancel().await.unwrap();
}
