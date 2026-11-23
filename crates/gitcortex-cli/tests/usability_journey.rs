mod common;

use std::path::{Path, PathBuf};

use common::*;

fn single_store_dir(data_root: &Path) -> PathBuf {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(data_root)
        .expect("read isolated store root")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(
        dirs.len(),
        1,
        "expected exactly one repo store dir under {}, got {:?}",
        data_root.display(),
        dirs
    );
    dirs.pop().unwrap()
}

fn fixture_with_call_graph() -> Fixture {
    let f = Fixture::new();
    std::fs::create_dir_all(f.repo.path().join("src")).unwrap();
    std::fs::write(
        f.repo.path().join("src/lib.rs"),
        "pub fn leaf_helper() -> i32 {\n    41\n}\n\npub fn entry_caller() -> i32 {\n    leaf_helper() + 1\n}\n",
    )
    .unwrap();
    f.git(&["add", "."]);
    f.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=t@t",
        "commit",
        "-m",
        "add call graph",
    ]);
    f
}

#[test]
fn first_use_to_answer_and_deinit() {
    let f = fixture_with_call_graph();

    // 1. Explicit CLI-only init on a clean repository.
    let init = f.run(&["init", "--editor", "none"]);
    assert!(init.status.success(), "{}", message(&init));
    assert!(
        f.git(&["status", "--porcelain"]).is_empty(),
        "CLI-only init should leave a clean tracked working tree"
    );

    // 2. Doctor and status confirm a ready index.
    let doctor = f.run(&["doctor"]);
    assert!(doctor.status.success(), "{}", message(&doctor));

    let status = f.run(&["status"]);
    assert!(status.status.success(), "{}", message(&status));
    let status_out = message(&status);
    assert!(status_out.contains("ready"), "{status_out}");

    // 3. First-answer queries return verifiable file/line evidence.
    let search = f.run(&[
        "query",
        "search",
        "leaf_helper",
        "--limit",
        "10",
        "--format",
        "agent-json",
    ]);
    assert!(search.status.success(), "{}", message(&search));
    let search_json: serde_json::Value =
        serde_json::from_slice(&search.stdout).expect("valid search agent-json");
    let search_str = search_json.to_string();
    assert!(search_str.contains("leaf_helper"), "{search_str}");
    assert!(search_str.contains("src/lib.rs"), "{search_str}");

    let ctx = f.run(&[
        "query",
        "symbol-context",
        "leaf_helper",
        "--format",
        "agent-json",
    ]);
    assert!(ctx.status.success(), "{}", message(&ctx));
    let ctx_json: serde_json::Value =
        serde_json::from_slice(&ctx.stdout).expect("valid symbol-context agent-json");
    let ctx_str = ctx_json.to_string();
    assert!(ctx_str.contains("leaf_helper"), "{ctx_str}");
    assert!(ctx_str.contains("entry_caller"), "{ctx_str}");
    assert!(ctx_str.contains("src/lib.rs"), "{ctx_str}");

    let callers = f.run(&["query", "find-callers", "leaf_helper"]);
    assert!(callers.status.success(), "{}", message(&callers));
    let callers_out = message(&callers);
    assert!(callers_out.contains("entry_caller"), "{callers_out}");
    assert!(callers_out.contains("src/lib.rs"), "{callers_out}");

    // 4. Uncommitted source edit transitions readiness to `unverified`.
    std::fs::write(
        f.repo.path().join("src/lib.rs"),
        "pub fn leaf_helper() -> i32 {\n    42\n}\n\npub fn entry_caller() -> i32 {\n    leaf_helper() + 1\n}\n",
    )
    .unwrap();
    let dirty_status = f.run(&["status"]);
    assert!(dirty_status.status.success(), "{}", message(&dirty_status));
    assert!(
        message(&dirty_status).contains("unverified"),
        "{}",
        message(&dirty_status)
    );
    let dirty_doctor = f.run(&["doctor"]);
    assert!(
        !dirty_doctor.status.success(),
        "doctor must fail when checkout is unverified: {}",
        message(&dirty_doctor)
    );

    // 5. Commit on a feature branch, sync deterministically, and run documented HEAD blast-radius.
    f.git(&[
        "-c",
        "core.hooksPath=/dev/null",
        "checkout",
        "-b",
        "feat/journey",
    ]);
    f.git(&["add", "src/lib.rs"]);
    f.git(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "user.name=Test",
        "-c",
        "user.email=t@t",
        "commit",
        "-m",
        "update leaf_helper",
    ]);
    let hook = f.run(&["hook"]);
    assert!(hook.status.success(), "{}", message(&hook));

    let blast = f.run(&[
        "blast-radius",
        "--base",
        "main",
        "--head",
        "HEAD",
        "--format",
        "text",
    ]);
    assert!(blast.status.success(), "{}", message(&blast));
    let blast_out = message(&blast);
    assert!(blast_out.contains("leaf_helper"), "{blast_out}");
    assert!(blast_out.contains("entry_caller"), "{blast_out}");

    // 6. Seed an unrelated editor MCP entry, install Claude integration, then deinit.
    let mcp_path = f.repo.path().join(".mcp.json");
    std::fs::write(
        &mcp_path,
        r#"{
  "mcpServers": {
    "unrelated-server": {
      "command": "echo",
      "args": ["keep-me"]
    }
  }
}
"#,
    )
    .unwrap();

    let init_claude = f.run(&["init", "--editor", "claude"]);
    assert!(init_claude.status.success(), "{}", message(&init_claude));
    let before_deinit: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(before_deinit["mcpServers"]["unrelated-server"].is_object());
    assert!(before_deinit["mcpServers"]["gitcortex"].is_object());

    // `gcx deinit` without `--purge` removes GitCortex config while keeping unrelated entries and the store.
    let deinit = f.run(&["deinit"]);
    assert!(deinit.status.success(), "{}", message(&deinit));

    let after_deinit: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(
        after_deinit["mcpServers"]["unrelated-server"].is_object(),
        "deinit must preserve unrelated MCP entries: {after_deinit}"
    );
    assert!(
        after_deinit["mcpServers"]["gitcortex"].is_null(),
        "deinit must remove the gitcortex MCP entry: {after_deinit}"
    );

    let store_dir = single_store_dir(f.data.path());
    assert!(
        store_dir.join("graph.kuzu").exists(),
        "deinit without --purge must retain the local graph store"
    );

    // Explicit `gcx deinit --purge` removes the repository's local graph data.
    let purge = f.run(&["deinit", "--purge"]);
    assert!(purge.status.success(), "{}", message(&purge));
    assert!(
        !store_dir.join("graph.kuzu").exists(),
        "deinit --purge must remove graph.kuzu"
    );
    let after_purge = f.run(&["status"]);
    assert_eq!(
        after_purge.status.code(),
        Some(3),
        "status after deinit --purge should report missing store: {}",
        message(&after_purge)
    );
}

