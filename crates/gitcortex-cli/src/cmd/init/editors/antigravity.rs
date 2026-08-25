use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use super::claude::SKILLS;
use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

/// Antigravity's recommended packaging unit is a "plugin": a namespaced
/// bundle under `<customization-root>/plugins/<name>/` combining rules,
/// skills, hooks, and MCP servers into one deployable unit (confirmed from
/// the `agy` binary's own embedded docs, `strings $(which agy)`, section
/// "Plugins" / "Directory Structure"). Project customization root is
/// `.agents/`, so the plugin lives at `.agents/plugins/gitcortex/`.
const PLUGIN_DIR: &str = "gitcortex";

pub(crate) const AGENTS_MD_SECTION: &str = r#"<!-- >>> gitcortex antigravity integration >>> -->
## GitCortex knowledge graph

This repository is indexed by GitCortex. Run these directly in a terminal,
or via the `gitcortex` MCP server's compact `gcx` dispatch tool, registered
automatically by this plugin:

- `gcx query lookup-symbol <name>` locates a definition.
- `gcx query find-callers <name>` / `gcx query find-callees <name>` trace call relationships.
- `gcx query get-subgraph <name>` maps a symbol neighbourhood.
- `gcx query tour` summarizes unfamiliar areas.
- `gcx blast-radius --base <branch> --head <branch>` assesses change impact.

Treat graph output as navigation evidence and confirm behavior in source and
tests before editing. If the index is stale, run `gcx hook`.
<!-- <<< gitcortex antigravity integration <<< -->
"#;

const PLUGIN_MANIFEST: &str = r#"{
  "name": "gitcortex"
}
"#;

const HOOKS_JSON_TEMPLATE: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Read",
        "hooks": [
          {
            "type": "command",
            "command": "gcx-context.sh"
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

pub fn install(repo_root: &Path, global_editor_config: bool) -> Result<()> {
    let plugin_dir = repo_root.join(".agents").join("plugins").join(PLUGIN_DIR);
    write_plugin_manifest(&plugin_dir)?;
    write_agents_md(&plugin_dir)?;
    write_skills(&plugin_dir)?;
    write_hooks_json(&plugin_dir)?;
    // Project-local, via the plugin's own mcp_config.json — not gated behind
    // --global-editor-config, since it only touches files inside this repo.
    write_mcp_config(&plugin_dir.join("mcp_config.json"))?;
    if global_editor_config {
        write_mcp_config(&home_dir().join(".antigravity").join("mcp.json"))?;
        // The `agy` CLI is a separate product from the Antigravity IDE and
        // reads its own global config, not the plugin's project-local one.
        write_mcp_config(
            &home_dir()
                .join(".gemini")
                .join("config")
                .join("mcp_config.json"),
        )?;
    }
    Ok(())
}

fn write_plugin_manifest(plugin_dir: &Path) -> Result<()> {
    fs::create_dir_all(plugin_dir)?;
    let path = plugin_dir.join("plugin.json");
    if !path.exists() {
        fs::write(&path, PLUGIN_MANIFEST).context("write plugin.json")?;
    }
    Ok(())
}

fn write_agents_md(plugin_dir: &Path) -> Result<()> {
    let dir = plugin_dir.join("rules");
    fs::create_dir_all(&dir)?;
    let path = dir.join("AGENTS.md");

    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex knowledge graph") {
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}{AGENTS_MD_SECTION}"))
            .context("update plugin rules/AGENTS.md")?;
    } else {
        fs::write(&path, AGENTS_MD_SECTION.trim_start()).context("write plugin rules/AGENTS.md")?;
    }
    Ok(())
}

/// Antigravity skills share Claude Code's `SKILL.md` shape — same content,
/// different root (`skills/<name>/SKILL.md` inside the plugin).
fn write_skills(plugin_dir: &Path) -> Result<usize> {
    let skills_dir = plugin_dir.join("skills");
    let mut written = 0;
    for (name, description, body) in SKILLS {
        let dir = skills_dir.join(name);
        fs::create_dir_all(&dir)?;
        let path = dir.join("SKILL.md");
        if !path.exists() {
            let content = format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}");
            fs::write(&path, content).with_context(|| format!("write skill {name}"))?;
            written += 1;
        }
    }
    Ok(written)
}

