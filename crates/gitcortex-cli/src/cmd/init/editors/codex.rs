use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::cmd::init::helpers::write_atomic;

pub(crate) const AGENTS_SECTION: &str = r#"<!-- >>> gitcortex codex integration >>> -->
## GitCortex knowledge graph

This repository is indexed by GitCortex. For structural questions, prefer these
over broad file scans — run them directly as shell commands, or via the `gcx`
MCP tool if `mcp_servers.gitcortex` is configured:

- `gcx query lookup-symbol <name>` locates a definition.
- `gcx query find-callers <name>` / `gcx query find-callees <name>` trace call relationships.
- `gcx query get-subgraph <name>` maps a symbol neighbourhood.
- `gcx query tour` summarizes unfamiliar areas.
- `gcx blast-radius --base <branch> --head <branch>` assesses change impact.

Treat graph output as navigation evidence and confirm behavior in source and
tests before editing. If the index is stale, run `gcx hook`.
<!-- <<< gitcortex codex integration <<< -->
"#;

pub(crate) const CODEX_MCP_SECTION: &str = r#"# >>> gitcortex codex MCP >>>
[mcp_servers.gitcortex]
command = "gcx"
args = ["serve"]
startup_timeout_sec = 30
# <<< gitcortex codex MCP <<<
"#;

/// Codex's hooks.json shares Claude Code's PreToolUse contract (event name,
/// {"matcher", "hooks": [{"type": "command", "command", ...}]} shape) — see
/// learn.chatgpt.com/codex/hooks. `view_file` is Codex's file-read tool name.
const HOOKS_JSON_TEMPLATE: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "view_file",
        "hooks": [
          {
            "type": "command",
            "command": "COMMAND_PLACEHOLDER"
          }
        ]
      }
    ]
  }
}
"#;

const PRE_TOOL_USE_HOOK: &str = r#"#!/usr/bin/env sh
# GitCortex PreToolUse hook — appends call-graph context when a file is read.
set -e
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

input=$(cat)
file_path=$(printf '%s' "$input" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" \
  2>/dev/null || true)

[ -z "$file_path" ] && exit 0
command -v gcx >/dev/null 2>&1 || exit 0

gcx query context "$file_path" 2>/dev/null || true
"#;

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_agents_md(repo_root)?;
    write_codex_config(repo_root)?;
    write_hooks_json(repo_root)?;
    Ok(())
}

fn write_agents_md(repo_root: &Path) -> Result<()> {
    let path = repo_root.join("AGENTS.md");

    if path.exists() {
        let existing = fs::read_to_string(&path).context("read AGENTS.md")?;
        if existing.contains("gitcortex codex integration") {
            return Ok(());
        }
        if let Some(start) = existing.find("# GitCortex - Codex Guide") {
            let prefix = existing[..start].trim_end();
            let migrated = if prefix.is_empty() {
                AGENTS_SECTION.to_owned()
            } else {
                format!("{prefix}\n\n{AGENTS_SECTION}")
            };
            write_atomic(&path, &migrated).context("migrate AGENTS.md")?;
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}\n\n{AGENTS_SECTION}"))
            .context("update AGENTS.md")?;
    } else {
        fs::write(&path, AGENTS_SECTION).context("write AGENTS.md")?;
    }
    Ok(())
}

fn write_codex_config(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".codex");
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    const CODEX_MCP: &str = CODEX_MCP_SECTION;

    if path.exists() {
        let existing = fs::read_to_string(&path).context("read .codex/config.toml")?;
        if existing.contains("[mcp_servers.gitcortex]") {
            if existing.contains("args = [\"serve\", \"--compact\"]") {
                let migrated =
                    existing.replace("args = [\"serve\", \"--compact\"]", "args = [\"serve\"]");
                write_atomic(&path, &migrated).context("migrate .codex/config.toml")?;
            }
            return Ok(());
        }
        write_atomic(&path, &format!("{}\n\n{CODEX_MCP}", existing.trim_end()))
            .context("update .codex/config.toml")?;
    } else {
        fs::write(&path, CODEX_MCP).context("write .codex/config.toml")?;
    }
    Ok(())
}

fn write_hooks_json(repo_root: &Path) -> Result<()> {
    let hook_dir = repo_root.join(".codex").join("hooks");
    fs::create_dir_all(&hook_dir)?;
    let hook_path = hook_dir.join("gcx-context.sh");
    if !hook_path.exists() {
        fs::write(&hook_path, PRE_TOOL_USE_HOOK).context("write gcx-context.sh")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms)?;
        }
    }

    let path = repo_root.join(".codex").join("hooks.json");
    if path.exists() {
        // A hooks.json already exists — don't clobber it; leave it for the
        // user to merge manually rather than guess at preserving arbitrary
        // existing hook structure.
        return Ok(());
    }
    let content = HOOKS_JSON_TEMPLATE.replace("COMMAND_PLACEHOLDER", ".codex/hooks/gcx-context.sh");
    fs::write(&path, content).context("write .codex/hooks.json")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_hooks_json_with_matching_hook_script() {
        let dir = tempfile::tempdir().unwrap();
        write_hooks_json(dir.path()).unwrap();

        let hooks_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join(".codex/hooks.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            hooks_json["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            ".codex/hooks/gcx-context.sh"
        );
        assert!(dir.path().join(".codex/hooks/gcx-context.sh").exists());
    }

    #[test]
    fn does_not_clobber_existing_hooks_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".codex")).unwrap();
        fs::write(
            dir.path().join(".codex/hooks.json"),
            r#"{"hooks":{"other":true}}"#,
        )
        .unwrap();

        write_hooks_json(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(".codex/hooks.json")).unwrap();
        assert!(content.contains("other"));
    }

    #[test]
    fn migrates_legacy_codex_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("AGENTS.md"),
            "# Existing\n\n# GitCortex - Codex Guide\nlegacy generated body\n",
        )
        .expect("write AGENTS");
        fs::create_dir(temp.path().join(".codex")).expect("create codex dir");
        fs::write(
            temp.path().join(".codex/config.toml"),
            "[mcp_servers.gitcortex]\ncommand = \"gcx\"\nargs = [\"serve\", \"--compact\"]\n",
        )
        .expect("write config");

        install(temp.path(), false).expect("install");
        let agents = fs::read_to_string(temp.path().join("AGENTS.md")).expect("read AGENTS");
        assert!(agents.contains("# Existing"));
        assert!(agents.contains("gitcortex codex integration"));
        assert!(!agents.contains("legacy generated body"));
        let config =
            fs::read_to_string(temp.path().join(".codex/config.toml")).expect("read config");
        assert!(config.contains("args = [\"serve\"]"));
        assert!(!config.contains("--compact"));
    }
}
