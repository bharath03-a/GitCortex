use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub fn install(_repo_root: &Path, global_editor_config: bool) -> Result<()> {
    if global_editor_config {
        write_antigravity_mcp()?;
    }
    Ok(())
}

fn write_antigravity_mcp() -> Result<()> {
    let dir = home_dir().join(".antigravity");
    fs::create_dir_all(&dir)?;
    let path = dir.join("mcp.json");

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).context("read ~/.antigravity/mcp.json")?;
        serde_json::from_str::<serde_json::Value>(&text)
            .context("parse existing ~/.antigravity/mcp.json")?
    } else {
        json!({})
    };

    require_json_object(&root, &path)?;
    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text = serde_json::to_string_pretty(&root).context("serialize ~/.antigravity/mcp.json")?;
    write_atomic(&path, &text).context("write ~/.antigravity/mcp.json")?;
    Ok(())
}
