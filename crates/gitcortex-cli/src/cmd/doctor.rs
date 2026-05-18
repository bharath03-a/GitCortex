use std::path::{Path, PathBuf};

use anyhow::Result;
use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;

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
            print_summary(all_ok);
            return Ok(());
        }
    };

    // 3. Git hooks
    for hook in &["post-commit", "post-merge", "post-rewrite", "post-checkout"] {
        check_hook(&repo_root, hook, &mut all_ok);
    }

    // 4. Graph store
    match KuzuGraphStore::open(&repo_root) {
        Ok(store) => {
            let branch = current_branch(&repo_root).unwrap_or_else(|_| "main".into());
            let node_count = store.list_all_nodes(&branch).map(|v| v.len()).unwrap_or(0);
            let edge_count = store.list_all_edges(&branch).map(|v| v.len()).unwrap_or(0);
            ok(&format!(
                "graph store accessible  ({node_count} nodes, {edge_count} edges on {branch})"
            ));

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
                _ => {
                    warn("could not determine index freshness");
                }
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

    // 6. MCP editor registrations
    check_editor_mcp(&repo_root, &mut all_ok);

    eprintln!();
    print_summary(all_ok);
    Ok(())
}

fn check_hook(repo_root: &Path, hook: &str, all_ok: &mut bool) {
    let hook_path = repo_root.join(".git").join("hooks").join(hook);
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

fn check_editor_mcp(repo_root: &Path, all_ok: &mut bool) {
    let home = dirs_home();

    let editors: &[EditorCheck] = &[
        (
            "Claude Code",
            Box::new({
                let home = home.clone();
                move || {
                    home.as_ref()
                        .map(|h| {
                            let p = h.join(".claude.json");
                            p.exists() && {
                                std::fs::read_to_string(&p)
                                    .map(|s| s.contains("gcx"))
                                    .unwrap_or(false)
                            }
                        })
                        .unwrap_or(false)
                }
            }),
        ),
        (
            "Cursor",
            Box::new({
                let root = repo_root.to_path_buf();
                move || root.join(".cursor").join("mcp.json").exists()
            }),
        ),
        (
            "Windsurf",
            Box::new({
                let home = home.clone();
                move || {
                    home.as_ref()
                        .map(|h| {
                            h.join(".codeium")
                                .join("windsurf")
                                .join("mcp_config.json")
                                .exists()
                        })
                        .unwrap_or(false)
                }
            }),
        ),
        (
            "Copilot",
            Box::new({
                let root = repo_root.to_path_buf();
                move || {
                    root.join(".github")
                        .join("copilot-instructions.md")
                        .exists()
                }
            }),
        ),
    ];

    let mut any_registered = false;
    for (name, check) in editors {
        if check() {
            ok(&format!("MCP registered  ({name})"));
            any_registered = true;
        } else {
            info(&format!(
                "MCP not configured for {name}  (run: gcx init --editor {})",
                name.to_ascii_lowercase().replace(' ', "-")
            ));
        }
    }

    if !any_registered {
        fail("MCP not registered for any editor", "run: gcx init", all_ok);
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

fn warn(msg: &str) {
    eprintln!("  [warn] {msg}");
}

fn info(msg: &str) {
    eprintln!("  [--] {msg}");
}

fn print_summary(all_ok: bool) {
    if all_ok {
        eprintln!("All checks passed.");
    } else {
        eprintln!("Some checks failed — see above for fixes.");
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
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}

fn head_sha(repo_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}
