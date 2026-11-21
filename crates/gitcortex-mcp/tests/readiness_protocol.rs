//! Exercise initialized MCP sessions, not just internal tool functions.
use std::{path::Path, process::Command};

use gitcortex_core::store::GraphStore;
use gitcortex_mcp::mcp::tools::GitCortexServer;
use gitcortex_store::kuzu::KuzuGraphStore;
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {:?}", output.stderr);
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
async fn protocol_reports_scope_and_rejects_missing_or_stale_impact() {
    // One fixture per mode avoids simultaneous Kuzu opens in this process.
    for compact in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git(root, &["init", "-b", "main"]);
        git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@t",
                "commit",
                "--allow-empty",
                "-m",
                "start",
            ],
        );
        let head = git(root, &["rev-parse", "HEAD"]);
        let mut store = KuzuGraphStore::open(root).unwrap();
        store.set_last_indexed_sha("main", &head).unwrap();
        drop(store);
        let server = GitCortexServer::new_with_mode(root, compact).unwrap();
        let (server_io, client_io) = tokio::io::duplex(65536);
        let task = tokio::spawn(async move { server.serve(server_io).await.unwrap() });
        let client = ().serve(client_io).await.unwrap();
        let service = task.await.unwrap();
        let tools = client.list_all_tools().await.unwrap();
        assert!(tools
            .iter()
            .any(|t| t.name == if compact { "gcx" } else { "lookup_symbol" }));
        let call = |action: &str, params: Value| {
            let (name, arguments) = if compact {
                (
                    "gcx".to_owned(),
                    json!({"action": action, "params": params}),
                )
            } else {
                (action.to_owned(), params)
            };
            CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone())
        };
        let result = client
            .call_tool(call("lookup_symbol", json!({"name":"absent"})))
            .await
            .unwrap();
        let data = result.structured_content.unwrap();
        assert!(
            data.is_object(),
            "MCP structuredContent must be an object: {data}"
        );
        assert_eq!(data["items"], json!([]));
        assert_eq!(data["index"]["state"], "ready");
        assert_eq!(data["index"]["branch"], "main");

        std::fs::write(root.join("edit.rs"), "pub fn in_progress() {}\n").unwrap();
        let result = client
            .call_tool(call("lookup_symbol", json!({"name":"absent"})))
            .await
            .unwrap();
        assert_eq!(
            result.structured_content.unwrap()["index"]["state"],
            "unverified"
        );
        let result = client
            .call_tool(call("pre_edit_impact", json!({"function_name":"absent"})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        let data = result.structured_content.unwrap();
        assert_eq!(data["risk_level"], "UNKNOWN");
        assert_eq!(data["evidence_complete"], false);
        std::fs::remove_file(root.join("edit.rs")).unwrap();

        // The daemon must follow a checkout immediately, not its startup branch.
        git(root, &["checkout", "-b", "feature"]);
        let result = client
            .call_tool(call("pre_edit_impact", json!({"function_name":"absent"})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        let data = result.structured_content.unwrap();
        assert_eq!(data["index"]["branch"], "feature");
        assert_eq!(data["index"]["state"], "missing");
        assert!(data.get("risk_level").is_none());

        // A missing feature graph must not poison the main branch's state.
        let result = client
            .call_tool(call("graph_stats", json!({"branch":"main"})))
            .await
            .unwrap();
        assert_eq!(
            result.structured_content.unwrap()["index"]["state"],
            "ready"
        );
        git(root, &["checkout", "main"]);
        git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@t",
                "commit",
                "--allow-empty",
                "-m",
                "advance",
            ],
        );
        let result = client
            .call_tool(call("pre_edit_impact", json!({"function_name":"absent"})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["index"]["state"],
            "stale"
        );
        // Historical main is checked against main, not the checkout's HEAD.
        git(root, &["checkout", "-b", "later"]);
        git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@t",
                "commit",
                "--allow-empty",
                "-m",
                "later",
            ],
        );
        let result = client
            .call_tool(call("graph_stats", json!({"branch":"main"})))
            .await
            .unwrap();
        let data = result.structured_content.unwrap();
        assert_ne!(
            data["index"]["target_revision"],
            data["index"]["checkout_revision"]
        );
        assert_eq!(data["index"]["state"], "stale");
        git(root, &["checkout", "--detach", &head]);
        let result = client
            .call_tool(call("graph_stats", json!({})))
            .await
            .unwrap();
        let data = result.structured_content.unwrap();
        assert_eq!(
            data["index"]["branch"],
            git(root, &["rev-parse", "--short", "HEAD"])
        );
        assert_ne!(data["index"]["branch"], "main");
        // Planning is pure and does not need an index.
        let result = client
            .call_tool(call("plan_query", json!({"question":"Who calls absent?"})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        client.cancel().await.unwrap();
        service.cancel().await.unwrap();
    }
}
