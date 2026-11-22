use std::path::{Path, PathBuf};

use anyhow::Result;
use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;

use super::{init::universal::git_hooks_dir, serve_lock};

pub fn run() -> Result<()> {
    eprintln!("gcx doctor\n");

    let mut all_ok = true;

    // 1. Binary version
    let exe = std::env::current_exe().ok();
    let exe_display = exe.as_deref().and_then(|p| p.to_str()).unwrap_or("unknown");
    ok(&format!(
        "gcx v{} on PATH ({})",
        env!("CARGO_PKG_VERSION"),
        exe_display
    ));

    // 2. Git repository
    let repo_root = match find_repo_root() {
        Some(r) => {
            ok("git repository detected");
            r
        }
        None => {
            fail(
                "not inside a git repository",
                "cd into a git repo first",
                &mut all_ok,
            );
            return finish(all_ok);
        }
    };

    // 3. Git hooks
    match git_hooks_dir(&repo_root) {
        Ok(hooks_dir) => {
            for hook in &["post-commit", "post-merge", "post-rewrite", "post-checkout"] {
                check_hook(&hooks_dir, hook, &mut all_ok);
            }
        }
        Err(error) => fail(
            &format!("could not resolve Git hooks directory: {error}"),
            "run: git rev-parse --git-path hooks",
            &mut all_ok,
        ),
    }

    // 4. Graph store. An active MCP server intentionally owns Kuzu's
    // process-exclusive lock and performs Git synchronization itself.
    let server_active = required(
        serve_lock::is_active(&repo_root),
        "could not inspect MCP server ownership",
        "stop active gcx processes and rerun gcx doctor",
        &mut all_ok,
    )
    .unwrap_or(false);
    if server_active {
        ok("graph store owned by active MCP server");
        let readiness = tokio::runtime::Runtime::new()
            .map_err(anyhow::Error::from)
            .and_then(|runtime| {
                runtime.block_on(gitcortex_mcp::mcp::server::daemon_readiness(&repo_root))
            });
        match readiness {
            Ok(index) if index["state"] == "ready" => {
                ok("active MCP server verified index readiness")
            }
            Ok(index) => fail(
                &format!(
                    "active MCP index is {}",
                    index["state"].as_str().unwrap_or("unverified")
                ),
                index["next_action"]
                    .as_str()
                    .unwrap_or("inspect source and restart gcx serve"),
                &mut all_ok,
            ),
            Err(error) => fail(
                &format!("could not verify active MCP freshness: {error}"),
                "restart gcx serve and rerun gcx doctor",
                &mut all_ok,
            ),
        }
    } else {
        match KuzuGraphStore::open(&repo_root) {
            Ok(store) => {
                // Persistent overlays survive daemon shutdown. A matching
                // commit SHA alone does not certify the graph's contents.
                let scope = current_branch(&repo_root).map(|branch| {
                    gitcortex_mcp::mcp::readiness::inspect(&repo_root, &store, &branch)
                });
                match scope {
                    Ok(index) if index.state != "ready" => fail(
                        &format!("index readiness is {}", index.state),
                        index.next_action,
                        &mut all_ok,
                    ),
                    Err(error) => fail(
                        &format!("could not verify index scope: {error}"),
                        "inspect Git HEAD and rerun gcx doctor",
                        &mut all_ok,
                    ),
                    _ => {}
                }
                let Some(branch) = required(
                    current_branch(&repo_root),
                    "could not resolve current branch",
                    "run: git symbolic-ref --short HEAD or git rev-parse --short HEAD",
                    &mut all_ok,
                ) else {
                    eprintln!();
                    return finish(all_ok);
                };
                let node_count = required(
                    store.list_all_nodes(&branch).map(|nodes| nodes.len()),
                    "could not read graph nodes",
                    "run: gcx clean && gcx init",
                    &mut all_ok,
                );
                let edge_count = required(
                    store.list_all_edges(&branch).map(|edges| edges.len()),
                    "could not read graph edges",
                    "run: gcx clean && gcx init",
                    &mut all_ok,
                );
                if let (Some(node_count), Some(edge_count)) = (node_count, edge_count) {
                    ok(&format!(
                        "graph store accessible  ({node_count} nodes, {edge_count} edges on {branch})"
                    ));
                }

                // 5. Index freshness
                match (store.last_indexed_sha(&branch), head_sha(&repo_root)) {
                    (Ok(Some(indexed)), Ok(head)) if indexed == head => {
                        ok(&format!(
                            "indexed commit matches HEAD ({}) — working-tree edits checked separately",
                            &head[..7.min(head.len())]
                        ));
                    }
                    (Ok(Some(indexed)), Ok(head)) => {
                        let msg = format!(
                            "index is stale  (indexed {} → HEAD {})",
                            &indexed[..7.min(indexed.len())],
                            &head[..7.min(head.len())]
                        );
                        fail(
                            &msg,
                            "run: git commit --allow-empty  or  gcx hook",
                            &mut all_ok,
                        );
                    }
                    (Ok(None), _) => {
                        fail(
                            "no index found for this branch",
                            "run: gcx init",
                            &mut all_ok,
                        );
                    }
                    (Err(error), _) => fail(
                        &format!("could not read indexed revision: {error}"),
                        "run: gcx clean && gcx init",
                        &mut all_ok,
                    ),
                    (_, Err(error)) => fail(
                        &format!("could not read Git HEAD: {error}"),
                        "run: git rev-parse HEAD",
                        &mut all_ok,
                    ),
                }
            }
            Err(e) => {
                fail(
                    &format!("graph store not accessible: {e}"),
                    "run: gcx init",
                    &mut all_ok,
                );
            }
        }
    }

    // 6. WSL detection (Linux only — silently skipped everywhere else)
    check_wsl();

    // 7. Assistant/editor registrations
    check_editor_mcp(&repo_root);

    eprintln!();
    finish(all_ok)
}