#[test]
fn deleted_store_recovery_is_actionable() {
    let f = fixture_with_call_graph();
    f.init();

    let before = f.run(&["status"]);
    assert!(before.status.success(), "{}", message(&before));
    assert!(message(&before).contains("ready"), "{}", message(&before));

    // Delete the local store directory completely.
    let store_dir = single_store_dir(f.data.path());
    std::fs::remove_dir_all(&store_dir).expect("delete store directory");
    assert!(!store_dir.exists());

    // Status exits 3 (NotReady) with actionable recovery advice and no false safety.
    let missing = f.run(&["status"]);
    assert_eq!(missing.status.code(), Some(3), "{}", message(&missing));
    let missing_out = message(&missing);
    assert!(missing_out.contains("missing"), "{missing_out}");
    assert!(missing_out.contains("gcx init"), "{missing_out}");
    assert!(
        missing_out.contains("does not mean no dependencies"),
        "{missing_out}"
    );

    // Recover using the documented command and verify ready status + passing doctor.
    let recover = f.run(&["init", "--editor", "none"]);
    assert!(recover.status.success(), "{}", message(&recover));

    let after = f.run(&["status"]);
    assert!(after.status.success(), "{}", message(&after));
    assert!(message(&after).contains("ready"), "{}", message(&after));

    let doctor = f.run(&["doctor"]);
    assert!(doctor.status.success(), "{}", message(&doctor));
}

#[test]
fn upgrade_schema_failure_never_suggests_blind_deletion() {
    let f = fixture_with_call_graph();
    f.init();

    let store_dir = single_store_dir(f.data.path());
    let schema_path = store_dir.join("schema_version");
    let graph_path = store_dir.join("graph.kuzu");
    assert!(graph_path.exists(), "graph.kuzu should exist after init");

    // Simulate an incompatible schema version from a different binary/upgrade.
    std::fs::write(&schema_path, "999\n").expect("write incompatible schema version");

    // Both `status` and `query` fail closed without deleting the store or advising `rm -rf`.
    for args in [
        &["status"][..],
        &["query", "lookup-symbol", "leaf_helper"][..],
    ] {
        let out = f.run(args);
        assert!(
            !out.status.success(),
            "command {:?} must fail on incompatible schema: {}",
            args,
            message(&out)
        );
        let text = message(&out);
        let lower = text.to_lowercase();
        assert!(
            lower.contains("schema"),
            "error for {:?} should mention schema mismatch: {text}",
            args
        );
        assert!(
            lower.contains("gcx init"),
            "error for {:?} should point to explicit rebuild via gcx init: {text}",
            args
        );
        for forbidden in ["rm -rf", "rm -r", "delete the store", "blindly"] {
            assert!(
                !lower.contains(forbidden),
                "error for {:?} must never suggest blind deletion ({forbidden}): {text}",
                args
            );
        }
    }

    // Store payload and schema marker must remain untouched on disk (no silent deletion).
    assert!(
        graph_path.exists(),
        "incompatible schema failure must not silently delete graph.kuzu"
    );
    assert_eq!(
        std::fs::read_to_string(&schema_path).unwrap().trim(),
        "999",
        "incompatible schema failure must not mutate schema_version"
    );

    // Storage inventory remains inspectable before the user chooses to rebuild.
    let storage_list = f.run(&["storage", "list"]);
    assert!(storage_list.status.success(), "{}", message(&storage_list));

    // Explicit rebuild recovers cleanly to ready state.
    let rebuild = f.run(&["init", "--editor", "none"]);
    assert!(rebuild.status.success(), "{}", message(&rebuild));

    let after_status = f.run(&["status"]);
    assert!(after_status.status.success(), "{}", message(&after_status));
    assert!(
        message(&after_status).contains("ready"),
        "{}",
        message(&after_status)
    );

    let after_query = f.run(&["query", "lookup-symbol", "leaf_helper"]);
    assert!(after_query.status.success(), "{}", message(&after_query));
    assert!(
        message(&after_query).contains("src/lib.rs"),
        "{}",
        message(&after_query)
    );
}
