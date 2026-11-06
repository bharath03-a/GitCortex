mod support;
use gitcortex_mcp::mcp::server::serve_daemon;
use gitcortex_store::kuzu::KuzuGraphStore;
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};
use support::*;
use tokio::io::AsyncWriteExt;
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn daemon_child() {
    if let Some(root) = std::env::var_os("GCX_TRUST_CHILD_ROOT") {
        serve_daemon(root.into()).await.unwrap();
    }
}

async fn run(case: &str, compact: bool) {
    let repo = TestRepo::new();
    repo.write(
        "a.rs",
        "pub fn original() {}\npub fn caller() { original(); }",
    );
    repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    drop(store);
    if case == "restart" {
        // Persist a dirty overlay, close the owner, then restart on changed disk.
        let mut store = KuzuGraphStore::open(repo.root()).unwrap();
        use gitcortex_core::store::GraphStore;
        repo.write("a.rs", "pub fn overlay() {}");
        let diff = gitcortex_indexer::IncrementalIndexer::new(repo.root())
            .unwrap()
            .index_files_from_disk(&[repo.root().join("a.rs")])
            .unwrap();
        store.set_working_tree_dirty("main", true).unwrap();
        store.apply_diff("main", &diff).unwrap();
        repo.write("a.rs", "pub fn restarted() {}");
    }
    if case == "omitted" {
        repo.git(&["checkout", "-b", "feat/x"]);
        repo.write("a.rs", "pub fn feature_only() {}");
        repo.commit("feature");
    }
    // A short alias also keeps Unix socket names below macOS sockaddr limits.
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let data = runtime.path().join("store");
    std::os::unix::fs::symlink(std::env::var_os("GCX_STORE_PATH").unwrap(), &data).unwrap();
    let _daemon = Daemon(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "daemon_child", "--nocapture"])
            .env("GCX_TRUST_CHILD_ROOT", repo.root())
            .env("GCX_STORE_PATH", &data)
            .env("GCX_CACHE_PATH", runtime.path().join("cache"))
            .env("GCX_DISABLE_SEMANTIC", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let id = gitcortex_store::branch::repo_id(repo.root());
    let socket = data.join(id).join("mcp.sock");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
            break stream;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "daemon startup at {}",
            socket.display()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    stream
        .write_all(&[if compact { 0 } else { 1 }])
        .await
        .unwrap();
    let client = ().serve(stream).await.unwrap();
    let call = |action: &str, params: Value| {
        let (name, arguments) = if compact {
            ("gcx", json!({"action": action, "params":params}))
        } else {
            (action, params)
        };
        CallToolRequestParams::new(name.to_owned())
            .with_arguments(arguments.as_object().unwrap().clone())
    };
    let query = || call("lookup_symbol", json!({"name":"original"}));
    if case == "restart" {
        let result = client
            .call_tool(call("lookup_symbol", json!({"name":"overlay"})))
            .await
            .unwrap()
            .structured_content
            .unwrap();
        if result["index"]["state"] == "ready" {
            assert_eq!(result["index"]["working_tree_verified"], true);
            assert_eq!(
                result["items"],
                json!([]),
                "stale startup data certified: {result}"
            );
        } else {
            assert_eq!(result["index"]["working_tree_verified"], false);
        }
    }

    // Poll the actual readiness barrier, rather than sleeping through startup.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let data = client
            .call_tool(query())
            .await
            .unwrap()
            .structured_content
            .unwrap();
        if data["index"]["state"] != "degraded" {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "startup: {data}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    match case {
        "edit" => repo.write("a.rs", "pub fn live_added() {}"),
        "delete" => repo.remove("a.rs"),
        "switch" => {
            repo.git(&["checkout", "-b", "feat/x"]);
            repo.write("a.rs", "pub fn feature_only() {}");
            repo.commit("feature");
        }
        "detached" => {
            repo.git(&["checkout", "--detach"]);
        }
        _ => {}
    }
    if matches!(case, "omitted" | "switch" | "detached") {
        let data = client
            .call_tool(query())
            .await
            .unwrap()
            .structured_content
            .unwrap();
        let expected = if case == "detached" {
            repo.git(&["rev-parse", "--short", "HEAD"])
        } else {
            "feat/x".to_owned()
        };
        assert_eq!(data["index"]["branch"], expected);
        assert_ne!(data["index"]["branch"], "main");
        if data["index"]["state"] == "ready" && case != "detached" {
            assert_eq!(data["items"], json!([]), "main data leaked: {data}");
        }
    }
    if matches!(case, "edit" | "delete" | "restart") {
        let name = match case {
            "edit" => "live_added",
            "restart" => "restarted",
            _ => "original",
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3); // MAX_BATCH_MS + 2s
        loop {
            let data = client
                .call_tool(call("lookup_symbol", json!({"name":name})))
                .await
                .unwrap()
                .structured_content
                .unwrap();
            let visible = data["items"].as_array().is_some_and(|items| {
                if case == "delete" {
                    items.is_empty()
                } else {
                    !items.is_empty()
                }
            });
            if visible
                && data["index"]["state"] != "degraded"
                && (case != "edit" || data["index"]["working_tree_verified"] == true)
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "debounce deadline: {data}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    if case == "schema" {
        let tools = client.list_all_tools().await.unwrap();
        for (action, params) in [
            ("list_definitions", json!({"file":"a.rs"})),
            ("find_callers", json!({"function_name":"original"})),
        ] {
            let result = client.call_tool(call(action, params)).await.unwrap();
            assert_ne!(result.is_error, Some(true), "{result:?}");
            let data = result.structured_content.unwrap();
            let tool = tools
                .iter()
                .find(|t| t.name == if compact { "gcx" } else { action })
                .unwrap();
            // MCP requires object structuredContent even when no outputSchema
            // is advertised. Validate the advertised schema whenever present.
            let schema = tool
                .output_schema
                .as_ref()
                .map(|s| Value::Object((**s).clone()))
                .unwrap_or_else(|| json!({"type":"object"}));
            assert!(
                jsonschema::is_valid(&schema, &data),
                "schema {schema}: {data}"
            );
            let key = if action == "find_callers" {
                "evidence"
            } else {
                "items"
            };
            assert!(
                !data[key]
                    .as_array()
                    .unwrap_or_else(|| panic!("{action}: {data}"))
                    .is_empty(),
                "{action}: {data}"
            );
            assert_eq!(data["index"]["branch"], "main");
        }
    }
    client.cancel().await.unwrap();
}
macro_rules! live {
    ($name:ident, $case:literal) => {
        #[tokio::test]
        async fn $name() {
            for compact in [false, true] {
                run($case, compact).await;
            }
        }
    };
}
#[tokio::test]
#[ignore = "trust-ledger: B3-001"]
async fn uncommitted_edit_visible_after_debounce() {
    for compact in [false, true] {
        run("edit", compact).await;
    }
}
live!(deleted_file_symbols_disappear, "delete");
live!(branch_omitted_answers_current_checkout, "omitted");
live!(branch_switch_while_running_updates_index_branch, "switch");
live!(
    restart_with_dirty_overlay_not_ready_until_verified,
    "restart"
);
live!(detached_head_never_answers_main, "detached");
live!(array_results_validate_against_output_schema, "schema");
