use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::Value;

use super::init::{
    editors::{claude, codex, copilot, windsurf},
    helpers::{home_dir, repo_root, write_atomic},
    universal::{ensure_hooks_scope, git_hooks_dir, HOOK_BLOCK_END, HOOK_BLOCK_START},
};
use super::serve_lock;
use gitcortex_store::branch;

pub fn run(
    dry_run: bool,
    purge: bool,
    global_editor_config: bool,
    shared_git_hooks: bool,
) -> Result<()> {
    let repo_root = repo_root()?;
    ensure_hooks_scope(&repo_root, shared_git_hooks || dry_run)?;
    // Dry-run never needs ownership. A real purge holds the lock throughout
    // cleanup so daemon startup cannot race the data deletion.
    let _repository_lock = if purge && !dry_run {
        Some(serve_lock::acquire_mutation(&repo_root)?)
    } else {
        None
    };
    let mut changes = 0usize;

    changes += remove_hooks(&repo_root, dry_run)?;

    for path in [
        ".cursor/rules/gitcortex.mdc",
        ".claude/hooks/pre-tool-use/gcx-context.sh",
    ] {
        changes += remove_file(&repo_root.join(path), dry_run)?;
    }
    for path in [".claude/commands/gcx", ".claude/skills/gcx"] {
        changes += remove_dir(&repo_root.join(path), dry_run)?;
    }

    changes += remove_text_section(
        &repo_root.join(".claude/CLAUDE.md"),
        claude::CLAUDE_MD_SECTION,
        dry_run,
    )?;
    changes += remove_legacy_tail(
        &repo_root.join(".claude/CLAUDE.md"),
        "## GitCortex MCP Tools (prefer over grep for code navigation)",
        dry_run,
    )?;
    changes += remove_text_section(&repo_root.join("AGENTS.md"), codex::AGENTS_SECTION, dry_run)?;
    changes += remove_legacy_tail(
        &repo_root.join("AGENTS.md"),
        "# GitCortex - Codex Guide",
        dry_run,
    )?;
    changes += remove_text_section(
        &repo_root.join(".codex/config.toml"),
        codex::CODEX_MCP_SECTION,
        dry_run,
    )?;
    changes += remove_toml_table(
        &repo_root.join(".codex/config.toml"),
        "mcp_servers.gitcortex",
        dry_run,
    )?;
    changes += remove_text_section(
        &repo_root.join(".windsurfrules"),
        windsurf::WINDSURF_RULES,
        dry_run,
    )?;
    changes += remove_text_section(
        &repo_root.join(".github/copilot-instructions.md"),
        copilot::COPILOT_INSTRUCTIONS,
        dry_run,
    )?;

    changes += remove_mcp_entry(&repo_root.join(".mcp.json"), dry_run)?;
    changes += remove_mcp_entry(&repo_root.join(".cursor/mcp.json"), dry_run)?;
    changes += remove_server_entry(&repo_root.join(".vscode/mcp.json"), "servers", dry_run)?;
    changes += remove_claude_hook(&repo_root.join(".claude/settings.json"), dry_run)?;

    if global_editor_config {
        changes += remove_mcp_entry(&home_dir().join(".claude.json"), dry_run)?;
        changes += remove_mcp_entry(
            &home_dir().join(".codeium/windsurf/mcp_config.json"),
            dry_run,
        )?;
        changes += remove_mcp_entry(&home_dir().join(".antigravity/mcp.json"), dry_run)?;
        changes += remove_mcp_entry(&home_dir().join(".gemini/config/mcp_config.json"), dry_run)?;
    }

    if purge {
        changes += remove_dir(&repo_root.join(".gitcortex"), dry_run)?;
        let repo_id = branch::storage_repo_id(&repo_root);
        let data_dir = branch::data_dir(&repo_id);
        if branch::has_repo_data(&repo_id)? {
            show("remove graph data", &data_dir);
            if !dry_run {
                branch::wipe_repo_data(&repo_id)?;
            }
            changes += 1;
        }
    }

    if changes == 0 {
        println!("GitCortex: nothing to remove");
    } else if dry_run {
        println!("GitCortex: {changes} change(s) planned; no files were modified");
    } else {
        prune_known_dirs(&repo_root);
        println!("GitCortex deinitialised ({changes} change(s))");
        if !purge {
            println!(
                "  Graph data and .gitcortex/ retained; use `gcx deinit --purge` to remove them"
            );
        }
    }
    Ok(())
}

