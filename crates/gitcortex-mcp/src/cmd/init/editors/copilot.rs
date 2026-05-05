use std::{fs, path::Path};

use anyhow::{Context, Result};

const COPILOT_INSTRUCTIONS: &str = r#"# GitCortex — Repository Knowledge Graph

This repository is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
The knowledge graph is always up to date with the current branch (updated on every git
operation via post-commit/post-merge/post-rewrite/post-checkout hooks).

## MCP Tools Available (via `gcx serve`)

Use these tools when answering questions about code structure, call chains, or
dependencies. They read the parsed knowledge graph — not grep output.

| Tool | Description |
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
"#;

pub fn install(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".github");
    fs::create_dir_all(&dir)?;
    let path = dir.join("copilot-instructions.md");

    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex") {
            return Ok(());
        }
        fs::write(path, format!("{existing}\n\n{COPILOT_INSTRUCTIONS}"))
            .context("update copilot-instructions.md")?;
    } else {
        fs::write(path, COPILOT_INSTRUCTIONS).context("write copilot-instructions.md")?;
    }
    Ok(())
}
