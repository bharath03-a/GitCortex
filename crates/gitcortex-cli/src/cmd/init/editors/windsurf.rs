use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub(crate) const WINDSURF_RULES: &str = r#"<!-- >>> gitcortex windsurf integration >>> -->
# GitCortex Agent Guide

This repository is indexed by GitCortex. Run these directly in a terminal; the
`gitcortex` MCP server registered globally in `~/.codeium/windsurf/mcp_config.json`
exposes the same actions through its one compact `gcx` dispatch tool, if you
prefer that path.

## Key commands

| CLI command | MCP action | When to use |
|------|------|-------------|
| `gcx query lookup-symbol <name>` | `lookup_symbol` | Find any function, struct, class, or trait by name |
| `gcx query find-callers <name>` | `find_callers` | Who calls this function? (backward trace) |
| `gcx query find-callees <name>` | `find_callees` | What does this function call? (forward trace) |
| `gcx query list-definitions <file>` | `list_definitions` | All symbols in a file |
| `gcx query find-implementors <name>` | `find_implementors` | All implementations of a trait or interface |
| `gcx query trace-path <from> <to>` | `trace_path` | Is there a call path from A to B? |
| `gcx query find-unused` | `find_unused_symbols` | Dead code candidates |
| `gcx query get-subgraph <name>` | `get_subgraph` | Everything within N hops of a symbol |

## Workflows

**Navigating unfamiliar code**: `lookup-symbol` → `list-definitions` → `get-subgraph`

**Debugging a crash**: `lookup-symbol` on the failing function → `find-callers` upstream

**Impact analysis**: `find-callers` then `gcx blast-radius --base main --head HEAD`

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
