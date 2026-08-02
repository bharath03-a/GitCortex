use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub(crate) const WINDSURF_RULES: &str = r#"<!-- >>> gitcortex windsurf integration >>> -->
# GitCortex Agent Guide

This repository is indexed by GitCortex. The `gitcortex` MCP server is registered
globally in `~/.codeium/windsurf/mcp_config.json`.

The server exposes one compact `gcx` dispatch tool. Use these values as its
`action` parameter.

## Key actions

| Action | When to use |
|------|-------------|
| `lookup_symbol` | Find any function, struct, class, or trait by name |
| `find_callers` | Who calls this function? (backward trace) |
| `find_callees` | What does this function call? (forward trace) |
| `list_definitions` | All symbols in a file |
| `find_implementors` | All implementations of a trait or interface |
| `trace_path` | Is there a call path from A to B? |
| `find_unused_symbols` | Dead code candidates |
| `get_subgraph` | Everything within N hops of a symbol |

## Workflows

**Navigating unfamiliar code**: `lookup_symbol` → `list_definitions` → `get_subgraph`

**Debugging a crash**: `lookup_symbol` on the failing function → `find_callers` upstream

**Impact analysis**: `find_callers` + `get_subgraph(direction: "in")`

See `.gitcortex/AGENT_GUIDE.md` for the full reference.
<!-- <<< gitcortex windsurf integration <<< -->
"#;

pub fn install(repo_root: &Path, global_editor_config: bool) -> Result<()> {
    write_windsurf_rules(repo_root)?;
    if global_editor_config {
        write_windsurf_mcp()?;
    }
    Ok(())
}

fn write_windsurf_rules(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".windsurfrules");
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex") {
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}\n\n{WINDSURF_RULES}"))
            .context("update .windsurfrules")?;
    } else {
        fs::write(path, WINDSURF_RULES).context("write .windsurfrules")?;
    }
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
