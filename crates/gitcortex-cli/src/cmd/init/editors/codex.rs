use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::cmd::init::guide;
use crate::cmd::init::helpers::write_atomic;
use crate::cmd::init::read_hook;

pub(crate) const MARKER: &str = "codex integration";

pub(crate) fn guidance() -> String {
    guide::managed_block(MARKER, "")
}

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

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_agents_md(repo_root)?;
    write_codex_config(repo_root)?;
    write_hooks_json(repo_root)?;
    Ok(())
}

fn write_agents_md(repo_root: &Path) -> Result<()> {
    let path = repo_root.join("AGENTS.md");
    if let Ok(existing) = fs::read_to_string(&path) {
        if let Some(start) = existing.find("# GitCortex - Codex Guide") {
            let prefix = existing[..start].trim_end();
            write_atomic(&path, &format!("{prefix}\n")).context("migrate AGENTS.md")?;
        }
    }
    guide::upsert_block(&path, MARKER, &guidance()).context("update AGENTS.md")?;
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
    read_hook::install(&hook_path)?;

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
