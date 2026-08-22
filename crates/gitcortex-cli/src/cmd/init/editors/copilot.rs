use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{require_json_object, write_atomic};

pub(crate) const COPILOT_INSTRUCTIONS: &str = r#"<!-- >>> gitcortex copilot integration >>> -->
# GitCortex — Repository Knowledge Graph

This repository is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
The knowledge graph is always up to date with the current branch (updated on every git
operation via post-commit/post-merge/post-rewrite/post-checkout hooks).

## Commands

Run these directly in a terminal, or via the `gcx serve` MCP server's compact
`gcx` dispatch tool if that's configured.

| CLI command | MCP action | Description |
|------|------|-------------|
| `gcx query lookup-symbol <name>` | `lookup_symbol(name)` | Find any function, struct, class, or trait |
| `gcx query find-callers <name>` | `find_callers(function_name)` | Who calls this? |
| `gcx query find-callees <name>` | `find_callees(function_name, depth)` | What does this call? |
| `gcx query list-definitions <file>` | `list_definitions(file)` | All symbols in a file |
| `gcx query find-implementors <name>` | `find_implementors(trait_name)` | All implementations |
| `gcx query trace-path <from> <to>` | `trace_path(from, to)` | Call paths from A to B |
| — (MCP only) | `list_symbols_in_range(file, start, end)` | Symbols in a line range |
| `gcx query find-unused` | `find_unused_symbols(branch)` | Dead code candidates |
| `gcx query get-subgraph <name>` | `get_subgraph(seed_name, depth, direction)` | Neighbourhood of a symbol |

## Suggested Workflows

- **Understand a module**: `list-definitions` then `get-subgraph` on key types
- **Track a bug**: `lookup-symbol` → `find-callers` walking upstream
- **Pre-refactor impact**: `find-callers` then `gcx blast-radius --base main --head HEAD`
- **Clean up**: `find-unused` filtered by kind

See `.gitcortex/AGENT_GUIDE.md` for the full guide.
<!-- <<< gitcortex copilot integration <<< -->
"#;

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_vscode_mcp(repo_root)?;
    let dir = repo_root.join(".github");
    fs::create_dir_all(&dir)?;
    let path = dir.join("copilot-instructions.md");

    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex") {
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}\n\n{COPILOT_INSTRUCTIONS}"))
            .context("update copilot-instructions.md")?;
    } else {
        fs::write(path, COPILOT_INSTRUCTIONS).context("write copilot-instructions.md")?;
    }
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
