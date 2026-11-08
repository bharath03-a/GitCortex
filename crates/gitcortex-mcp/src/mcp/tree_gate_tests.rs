//! Readiness and job behavior through real MCP calls, with the job queue held
//! so an in-flight job can be observed deterministically.
use std::time::Duration;

use gitcortex_core::store::GraphStore;
use gitcortex_store::branch::{last_sha_path, storage_repo_id};
use rmcp::{model::CallToolRequestParams, service::RunningService, RoleClient, ServiceExt};
use serde_json::{json, Value};

use super::readiness::IndexReadiness;
use super::test_support::{commit_file, git, run_async, TempRepo};
use super::tools::GitCortexServer;
use super::tree_jobs::apply_tree_index;

const DEADLINE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(25);

struct Harness {
    client: RunningService<RoleClient, ()>,
    compact: bool,
    server: GitCortexServer,
    head: String,
    repo: TempRepo,
}

impl Harness {
    // main checked out; feat/x carries `feature_only`, never indexed.
    async fn start(compact: bool) -> Self {
        let repo = TempRepo::init();
        commit_file(repo.path(), "a.rs", "pub fn base() {}\n", "one");
        let head = commit_file(
            repo.path(),
            "a.rs",
            "pub fn base() {}\npub fn second() {}\n",
            "two",
        );
        git(repo.path(), &["switch", "-c", "feat/x"]);
        commit_file(repo.path(), "b.rs", "pub fn feature_only() {}\n", "feature");
        git(repo.path(), &["switch", "main"]);
        let server = GitCortexServer::new_daemon(repo.path()).expect("server");
        apply_tree_index(
            repo.path(),
            &mut *server.store_context().store.lock().expect("store"),
            "main",
            &head,
        )
        .expect("index main");
        let (near, far) = tokio::net::UnixStream::pair().expect("socket pair");
        let serving = server.clone_with_mode(compact);
        tokio::spawn(async move {
            if let Ok(service) = serving.serve(near).await {
                let _ = service.waiting().await;
            }
        });
        let client = ().serve(far).await.expect("client");
        Self {
            client,
            compact,
            server,
            head,
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
            .with_arguments(arguments.as_object().expect("object").clone())
    }

    async fn call(&self, action: &str, params: Value) -> Value {
        self.client
            .call_tool(self.request(action, params))
            .await
            .expect("call")
            .structured_content
            .expect("structured")
    }

    async fn stats(&self, branch: &str) -> Value {
        self.call("graph_stats", json!({"branch": branch})).await
    }

    async fn lookup(&self, name: &str, branch: &str) -> Value {
        self.call("lookup_symbol", json!({"name": name, "branch": branch}))
            .await
    }

    async fn wait_ready(&self, name: &str, branch: &str) -> Value {
        let deadline = tokio::time::Instant::now() + DEADLINE;
        loop {
            let data = self.lookup(name, branch).await;
            if state(&data) == "ready" {
                return data;
            }
            assert!(tokio::time::Instant::now() < deadline, "ready: {data}");
            tokio::time::sleep(POLL).await;
        }
    }

    fn markers_absent(&self, key: &str) {
        let sha = last_sha_path(&storage_repo_id(self.repo.path()), key);
        assert!(!sha.exists(), "{key}: {}", sha.display());
        assert!(!sha.with_extension("dirty").exists(), "{key}: dirty marker");
    }

    async fn stop(self) {
        self.client.cancel().await.expect("cancel");
    }
}

/// Holds the store mutex on its own thread, as an apply would, until dropped.
struct HeldStore {
    _release: std::sync::mpsc::Sender<()>,
}

fn hold_store(server: &GitCortexServer) -> HeldStore {
    let store = server.store_context().store;
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (locked, is_locked) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _held = store.lock().expect("store");
        locked.send(()).expect("signal");
        let _ = released.recv();
    });
    is_locked.recv().expect("store held");
    HeldStore { _release: release }
}

fn state(data: &Value) -> &str {
    data["index"]["state"].as_str().unwrap_or("")
}

