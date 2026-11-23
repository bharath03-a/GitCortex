use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::guide;
use crate::cmd::init::helpers::{require_json_object, write_atomic};

pub(crate) const MARKER: &str = "copilot integration";

pub(crate) fn guidance() -> String {
    guide::managed_block(MARKER, "")
}

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_vscode_mcp(repo_root)?;
    let path = repo_root.join(".github").join("copilot-instructions.md");
    guide::upsert_block(&path, MARKER, &guidance()).context("update copilot-instructions.md")?;
    Ok(())
}

fn write_vscode_mcp(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".vscode").join("mcp.json");
    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).context("read .vscode/mcp.json")?;
        serde_json::from_str::<serde_json::Value>(&text)
            .context("parse existing .vscode/mcp.json")?
    } else {
        json!({})
    };
    require_json_object(&root, &path)?;
    if root.pointer("/servers/gitcortex").is_none() {
        root["servers"]["gitcortex"] =
            json!({ "type": "stdio", "command": "gcx", "args": ["serve"] });
        write_atomic(&path, &serde_json::to_string_pretty(&root)?)
            .context("write .vscode/mcp.json")?;
    }
    Ok(())
}