fn remove_hooks(repo_root: &Path, dry_run: bool) -> Result<usize> {
    let hooks_dir = git_hooks_dir(repo_root)?;
    let mut changes = 0;
    for name in ["post-commit", "post-merge", "post-rewrite", "post-checkout"] {
        let path = hooks_dir.join(name);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let updated = strip_managed_block(&content, HOOK_BLOCK_START, HOOK_BLOCK_END)
            .or_else(|| strip_legacy_hook(&content));
        let Some(updated) = updated else {
            continue;
        };
        show("remove hook integration", &path);
        changes += 1;
        if dry_run {
            continue;
        }
        if is_empty_hook(&updated) {
            fs::remove_file(&path)?;
        } else {
            write_atomic(&path, &updated)?;
        }
    }
    Ok(changes)
}

fn strip_managed_block(content: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let start = content.find(start_marker)?;
    let tail = &content[start..];
    let end = start + tail.find(end_marker)? + end_marker.len();
    let mut result = format!("{}{}", &content[..start], &content[end..]);
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    Some(result.trim_end().to_owned() + "\n")
}

fn strip_legacy_hook(content: &str) -> Option<String> {
    if !content
        .lines()
        .any(|line| line.trim_start().starts_with("gcx hook"))
    {
        return None;
    }
    let updated = content
        .lines()
        .filter(|line| !line.trim_start().starts_with("gcx hook"))
        .collect::<Vec<_>>()
        .join("\n");
    Some(updated.trim_end().to_owned() + "\n")
}

fn is_empty_hook(content: &str) -> bool {
    content.lines().all(|line| {
        let line = line.trim();
        line.is_empty()
            || line == "#!/usr/bin/env sh"
            || line == "set -e"
            || line.starts_with("export PATH=\"$HOME/.cargo/bin:")
    })
}

fn remove_text_section(path: &Path, section: &str, dry_run: bool) -> Result<usize> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(0);
    };
    let unmarked = section
        .lines()
        .filter(|line| !line.contains(">>> gitcortex") && !line.contains("<<< gitcortex"))
        .collect::<Vec<_>>()
        .join("\n");
    let matched = [
        section,
        section.trim_start(),
        unmarked.as_str(),
        unmarked.trim_start(),
    ]
    .into_iter()
    .find(|candidate| !candidate.is_empty() && content.contains(candidate));
    let Some(matched) = matched else {
        return Ok(0);
    };
    show("remove generated section", path);
    if !dry_run {
        let updated = content.replace(matched, "");
        if updated.trim().is_empty() {
            fs::remove_file(path)?;
        } else {
            write_atomic(path, updated.trim_end())?;
        }
    }
    Ok(1)
}

fn remove_legacy_tail(path: &Path, heading: &str, dry_run: bool) -> Result<usize> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(0);
    };
    let Some(start) = content.find(heading) else {
        return Ok(0);
    };
    show("remove legacy generated section", path);
    if !dry_run {
        let updated = content[..start].trim_end();
        if updated.is_empty() {
            fs::remove_file(path)?;
        } else {
            write_atomic(path, updated)?;
        }
    }
    Ok(1)
}

fn remove_toml_table(path: &Path, table: &str, dry_run: bool) -> Result<usize> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(0);
    };
    if content.contains(">>> gitcortex codex MCP >>>") {
        return Ok(0);
    }
    let header = format!("[{table}]");
    let Some(start) = content.find(&header) else {
        return Ok(0);
    };
    let after_header = start + header.len();
    let end = content[after_header..]
        .find("\n[")
        .map(|offset| after_header + offset + 1)
        .unwrap_or(content.len());
    show("remove legacy TOML registration", path);
    if !dry_run {
        let updated = format!("{}{}", &content[..start], &content[end..]);
        if updated.trim().is_empty() {
            fs::remove_file(path)?;
        } else {
            write_atomic(path, updated.trim_end())?;
        }
    }
    Ok(1)
}

fn remove_mcp_entry(path: &Path, dry_run: bool) -> Result<usize> {
    remove_server_entry(path, "mcpServers", dry_run)
}

fn remove_server_entry(path: &Path, collection: &str, dry_run: bool) -> Result<usize> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(0);
    };
    let mut root: Value = serde_json::from_str(&content)
        .with_context(|| format!("parse existing {}", path.display()))?;
    let Some(servers) = root.get_mut(collection).and_then(Value::as_object_mut) else {
        return Ok(0);
    };
    if !servers.contains_key("gitcortex") {
        return Ok(0);
    }
    show("remove MCP registration", path);
    if !dry_run {
        servers.remove("gitcortex");
        if servers.is_empty() {
            if let Some(object) = root.as_object_mut() {
                object.remove(collection);
            }
        }
        if root.as_object().is_some_and(serde_json::Map::is_empty) {
            fs::remove_file(path)?;
        } else {
            write_atomic(path, &serde_json::to_string_pretty(&root)?)?;
        }
    }
    Ok(1)
}