fn next_action(data: &Value) -> &str {
    data["index"]["next_action"].as_str().unwrap_or("")
}

fn both_modes<F, Fut>(test: F)
where
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for compact in [false, true] {
        run_async(test(compact));
    }
}

#[test]
fn indexing_gate_blocks_queries_while_a_job_is_held() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        let permit = live.server.tree_jobs().hold_permit().await;
        for _ in 0..2 {
            let data = live.lookup("feature_only", "feat/x").await;
            assert_eq!(data["status"], "index_not_ready", "{data}");
            assert_eq!(state(&data), "indexing", "{data}");
            assert!(data.get("items").is_none(), "{data}");
        }
        drop(permit);
        let ready = live.wait_ready("feature_only", "feat/x").await;
        assert!(
            !ready["items"].as_array().expect("items").is_empty(),
            "{ready}"
        );
        live.stop().await;
    });
}

#[test]
fn duplicate_requests_spawn_one_job() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        let permit = live.server.tree_jobs().hold_permit().await;
        let (a, b, c, d, e) = tokio::join!(
            live.stats("feat/x"),
            live.stats("feat/x"),
            live.stats("feat/x"),
            live.stats("feat/x"),
            live.stats("feat/x"),
        );
        for data in [a, b, c, d, e] {
            assert_eq!(state(&data), "indexing", "{data}");
        }
        assert_eq!(live.server.tree_jobs().started(), 1);
        drop(permit);
        live.wait_ready("feature_only", "feat/x").await;
        assert_eq!(live.server.tree_jobs().started(), 1);
        live.stop().await;
    });
}

#[test]
fn non_branch_input_reports_missing_without_work() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        for input in [
            "main~1",
            "main^",
            "main@{0}",
            "main^{/second}",
            "refs/heads/main",
            "typo/no-such-branch",
        ] {
            let data = live.stats(input).await;
            assert_eq!(state(&data), "missing", "{input}: {data}");
            assert!(
                data["index"]["target_revision"].is_null(),
                "{input}: {data}"
            );
            assert!(next_action(&data).contains(input), "{input}: {data}");
            let gated = live.lookup("base", input).await;
            assert_eq!(gated["status"], "index_not_ready", "{input}: {gated}");
            live.markers_absent(input);
        }
        assert_eq!(live.server.tree_jobs().started(), 0);
        assert!(!live.server.tree_jobs().is_active());
        live.stop().await;
    });
}

#[test]
fn indexed_non_branch_keys_report_their_commit_without_a_job() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        let short = git(live.repo.path(), &["rev-parse", "--short", &live.head]);
        for key in [live.head.clone(), short] {
            apply_tree_index(
                live.repo.path(),
                &mut *live.server.store_context().store.lock().expect("store"),
                &key,
                &live.head,
            )
            .expect("index key");
            let data = live.lookup("second", &key).await;
            assert_eq!(state(&data), "ready", "{data}");
            assert_eq!(
                data["index"]["target_revision"],
                live.head.as_str(),
                "{data}"
            );
            assert!(
                !data["items"].as_array().expect("items").is_empty(),
                "{data}"
            );
        }
        assert_eq!(live.server.tree_jobs().started(), 0);
        live.stop().await;
    });
}

#[test]
fn remote_only_branch_explains_itself() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        git(
            live.repo.path(),
            &["update-ref", "refs/remotes/origin/foo", &live.head],
        );
        let data = live.stats("origin/foo").await;
        assert_eq!(state(&data), "missing", "{data}");
        let action = next_action(&data);
        assert!(action.contains("remote-only"), "{data}");
        assert!(action.contains("local branch"), "{data}");
        assert!(!action.contains("revspec"), "{data}");
        assert_eq!(live.server.tree_jobs().started(), 0);
        live.markers_absent("origin/foo");
        live.stop().await;
    });
}

