use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::guide;
use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub(crate) const MARKER: &str = "windsurf integration";

pub(crate) fn guidance() -> String {
    guide::managed_block(MARKER, "")
}

pub fn install(repo_root: &Path, global_editor_config: bool) -> Result<()> {
    write_windsurf_rules(repo_root)?;
    if global_editor_config {
        write_windsurf_mcp()?;
    }
    Ok(())
}

fn write_windsurf_rules(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".windsurfrules");
    guide::upsert_block(&path, MARKER, &guidance()).context("update .windsurfrules")?;
    Ok(())
}

fn write_windsurf_mcp() -> Result<()> {
    let dir = home_dir().join(".codeium").join("windsurf");
    fs::create_dir_all(&dir)?;
    let path = dir.join("mcp_config.json");

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).context("read windsurf mcp_config.json")?;
        serde_json::from_str::<serde_json::Value>(&text)
            .context("parse existing windsurf mcp_config.json")?
    } else {
        json!({})
    };

    require_json_object(&root, &path)?;
    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text = serde_json::to_string_pretty(&root).context("serialize windsurf mcp_config.json")?;
    write_atomic(&path, &text).context("write windsurf mcp_config.json")?;
    Ok(())
}