fn check_wsl() {
    #[cfg(target_os = "linux")]
    {
        if let Ok(version) = std::fs::read_to_string("/proc/version") {
            if version.to_ascii_lowercase().contains("microsoft") {
                info("running inside WSL2 — native Windows not supported; WSL2 is the recommended path");
            }
        }
    }
}

fn check_hook(hooks_dir: &Path, hook: &str, all_ok: &mut bool) {
    let hook_path = hooks_dir.join(hook);
    if hook_path.exists() {
        let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if content.contains("gcx hook") {
            ok(&format!("{hook} hook installed"));
        } else {
            fail(
                &format!("{hook} hook exists but doesn't call gcx"),
                "run: gcx init",
                all_ok,
            );
        }
    } else {
        fail(&format!("{hook} hook missing"), "run: gcx init", all_ok);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorRegistration {
    path: PathBuf,
    command: String,
    args: Vec<String>,
}

fn is_gcx_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let file_name = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed);
    file_name.eq_ignore_ascii_case("gcx") || file_name.eq_ignore_ascii_case("gcx.exe")
}

fn is_serve_args(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("serve")
}

fn inspect_editor_config(
    path: &Path,
    editor: &super::init::editors::EditorKind,
) -> Result<Option<EditorRegistration>> {
    use super::init::editors::EditorKind;
    use anyhow::Context;

    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    match editor {
        EditorKind::Codex => {
            let value: toml::Value = toml::from_str(&text)
                .with_context(|| format!("parse TOML config {}", path.display()))?;
            let Some(entry) = value
                .get("mcp_servers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("gitcortex"))
                .and_then(|v| v.as_table())
            else {
                return Ok(None);
            };
            let Some(command) = entry.get("command").and_then(|v| v.as_str()) else {
                return Ok(None);
            };
            let Some(raw_args) = entry.get("args").and_then(|v| v.as_array()) else {
                return Ok(None);
            };
            let mut args = Vec::with_capacity(raw_args.len());
            for item in raw_args {
                let Some(s) = item.as_str() else {
                    return Ok(None);
                };
                args.push(s.to_owned());
            }
            if !is_gcx_command(command) || !is_serve_args(&args) {
                return Ok(None);
            }
            Ok(Some(EditorRegistration {
                path: path.to_path_buf(),
                command: command.to_owned(),
                args,
            }))
        }
        EditorKind::ClaudeCode
        | EditorKind::Cursor
        | EditorKind::Windsurf
        | EditorKind::Copilot
        | EditorKind::Antigravity => {
            let value: serde_json::Value = serde_json::from_str(&text)
                .with_context(|| format!("parse JSON config {}", path.display()))?;
            if !value.is_object() {
                anyhow::bail!("{} must contain a JSON object", path.display());
            }
            let pointer = match editor {
                EditorKind::Copilot => "/servers/gitcortex",
                _ => "/mcpServers/gitcortex",
            };
            let Some(entry) = value.pointer(pointer).and_then(|v| v.as_object()) else {
                return Ok(None);
            };
            let Some(command) = entry.get("command").and_then(|v| v.as_str()) else {
                return Ok(None);
            };
            let Some(raw_args) = entry.get("args").and_then(|v| v.as_array()) else {
                return Ok(None);
            };
            let mut args = Vec::with_capacity(raw_args.len());
            for item in raw_args {
                let Some(s) = item.as_str() else {
                    return Ok(None);
                };
                args.push(s.to_owned());
            }
            if !is_gcx_command(command) || !is_serve_args(&args) {
                return Ok(None);
            }
            Ok(Some(EditorRegistration {
                path: path.to_path_buf(),
                command: command.to_owned(),
                args,
            }))
        }
    }
}

fn editor_candidate_paths(
    repo_root: &Path,
    home: Option<&Path>,
    editor: &super::init::editors::EditorKind,
) -> Vec<(&'static str, PathBuf)> {
    use super::init::editors::EditorKind;

    let mut paths = Vec::new();
    match editor {
        EditorKind::ClaudeCode => {
            paths.push(("local", repo_root.join(".mcp.json")));
            if let Some(h) = home {
                paths.push(("global", h.join(".claude.json")));
            }
        }
        EditorKind::Cursor => {
            paths.push(("local", repo_root.join(".cursor").join("mcp.json")));
        }
        EditorKind::Windsurf => {
            if let Some(h) = home {
                paths.push((
                    "global",
                    h.join(".codeium").join("windsurf").join("mcp_config.json"),
                ));
            }
        }
        EditorKind::Copilot => {
            paths.push(("local", repo_root.join(".vscode").join("mcp.json")));
        }
        EditorKind::Antigravity => {
            paths.push((
                "local",
                repo_root
                    .join(".agents")
                    .join("plugins")
                    .join("gitcortex")
                    .join("mcp_config.json"),
            ));
            if let Some(h) = home {
                paths.push(("global", h.join(".antigravity").join("mcp.json")));
            }
        }
        EditorKind::Codex => {
            paths.push(("local", repo_root.join(".codex").join("config.toml")));
        }
    }
    paths
}

fn check_editor_mcp(repo_root: &Path) {
    use super::init::editors::{editor_flag, EditorKind};

    let home = dirs_home();
    let mut any_registered = false;

    for editor in EditorKind::all() {
        let candidates = editor_candidate_paths(repo_root, home.as_deref(), &editor);
        let mut found: Option<(&'static str, EditorRegistration)> = None;
        let mut parse_errors: Vec<(PathBuf, anyhow::Error)> = Vec::new();

        for (scope, path) in candidates {
            match inspect_editor_config(&path, &editor) {
                Ok(Some(reg)) => {
                    found = Some((scope, reg));
                    break;
                }
                Ok(None) => {}
                Err(err) => {
                    parse_errors.push((path, err));
                }
            }
        }

        if let Some((scope, reg)) = found {
            ok(&format!(
                "{} registration found ({scope}, {}: {} {} — reload editor; registration alone does not confirm an active connection)",
                editor.display_name(),
                reg.path.display(),
                reg.command,
                reg.args.join(" "),
            ));
            any_registered = true;
        } else {
            for (path, err) in parse_errors {
                info(&format!(
                    "{} config malformed at {} ({err})",
                    editor.display_name(),
                    path.display()
                ));
            }
            let token = editor_flag(&editor);
            let hint = if matches!(editor, EditorKind::Windsurf) {
                format!("gcx init --editor {token} --global-editor-config")
            } else {
                format!("gcx init --editor {token}")
            };
            info(&format!(
                "assistant not configured for {}  (run: {hint})",
                editor.display_name()
            ));
        }
    }

    if !any_registered {
        info(
            "no assistant registered (optional; accepted --editor tokens: claude, cursor, windsurf, copilot, antigravity, codex, all)",
        );
    }
}

fn ok(msg: &str) {
    eprintln!("  [ok] {msg}");
}

fn fail(msg: &str, fix: &str, all_ok: &mut bool) {
    eprintln!("  [FAIL] {msg}");
    eprintln!("         → {fix}");
    *all_ok = false;
}

fn required<T, E: std::fmt::Display>(
    result: std::result::Result<T, E>,
    label: &str,
    fix: &str,
    all_ok: &mut bool,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            fail(&format!("{label}: {error}"), fix, all_ok);
            None
        }
    }
}