#[test]
fn failed_job_reports_stable_actionable_next_step() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        let repo = git2::Repository::open(live.repo.path()).expect("repo");
        let tree = repo
            .revparse_single("refs/heads/feat/x")
            .and_then(|o| o.peel_to_tree())
            .expect("tree");
        let blob = tree.get_name("b.rs").expect("entry").id().to_string();
        let path = repo
            .path()
            .join("objects")
            .join(&blob[..2])
            .join(&blob[2..]);
        std::fs::remove_file(path).expect("corrupt fixture");

        let deadline = tokio::time::Instant::now() + DEADLINE;
        let failed = loop {
            let data = live.stats("feat/x").await;
            if state(&data) == "missing" {
                break data;
            }
            assert!(tokio::time::Instant::now() < deadline, "failure: {data}");
            tokio::time::sleep(POLL).await;
        };
        let action = next_action(&failed);
        assert!(action.contains("failed"), "{failed}");
        assert!(action.contains("60 s"), "{failed}");
        assert!(action.contains("close MCP sessions"), "{failed}");
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let later = live.stats("feat/x").await;
        assert_eq!(later["index"], failed["index"]);
        assert_ne!(later["status"], "index_changed", "{later}");
        live.stop().await;
    });
}

fn scope_readiness(
    server: &GitCortexServer,
    checkout: Option<&str>,
    scope: &str,
) -> IndexReadiness {
    let store = server.store_context().store;
    let store = store.lock().expect("store");
    server.scope_readiness(&*store, checkout, scope)
}

#[test]
fn unknown_checkout_is_unavailable_and_starts_no_job() {
    run_async(async {
        let live = Harness::start(true).await;
        let state = scope_readiness(&live.server, None, "main");
        assert_eq!(state.state, "unavailable");
        assert!(!state.working_tree_verified);
        let feature = scope_readiness(&live.server, None, "feat/x");
        assert_eq!(feature.state, "unavailable");
        assert_eq!(live.server.tree_jobs().started(), 0);
        live.stop().await;
    });
}

#[test]
fn readiness_reports_indexing_instead_of_waiting_for_an_apply() {
    run_async(async {
        let live = Harness::start(true).await;
        let held = hold_store(&live.server);
        let applying = live.server.tree_jobs().mark_applying();
        let server = live.server.clone();
        let reply = tokio::task::spawn_blocking(move || {
            server.readiness_scopes(&["main".to_owned(), "feat/x".to_owned()])
        });
        let scopes = tokio::time::timeout(DEADLINE, reply)
            .await
            .expect("readiness must not wait for the store")
            .expect("task")
            .expect("readiness");
        assert!(
            scopes.iter().all(|scope| scope.state == "indexing"),
            "{scopes:?}"
        );
        assert!(scopes.iter().all(|scope| !scope.working_tree_verified));
        assert_eq!(live.server.tree_jobs().started(), 0);
        drop(applying);
        drop(held);
        live.stop().await;
    });
}

#[test]
fn diagnostics_answer_without_waiting_for_an_apply() {
    run_async(async {
        let live = Harness::start(true).await;
        let held = hold_store(&live.server);
        let applying = live.server.tree_jobs().mark_applying();
        let result = tokio::time::timeout(
            DEADLINE,
            live.client
                .call_tool(live.request("graph_stats", json!({}))),
        )
        .await
        .expect("graph_stats must not wait for the store")
        .expect("call");
        assert_ne!(result.is_error, Some(true), "{result:?}");
        let data = result.structured_content.expect("structured");
        assert_eq!(state(&data), "indexing", "{data}");
        drop(applying);
        drop(held);
        live.stop().await;
    });
}

#[test]
fn checked_out_branch_never_starts_a_tree_job() {
    both_modes(|compact| async move {
        let live = Harness::start(compact).await;
        let data = live.stats("main").await;
        assert_ne!(state(&data), "indexing", "{data}");
        assert_eq!(live.server.tree_jobs().started(), 0);
        let store = live.server.store_context().store;
        assert!(!store
            .lock()
            .expect("store")
            .working_tree_dirty("main")
            .expect("dirty"));
        drop(store);
        live.stop().await;
    });
}
