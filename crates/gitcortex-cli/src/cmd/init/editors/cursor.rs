use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{require_json_object, write_atomic};

const CURSOR_RULES: &str = r#"---
description: GitCortex knowledge graph — use the gcx CLI (or its MCP tool) to navigate the codebase
globs: ["**/*"]
alwaysApply: true
---

# GitCortex Agent Guide

This repository is indexed by GitCortex. Run these directly in a terminal; the
`gitcortex` MCP server in `.cursor/mcp.json` exposes the same actions through
its one compact `gcx` dispatch tool, if you prefer that path.

## Key commands

| CLI command | MCP action | When to use |
|------|------|-------------|
| `gcx query lookup-symbol <name>` | `lookup_symbol` | Find any function, struct, class, or trait by name |
| `gcx query find-callers <name>` | `find_callers` | Who calls this function? (backward trace) |
| `gcx query find-callees <name>` | `find_callees` | What does this function call? (forward trace) |
| `gcx query list-definitions <file>` | `list_definitions` | All symbols in a file — faster than reading the whole file |
| `gcx query find-implementors <name>` | `find_implementors` | All implementations of a trait or interface |
| `gcx query trace-path <from> <to>` | `trace_path` | Is there a call path from A to B? |
| `gcx query find-unused` | `find_unused_symbols` | Dead code candidates |
| `gcx query get-subgraph <name>` | `get_subgraph` | Everything within N hops of a symbol |

## Workflows

**Navigating unfamiliar code**: `lookup-symbol` → `list-definitions` → `get-subgraph`

**Debugging a crash**: `lookup-symbol` on the failing function → `find-callers` upstream

**Impact analysis before refactoring**: `find-callers` then `gcx blast-radius --base main --head HEAD`

**Finding dead code**: `find-unused` filtered by kind

See `.gitcortex/AGENT_GUIDE.md` for the full reference.
"#;

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_cursor_rules(repo_root)?;
    write_cursor_mcp(repo_root)?;
    Ok(())
}

fn write_cursor_rules(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".cursor").join("rules");
    fs::create_dir_all(&dir)?;
    let path = dir.join("gitcortex.mdc");
    if !path.exists() {
        fs::write(path, CURSOR_RULES).context("write .cursor/rules/gitcortex.mdc")?;
    }
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