fn info(msg: &str) {
    eprintln!("  [--] {msg}");
}

fn print_summary(all_ok: bool) {
    if all_ok {
        eprintln!("All checks passed.");
    } else {
        eprintln!("Setup issues found — see above for fixes.");
    }
}

fn finish(all_ok: bool) -> Result<()> {
    print_summary(all_ok);
    if all_ok {
        Ok(())
    } else {
        anyhow::bail!("gcx doctor found setup issues")
    }
}

fn find_repo_root() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8(output.stdout).ok()?;
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    } else {
        None
    }
}

fn current_branch(repo_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    } else {
        let sha = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(repo_root)
            .output()?;
        if !sha.status.success() {
            anyhow::bail!("git rev-parse --short HEAD failed");
        }
        let value = String::from_utf8(sha.stdout)?.trim().to_owned();
        if value.is_empty() {
            anyhow::bail!("git rev-parse --short HEAD returned no revision");
        }
        Ok(value)
    }
}

fn head_sha(repo_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("git rev-parse HEAD failed");
    }
    let value = String::from_utf8(output.stdout)?.trim().to_owned();
    if value.is_empty() {
        anyhow::bail!("git rev-parse HEAD returned no revision");
    }
    Ok(value)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::init::{
        detect::parse_editor_flag,
        editors::{editor_flag, EditorKind},
    };

    #[test]
    fn required_error_marks_doctor_unhealthy() {
        let mut all_ok = true;
        let result: std::result::Result<usize, &str> = Err("store read failed");
        assert!(required(result, "read graph", "run: gcx init", &mut all_ok).is_none());
        assert!(!all_ok);
    }

    #[test]
    fn git_revision_resolution_fails_outside_repository() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(current_branch(temp.path()).is_err());
        assert!(head_sha(temp.path()).is_err());
    }

    #[test]
    fn editor_flags_round_trip_through_parser() {
        for editor in EditorKind::all() {
            let parsed = parse_editor_flag(editor_flag(&editor)).expect("canonical flag parses");
            assert_eq!(parsed, vec![editor]);
        }
    }

    #[test]
    fn absolute_gcx_command_path_is_accepted() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".mcp.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"gitcortex":{"command":"/usr/local/bin/gcx","args":["serve"]}}}"#,
        )
        .unwrap();
        let reg = inspect_editor_config(&path, &EditorKind::ClaudeCode)
            .expect("valid JSON")
            .expect("registration found");
        assert_eq!(reg.command, "/usr/local/bin/gcx");
        assert_eq!(reg.args, vec!["serve"]);
    }
}
