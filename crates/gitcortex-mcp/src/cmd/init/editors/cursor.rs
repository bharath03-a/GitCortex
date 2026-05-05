use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

const CURSOR_RULES: &str = r#"---
description: GitCortex knowledge graph — use these MCP tools to navigate the codebase
globs: ["**/*"]
alwaysApply: true
---

# GitCortex Agent Guide

This repository is indexed by GitCortex. The `gitcortex` MCP server is registered
in `.cursor/mcp.json`. Use these tools instead of grep or file search for structural
questions.

## Key Tools

| Tool | When to use |
|------|-------------|
| `lookup_symbol` | Find any function, struct, class, or trait by name |
| `find_callers` | Who calls this function? (backward trace) |
| `find_callees` | What does this function call? (forward trace) |
| `list_definitions` | All symbols in a file — faster than reading the whole file |
| `find_implementors` | All implementations of a trait or interface |
| `trace_path` | Is there a call path from A to B? |
| `find_unused_symbols` | Dead code candidates |
| `get_subgraph` | Everything within N hops of a symbol |

## Workflows

**Navigating unfamiliar code**: `lookup_symbol` → `list_definitions` → `get_subgraph`

**Debugging a crash**: `lookup_symbol` on the failing function → `find_callers` upstream

**Impact analysis before refactoring**: `find_callers` + `get_subgraph(direction: "in")`

**Finding dead code**: `find_unused_symbols` filtered by kind

See `.gitcortex/AGENT_GUIDE.md` for the full reference.
"#;

pub fn install(repo_root: &Path) -> Result<()> {
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
        serde_json::from_str::<serde_json::Value>(&text).unwrap_or(json!({}))
    } else {
        json!({})
    };

    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text = serde_json::to_string_pretty(&root).context("serialize .cursor/mcp.json")?;
    fs::write(path, text).context("write .cursor/mcp.json")?;
    Ok(())
}
