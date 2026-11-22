mod common;
use common::*;

#[test]
fn doctor_local_antigravity_matches_installer() {
    let f = Fixture::new();
    let o = f.run(&["init", "--editor", "antigravity"]);
    assert!(o.status.success(), "{}", message(&o));
    let local_cfg = f
        .repo
        .path()
        .join(".agents/plugins/gitcortex/mcp_config.json");
    assert!(local_cfg.exists(), "local antigravity config must exist");
    assert!(
        !f.home.path().join(".antigravity/mcp.json").exists(),
        "must not write global config without --global-editor-config"
    );

    // Generated plugin Markdown files leave the working tree unverified until
    // committed; commit them and sync the isolated store with `gcx hook`.
    f.git(&["add", "."]);
    f.git(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "user.name=Test",
        "-c",
        "user.email=t@t",
        "commit",
        "-m",
        "add antigravity plugin",
    ]);
    let hook = f.run(&["hook"]);
    assert!(hook.status.success(), "{}", message(&hook));

    let doc = f.run(&["doctor"]);
    let m = message(&doc);
    assert!(doc.status.success(), "{m}");
    assert!(
        m.contains("[ok] Antigravity registration found (local,"),
        "{m}"
    );
    assert!(m.contains("mcp_config.json"), "{m}");
}

#[test]
fn doctor_configured_is_not_connected() {
    let f = Fixture::new();
    for (editor, extra) in [
        ("claude", None),
        ("cursor", None),
        ("copilot", None),
        ("antigravity", None),
        ("codex", None),
        ("windsurf", Some("--global-editor-config")),
    ] {
        let mut args = vec!["init", "--editor", editor];
        if let Some(flag) = extra {
            args.push(flag);
        }
        let o = f.run(&args);
        assert!(o.status.success(), "init {editor}: {}", message(&o));
    }

    f.git(&["add", "."]);
    f.git(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "user.name=Test",
        "-c",
        "user.email=t@t",
        "commit",
        "-m",
        "add editor configs",
    ]);
    let hook = f.run(&["hook"]);
    assert!(hook.status.success(), "{}", message(&hook));

    let doc = f.run(&["doctor"]);
    let m = message(&doc);
    assert!(doc.status.success(), "{m}");
    for name in [
        "Claude Code",
        "Cursor",
        "Windsurf",
        "Copilot",
        "Antigravity",
        "Codex",
    ] {
        assert!(
            m.contains(&format!("[ok] {name} registration found")),
            "missing registration found for {name}:\n{m}"
        );
    }
    assert!(
        m.contains("does not confirm an active connection"),
        "doctor should clarify registration is not proof of live connection:\n{m}"
    );
}

