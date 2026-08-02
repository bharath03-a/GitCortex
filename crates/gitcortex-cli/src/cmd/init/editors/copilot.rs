use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::json;

use crate::cmd::init::helpers::{require_json_object, write_atomic};

pub(crate) const COPILOT_INSTRUCTIONS: &str = r#"<!-- >>> gitcortex copilot integration >>> -->
# GitCortex — Repository Knowledge Graph

This repository is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
The knowledge graph is always up to date with the current branch (updated on every git
operation via post-commit/post-merge/post-rewrite/post-checkout hooks).

## MCP actions available through `gcx serve`

The compact server exposes one `gcx` dispatch tool. Use these values as its
`action` when answering structural questions.

| Action | Description |
|------|-------------|
| `lookup_symbol(name)` | Find any function, struct, class, or trait |
| `find_callers(function_name)` | Who calls this? |
| `find_callees(function_name, depth)` | What does this call? |
| `list_definitions(file)` | All symbols in a file |
| `find_implementors(trait_name)` | All implementations |
| `trace_path(from, to)` | Call paths from A to B |
| `list_symbols_in_range(file, start, end)` | Symbols in a line range |
| `find_unused_symbols(branch)` | Dead code candidates |
| `get_subgraph(seed_name, depth, direction)` | Neighbourhood of a symbol |

## Suggested Workflows

- **Understand a module**: `list_definitions` then `get_subgraph` on key types
- **Track a bug**: `lookup_symbol` → `find_callers` walking upstream
- **Pre-refactor impact**: `find_callers` + `get_subgraph(direction: "in")`
- **Clean up**: `find_unused_symbols` filtered by kind

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