fn write_hooks_json(plugin_dir: &Path) -> Result<()> {
    let hook_path = plugin_dir.join("gcx-context.sh");
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

    let path = plugin_dir.join("hooks.json");
    if path.exists() {
        // A hooks.json already exists — don't clobber it; leave it for the
        // user to merge manually rather than guess at preserving arbitrary
        // existing hook structure.
        return Ok(());
    }
    fs::write(&path, HOOKS_JSON_TEMPLATE).context("write plugin hooks.json")?;
    Ok(())
}

fn write_mcp_config(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    let mut root = if path.exists() {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        if text.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str::<serde_json::Value>(&text)
                .with_context(|| format!("parse existing {}", path.display()))?
        }
    } else {
        json!({})
    };

    require_json_object(&root, path)?;
    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text = serde_json::to_string_pretty(&root)
        .with_context(|| format!("serialize {}", path.display()))?;
    write_atomic(path, &text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        install, write_agents_md, write_hooks_json, write_mcp_config, write_plugin_manifest,
        write_skills,
    };

    #[test]
    fn writes_plugin_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("plugin");
        write_plugin_manifest(&plugin_dir).unwrap();
        let content = std::fs::read_to_string(plugin_dir.join("plugin.json")).unwrap();
        assert!(content.contains("\"name\": \"gitcortex\""));
    }

    #[test]
    fn writes_agents_md_under_plugin_rules() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_md(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join("rules/AGENTS.md")).unwrap();
        assert!(content.contains("GitCortex knowledge graph"));
    }

    #[test]
    fn agents_md_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_md(dir.path()).unwrap();
        let first = std::fs::read_to_string(dir.path().join("rules/AGENTS.md")).unwrap();
        write_agents_md(dir.path()).unwrap();
        let second = std::fs::read_to_string(dir.path().join("rules/AGENTS.md")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn writes_skills_as_skill_md_per_directory() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_skills(dir.path()).unwrap();
        assert_eq!(written, 4);
        let path = dir.path().join("skills/exploring/SKILL.md");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\nname: exploring\n"));
        assert!(content.contains("description:"));
    }

    #[test]
    fn skills_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_skills(dir.path()).unwrap();
        let second = write_skills(dir.path()).unwrap();
        assert_eq!(second, 0);
    }

    #[test]
    fn writes_valid_hooks_json_with_matching_hook_script() {
        let dir = tempfile::tempdir().unwrap();
        write_hooks_json(dir.path()).unwrap();

        let hooks_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("hooks.json")).unwrap())
                .unwrap();
        assert_eq!(
            hooks_json["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "gcx-context.sh"
        );
        assert!(dir.path().join("gcx-context.sh").exists());
    }

    #[test]
    fn does_not_clobber_existing_hooks_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hooks.json"), r#"{"hooks":{"other":true}}"#).unwrap();

        write_hooks_json(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join("hooks.json")).unwrap();
        assert!(content.contains("other"));
    }

    #[test]
    fn writes_new_agy_style_mcp_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("mcp_config.json");

        write_mcp_config(&path).unwrap();

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["mcpServers"]["gitcortex"]["command"], "gcx");
        assert_eq!(written["mcpServers"]["gitcortex"]["args"][0], "serve");
    }

    #[test]
    fn preserves_existing_servers_when_merging() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"foo"}}}"#).unwrap();

        write_mcp_config(&path).unwrap();

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["mcpServers"]["other"]["command"], "foo");
        assert_eq!(written["mcpServers"]["gitcortex"]["command"], "gcx");
    }

    #[test]
    fn is_idempotent_when_gitcortex_already_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"gitcortex":{"command":"custom"}}}"#,
        )
        .unwrap();

        write_mcp_config(&path).unwrap();

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["mcpServers"]["gitcortex"]["command"], "custom");
    }

    #[test]
    fn install_writes_project_local_mcp_config_without_global_flag() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), false).unwrap();
        let path = dir.path().join(".agents/plugins/gitcortex/mcp_config.json");
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["mcpServers"]["gitcortex"]["command"], "gcx");
    }
}