fn remove_claude_hook(path: &Path, dry_run: bool) -> Result<usize> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(0);
    };
    let mut root: Value = serde_json::from_str(&content)
        .with_context(|| format!("parse existing {}", path.display()))?;
    let changed = {
        let Some(entries) = root
            .pointer_mut("/hooks/PreToolUse")
            .and_then(Value::as_array_mut)
        else {
            return Ok(0);
        };
        let before = entries.len();
        entries.retain(|entry| !entry.to_string().contains("gcx-context.sh"));
        entries.len() != before
    };
    if !changed {
        return Ok(0);
    }
    show("remove Claude hook registration", path);
    if !dry_run {
        if root
            .pointer("/hooks/PreToolUse")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
                hooks.remove("PreToolUse");
            }
        }
        if root
            .get("hooks")
            .and_then(Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
        {
            if let Some(object) = root.as_object_mut() {
                object.remove("hooks");
            }
        }
        if root.as_object().is_some_and(serde_json::Map::is_empty) {
            fs::remove_file(path)?;
        } else {
            write_atomic(path, &serde_json::to_string_pretty(&root)?)?;
        }
    }
    Ok(1)
}

fn remove_file(path: &Path, dry_run: bool) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    show("remove", path);
    if !dry_run {
        fs::remove_file(path)?;
    }
    Ok(1)
}

fn remove_dir(path: &Path, dry_run: bool) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    show("remove", path);
    if !dry_run {
        fs::remove_dir_all(path)?;
    }
    Ok(1)
}

fn show(action: &str, path: &Path) {
    println!("  {action}: {}", path.display());
}

fn prune_known_dirs(repo_root: &Path) {
    for path in [
        ".claude/hooks/pre-tool-use",
        ".claude/hooks",
        ".claude/commands",
        ".claude/skills",
        ".claude",
        ".cursor/rules",
        ".cursor",
        ".codex",
        ".vscode",
    ] {
        let _ = fs::remove_dir(repo_root.join(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_managed_hook_block() {
        let input = "#!/bin/sh\necho before\n# >>> gitcortex managed hook >>>\ngcx hook\n# <<< gitcortex managed hook <<<\necho after\n";
        let output =
            strip_managed_block(input, HOOK_BLOCK_START, HOOK_BLOCK_END).expect("managed block");
        assert!(output.contains("echo before"));
        assert!(output.contains("echo after"));
        assert!(!output.contains("gcx hook"));
    }

    #[test]
    fn removes_only_gitcortex_from_shared_mcp_json() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.json");
        fs::write(
            &path,
            r#"{"servers":{"other":{"command":"other"},"gitcortex":{"command":"gcx"}}}"#,
        )
        .expect("write fixture");
        assert_eq!(
            remove_server_entry(&path, "servers", false).expect("remove entry"),
            1
        );
        let value: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("read updated configuration"))
                .expect("parse updated configuration");
        assert!(value.pointer("/servers/other").is_some());
        assert!(value.pointer("/servers/gitcortex").is_none());
    }

    #[test]
    fn removes_legacy_hook_and_codex_table_without_touching_neighbors() {
        let legacy = "#!/usr/bin/env sh\nset -e\necho before\ngcx hook\necho after\n";
        let hook = strip_legacy_hook(legacy).expect("legacy hook");
        assert!(hook.contains("echo before"));
        assert!(hook.contains("echo after"));
        assert!(!hook.contains("gcx hook"));

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("config.toml");
        fs::write(
            &path,
            "[before]\nvalue = 1\n\n[mcp_servers.gitcortex]\ncommand = \"gcx\"\nargs = [\"serve\", \"--compact\"]\n\n[after]\nvalue = 2\n",
        )
        .expect("write config");
        assert_eq!(
            remove_toml_table(&path, "mcp_servers.gitcortex", false).expect("remove table"),
            1
        );
        let updated = fs::read_to_string(path).expect("read config");
        assert!(updated.contains("[before]"));
        assert!(updated.contains("[after]"));
        assert!(!updated.contains("mcp_servers.gitcortex"));
    }
}
