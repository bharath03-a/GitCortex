use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::home_dir;

pub fn install(_repo_root: &Path) -> Result<()> {
    write_antigravity_mcp()?;
    Ok(())
}

fn write_antigravity_mcp() -> Result<()> {
    let dir = home_dir().join(".antigravity");
    fs::create_dir_all(&dir)?;
    let path = dir.join("mcp.json");

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).context("read ~/.antigravity/mcp.json")?;
        serde_json::from_str::<serde_json::Value>(&text).unwrap_or(json!({}))
    } else {
        json!({})
    };

    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text =
        serde_json::to_string_pretty(&root).context("serialize ~/.antigravity/mcp.json")?;
    fs::write(path, text).context("write ~/.antigravity/mcp.json")?;
    Ok(())
}