#[test]
fn doctor_substring_is_not_registration() {
    let f = Fixture::new();
    f.init();

    // Seed every supported config path with a valid document containing the
    // substring "gitcortex" / "gcx" in comments or unrelated entries, NOT a
    // real gitcortex -> gcx serve MCP registration.
    std::fs::write(
        f.repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"other":{"command":"echo","args":["gitcortex","gcx"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".cursor")).unwrap();
    std::fs::write(
        f.repo.path().join(".cursor/mcp.json"),
        r#"{"mcpServers":{"gitcortex":{"command":"echo","args":["serve"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".vscode")).unwrap();
    std::fs::write(
        f.repo.path().join(".vscode/mcp.json"),
        r#"{"servers":{"gitcortex":{"command":"gcx","args":["status"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".agents/plugins/gitcortex")).unwrap();
    std::fs::write(
        f.repo
            .path()
            .join(".agents/plugins/gitcortex/mcp_config.json"),
        r#"{"notes":"gitcortex gcx serve"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".codex")).unwrap();
    std::fs::write(
        f.repo.path().join(".codex/config.toml"),
        "# [mcp_servers.gitcortex]\n[mcp_servers.other]\ncommand = \"echo\"\nargs = [\"gitcortex\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(f.home.path().join(".codeium/windsurf")).unwrap();
    std::fs::write(
        f.home.path().join(".codeium/windsurf/mcp_config.json"),
        r#"{"mcpServers":{"other":{"command":"gcx","args":["serve"]}}}"#,
    )
    .unwrap();

    let doc = f.run(&["doctor"]);
    let m = message(&doc);
    assert!(doc.status.success(), "{m}");
    assert!(
        !m.contains("registration found ("),
        "substring/wrong-command entries must never be certified as registrations:\n{m}"
    );
}

#[test]
fn doctor_malformed_config_reports_path() {
    let f = Fixture::new();
    f.init();

    let cases = [
        (f.repo.path().join(".mcp.json"), "{ not valid json"),
        (f.repo.path().join(".cursor/mcp.json"), "[1, 2, 3]"),
        (f.repo.path().join(".vscode/mcp.json"), "{ bad"),
        (
            f.repo
                .path()
                .join(".agents/plugins/gitcortex/mcp_config.json"),
            "not json",
        ),
        (
            f.repo.path().join(".codex/config.toml"),
            "[mcp_servers.gitcortex\nbroken = ",
        ),
        (
            f.home.path().join(".codeium/windsurf/mcp_config.json"),
            "{ broken",
        ),
    ];

    for (path, content) in &cases {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    let doc = f.run(&["doctor"]);
    let m = message(&doc);
    assert!(doc.status.success(), "{m}");
    assert!(
        !m.contains("registration found ("),
        "malformed configs must not report registration found:\n{m}"
    );
    for (path, _) in &cases {
        let needle = path.file_name().unwrap().to_str().unwrap();
        assert!(
            m.contains("malformed") && m.contains(needle),
            "expected malformed report mentioning {needle}:\n{m}"
        );
    }
}

#[test]
fn doctor_setup_hints_parse() {
    let f = Fixture::new();
    f.init();

    let doc = f.run(&["doctor"]);
    let m = message(&doc);
    assert!(doc.status.success(), "{m}");

    assert!(
        !m.contains("Agy CLI") && !m.contains("agy-cli"),
        "unsupported Agy CLI row/hint must be absent:\n{m}"
    );
    assert!(
        !m.contains("--editor <name>"),
        "unparsable --editor <name> command hint must be absent:\n{m}"
    );

    let mut tokens = Vec::new();
    for line in m.lines() {
        if let Some(rest) = line.split("gcx init --editor ").nth(1) {
            let token = rest
                .split(|c: char| c.is_whitespace() || c == ')' || c == '`' || c == ',')
                .next()
                .unwrap_or("");
            if !token.is_empty() {
                tokens.push(token.to_owned());
            }
        }
    }
    assert_eq!(
        tokens.len(),
        6,
        "expected exactly 6 editor setup hints, got {tokens:?} in:\n{m}"
    );

    for token in tokens {
        let test_fixture = Fixture::new();
        let o = test_fixture.run(&["init", "--editor", &token]);
        assert!(
            o.status.success(),
            "doctor hint token '{token}' failed to parse/run:\n{}",
            message(&o)
        );
    }
}

#[test]
fn editor_configs_preserve_unrelated_entries() {
    let f = Fixture::new();

    // Seed unrelated entries in all 6 config files before running installers.
    std::fs::write(
        f.repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"keep_claude":{"command":"node","args":["a.js"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".cursor")).unwrap();
    std::fs::write(
        f.repo.path().join(".cursor/mcp.json"),
        r#"{"mcpServers":{"keep_cursor":{"command":"node","args":["b.js"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".vscode")).unwrap();
    std::fs::write(
        f.repo.path().join(".vscode/mcp.json"),
        r#"{"servers":{"keep_copilot":{"type":"stdio","command":"node","args":["c.js"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".agents/plugins/gitcortex")).unwrap();
    std::fs::write(
        f.repo
            .path()
            .join(".agents/plugins/gitcortex/mcp_config.json"),
        r#"{"mcpServers":{"keep_agy":{"command":"node","args":["d.js"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.path().join(".codex")).unwrap();
    std::fs::write(
        f.repo.path().join(".codex/config.toml"),
        "[mcp_servers.keep_codex]\ncommand = \"node\"\nargs = [\"e.js\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(f.home.path().join(".codeium/windsurf")).unwrap();
    std::fs::write(
        f.home.path().join(".codeium/windsurf/mcp_config.json"),
        r#"{"mcpServers":{"keep_windsurf":{"command":"node","args":["f.js"]}}}"#,
    )
    .unwrap();

    // First install without --global-editor-config: Windsurf global file must remain untouched.
    let o = f.run(&["init", "--editor", "windsurf"]);
    assert!(o.status.success(), "{}", message(&o));
    let ws_before =
        std::fs::read_to_string(f.home.path().join(".codeium/windsurf/mcp_config.json")).unwrap();
    assert!(
        !ws_before.contains("gitcortex"),
        "windsurf must not write global config without --global-editor-config"
    );

    // Install all 6 with --global-editor-config twice (checking idempotence).
    for _ in 0..2 {
        let o = f.run(&["init", "--editor", "all", "--global-editor-config"]);
        assert!(o.status.success(), "{}", message(&o));
    }

    let json_checks = [
        (
            f.repo.path().join(".mcp.json"),
            "/mcpServers/keep_claude",
            "/mcpServers/gitcortex",
        ),
        (
            f.repo.path().join(".cursor/mcp.json"),
            "/mcpServers/keep_cursor",
            "/mcpServers/gitcortex",
        ),
        (
            f.repo.path().join(".vscode/mcp.json"),
            "/servers/keep_copilot",
            "/servers/gitcortex",
        ),
        (
            f.repo
                .path()
                .join(".agents/plugins/gitcortex/mcp_config.json"),
            "/mcpServers/keep_agy",
            "/mcpServers/gitcortex",
        ),
        (
            f.home.path().join(".codeium/windsurf/mcp_config.json"),
            "/mcpServers/keep_windsurf",
            "/mcpServers/gitcortex",
        ),
    ];
    for (path, keep_ptr, gcx_ptr) in json_checks {
        let text = std::fs::read_to_string(&path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            val.pointer(keep_ptr).is_some(),
            "lost unrelated entry {keep_ptr} in {}",
            path.display()
        );
        let gcx = val
            .pointer(gcx_ptr)
            .unwrap_or_else(|| panic!("missing {gcx_ptr} in {}", path.display()));
        assert_eq!(gcx["command"], "gcx");
        assert_eq!(gcx["args"], serde_json::json!(["serve"]));
    }

    let codex_text = std::fs::read_to_string(f.repo.path().join(".codex/config.toml")).unwrap();
    assert!(codex_text.contains("[mcp_servers.keep_codex]"));
    assert_eq!(
        codex_text.matches("[mcp_servers.gitcortex]").count(),
        1,
        "codex config must be idempotent:\n{codex_text}"
    );
}
