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
        ok("index freshness managed by active MCP watcher");
    } else {
        match KuzuGraphStore::open(&repo_root) {
            Ok(store) => {
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
                            "index is current  (HEAD {})",
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

type EditorCheck = (&'static str, Box<dyn Fn() -> bool>);

fn check_editor_mcp(repo_root: &Path) {
    let home = dirs_home();

    let editors: &[EditorCheck] = &[
        (
            "Claude Code",
            Box::new({
                let home = home.clone();
                let root = repo_root.to_path_buf();
                move || {
                    file_contains(&root.join(".mcp.json"), "gitcortex")
                        || home
                            .as_ref()
                            .map(|h| {
                                let p = h.join(".claude.json");
                                p.exists()
                                    && std::fs::read_to_string(&p)
                                        .map(|s| s.contains("gcx"))
                                        .unwrap_or(false)
                            })
                            .unwrap_or(false)
                }
            }),
        ),
        (
            "Cursor",
            Box::new({
                let root = repo_root.to_path_buf();
                move || file_contains(&root.join(".cursor/mcp.json"), "gitcortex")
            }),
        ),
        (
            "Windsurf",
            Box::new({
                let home = home.clone();
                move || {
                    home.as_ref()
                        .map(|h| {
                            file_contains(&h.join(".codeium/windsurf/mcp_config.json"), "gitcortex")
                        })
                        .unwrap_or(false)
                }
            }),
        ),
        (
            "Antigravity",
            Box::new({
                let home = home.clone();
                move || {
                    home.as_ref()
                        .map(|h| file_contains(&h.join(".antigravity/mcp.json"), "gitcortex"))
                        .unwrap_or(false)
                }
            }),
        ),
        (
            "Agy CLI",
            Box::new({
                let home = home.clone();
                move || {
                    home.as_ref()
                        .map(|h| {
                            file_contains(&h.join(".gemini/config/mcp_config.json"), "gitcortex")
                        })
                        .unwrap_or(false)
                }
            }),
        ),
        (
            "Copilot",
            Box::new({
                let root = repo_root.to_path_buf();
                move || file_contains(&root.join(".vscode/mcp.json"), "gitcortex")
            }),
        ),
        (
            "Codex",
            Box::new({
                let root = repo_root.to_path_buf();
                move || file_contains(&root.join(".codex/config.toml"), "mcp_servers.gitcortex")
            }),
        ),
    ];

    let mut any_registered = false;
    for (name, check) in editors {
        if check() {
            ok(&format!("assistant configured  ({name})"));
            any_registered = true;
        } else {
            info(&format!(
                "assistant not configured for {name}  (run: gcx init --editor {})",
                name.to_ascii_lowercase().replace(' ', "-")
            ));
        }
    }

    if !any_registered {
        info("assistant not configured (optional; run: gcx init --editor <name>)");
    }
}

fn file_contains(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains(needle))
        .unwrap_or(false)
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
        Some(PathBuf::from(s.trim()))
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
}
