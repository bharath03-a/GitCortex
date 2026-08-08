use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub fn install(_repo_root: &Path, global_editor_config: bool) -> Result<()> {
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
