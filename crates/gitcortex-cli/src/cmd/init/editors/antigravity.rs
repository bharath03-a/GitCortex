use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use super::claude::SKILLS;
use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub(crate) const AGENTS_MD_SECTION: &str = r#"<!-- >>> gitcortex antigravity integration >>> -->
## GitCortex knowledge graph

This repository is indexed by GitCortex. Run these directly in a terminal,
or via the `gitcortex` MCP server's compact `gcx` dispatch tool if that's
configured (`~/.gemini/config/mcp_config.json`):

- `gcx query lookup-symbol <name>` locates a definition.
- `gcx query find-callers <name>` / `gcx query find-callees <name>` trace call relationships.
- `gcx query get-subgraph <name>` maps a symbol neighbourhood.
- `gcx query tour` summarizes unfamiliar areas.
- `gcx blast-radius --base <branch> --head <branch>` assesses change impact.

Treat graph output as navigation evidence and confirm behavior in source and
tests before editing. If the index is stale, run `gcx hook`.
<!-- <<< gitcortex antigravity integration <<< -->
"#;

const HOOKS_JSON_TEMPLATE: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Read",
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

pub fn install(repo_root: &Path, global_editor_config: bool) -> Result<()> {
    write_agents_md(repo_root)?;
    write_skills(repo_root)?;
    write_hooks_json(repo_root)?;
    if global_editor_config {
        write_mcp_config(&home_dir().join(".antigravity").join("mcp.json"))?;
        // The `agy` CLI is a separate product from the Antigravity IDE and
        // reads its own config, not ~/.antigravity/mcp.json.
        write_mcp_config(
            &home_dir()
                .join(".gemini")
                .join("config")
                .join("mcp_config.json"),
        )?;
    }
    Ok(())
}

/// Antigravity's project customization root is `.agents/`; rules live under
/// `.agents/rules/` as `AGENTS.md` (or standalone `GEMINI.md`/`AGENTS.md` at
/// the root — the binary's own docs recommend consolidating into
/// `rules/AGENTS.md`, matched here).
fn write_agents_md(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".agents").join("rules");
    fs::create_dir_all(&dir)?;
    let path = dir.join("AGENTS.md");

    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex knowledge graph") {
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}{AGENTS_MD_SECTION}"))
            .context("update .agents/rules/AGENTS.md")?;
    } else {
        fs::write(&path, AGENTS_MD_SECTION.trim_start())
            .context("write .agents/rules/AGENTS.md")?;
    }
    Ok(())
}

/// Antigravity skills share Claude Code's `SKILL.md` shape — same content,
/// different root (`.agents/skills/<name>/SKILL.md` vs `.claude/skills/...`).
fn write_skills(repo_root: &Path) -> Result<usize> {
    let skills_dir = repo_root.join(".agents").join("skills");
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

fn write_hooks_json(repo_root: &Path) -> Result<()> {
    let hook_dir = repo_root.join(".agents").join("hooks");
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

    let path = repo_root.join(".agents").join("hooks.json");
    if path.exists() {
        // A hooks.json already exists — don't clobber it; leave it for the
        // user to merge manually rather than guess at preserving arbitrary
        // existing hook structure.
        return Ok(());
    }
    let content =
        HOOKS_JSON_TEMPLATE.replace("COMMAND_PLACEHOLDER", ".agents/hooks/gcx-context.sh");
    fs::write(&path, content).context("write .agents/hooks.json")?;
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
    use super::{write_agents_md, write_hooks_json, write_mcp_config, write_skills};

    #[test]
    fn writes_agents_md_under_agents_rules() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_md(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".agents/rules/AGENTS.md")).unwrap();
        assert!(content.contains("GitCortex knowledge graph"));
    }

    #[test]
    fn agents_md_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_md(dir.path()).unwrap();
        let first = std::fs::read_to_string(dir.path().join(".agents/rules/AGENTS.md")).unwrap();
        write_agents_md(dir.path()).unwrap();
        let second = std::fs::read_to_string(dir.path().join(".agents/rules/AGENTS.md")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn writes_skills_as_skill_md_per_directory() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_skills(dir.path()).unwrap();
        assert_eq!(written, 4);
        let path = dir.path().join(".agents/skills/exploring/SKILL.md");
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

        let hooks_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".agents/hooks.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            hooks_json["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            ".agents/hooks/gcx-context.sh"
        );
        assert!(dir.path().join(".agents/hooks/gcx-context.sh").exists());
    }

    #[test]
    fn does_not_clobber_existing_hooks_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".agents")).unwrap();
        std::fs::write(
            dir.path().join(".agents/hooks.json"),
            r#"{"hooks":{"other":true}}"#,
        )
        .unwrap();

        write_hooks_json(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".agents/hooks.json")).unwrap();
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
}
