use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::guide;
use crate::cmd::init::helpers::{require_json_object, write_atomic};

pub(crate) const MARKER: &str = "cursor integration";

const FRONTMATTER: &str = "---\ndescription: GitCortex code graph: callers, impact before edits, cross-file paths\nalwaysApply: true\n---\n\n";

/// Older releases wrote this file whole, without markers.
const LEGACY_HEADER: &str = "description: GitCortex knowledge graph";

pub(crate) fn guidance() -> String {
    guide::managed_block(MARKER, "")
}

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_cursor_rules(repo_root)?;
    write_cursor_mcp(repo_root)?;
    Ok(())
}

fn write_cursor_rules(repo_root: &Path) -> Result<()> {
    let path = repo_root
        .join(".cursor")
        .join("rules")
        .join("gitcortex.mdc");
    let legacy = fs::read_to_string(&path)
        .map(|text| text.contains(LEGACY_HEADER))
        .unwrap_or(false);
    if legacy || !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_atomic(&path, &format!("{FRONTMATTER}{}", guidance()))
            .context("write .cursor/rules/gitcortex.mdc")?;
        return Ok(());
    }
    guide::upsert_block(&path, MARKER, &guidance())
        .context("update .cursor/rules/gitcortex.mdc")?;
    Ok(())
}

fn write_cursor_mcp(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".cursor");
    fs::create_dir_all(&dir)?;
    let path = dir.join("mcp.json");

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).context("read .cursor/mcp.json")?;
        serde_json::from_str::<serde_json::Value>(&text)
            .context("parse existing .cursor/mcp.json")?
    } else {
        json!({})
    };

    require_json_object(&root, &path)?;
    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text = serde_json::to_string_pretty(&root).context("serialize .cursor/mcp.json")?;
    write_atomic(&path, &text).context("write .cursor/mcp.json")?;
    Ok(())
}
