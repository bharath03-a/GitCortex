use std::{fs, path::Path};

use anyhow::{Context, Result};

const AGENTS_SECTION: &str = r#"# GitCortex - Codex Guide

## What This Repo Is

GitCortex (`gcx`) is a local-first, branch-aware code knowledge graph for Git repositories. It indexes changed files with tree-sitter, stores graph data in embedded KuzuDB, and exposes graph queries to AI coding assistants through MCP and the `gcx` CLI.

## Workspace Layout

- `crates/gitcortex-core`: shared schema, graph types, and `GraphStore` trait. No I/O, no async.
- `crates/gitcortex-indexer`: sync tree-sitter parsers and incremental indexing.
- `crates/gitcortex-store`: KuzuDB backend implementing `GraphStore`.
- `crates/gitcortex-mcp`: MCP server and async boundary.
- `crates/gitcortex-cli`: `gcx` CLI commands.
- `crates/gitcortex-viz`: Rust-side viz embedding support.
- `viz`: React/Vite graph UI.
- `tests/integration/fixtures`: small language fixtures.
- `.gitcortex`: repo-level GitCortex config and generated context.

## Architecture Rules

- `gitcortex-core` must stay pure: no filesystem, subprocesses, network, or async.
- `gitcortex-indexer` and `gitcortex-store` are sync-only.
- `tokio` belongs only in `gitcortex-mcp` and CLI/server boundaries.
- Store behavior goes through the `GraphStore` trait; do not make indexer or MCP depend on concrete Kuzu details unless that layer owns the concern.
- Library code in `core`, `indexer`, and `store` should not introduce `.unwrap()` or `.expect()`. Propagate errors with the crate error type.
- Hook and incremental-index paths must stay fast. Avoid broad scans, unbounded allocations, and expensive work on every git operation.
- Keep changes surgical. Do not refactor adjacent code unless it is required for the task.

## GitCortex Graph Use

This repo is intended to be indexed by GitCortex. Use the compact GitCortex MCP server first for structural questions; it exposes one dispatch tool (`gcx`) so Codex gets graph access without loading the full tool-schema surface. Read source files after the graph narrows the search.

Useful commands:

```bash
gcx query search <fragment>
gcx query lookup-symbol <name>
gcx query find-callers <name>
gcx query find-callees <name>
gcx query list-definitions <path>
gcx query wiki <name>
gcx query tour --limit 12
gcx blast-radius --base main --head HEAD --format text
```

If graph queries fail with Kuzu schema or lock errors, fall back to `rg` and note that the local graph store likely needs `gcx clean && gcx init`.

## Development Commands

Prefer `just` entry points when available:

```bash
just fmt
just fmt-check
just clippy
just test
just ci
just build
```

Direct equivalents:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd viz && npm run lint
cd viz && npm run test --if-present
cd viz && npm run build
```

If `gitcortex-mcp` build fails because `viz/dist` files are missing, build the frontend first:

```bash
cd viz && npm ci && npm run build
```

## Review Stance

When asked for a review, lead with concrete bugs and risks. Include file and line references, severity, and suggested fixes. Skip broad summaries unless there are no findings.
"#;

pub fn install(repo_root: &Path) -> Result<()> {
    write_agents_md(repo_root)?;
    write_codex_config(repo_root)?;
    Ok(())
}

fn write_agents_md(repo_root: &Path) -> Result<()> {
    let path = repo_root.join("AGENTS.md");

    if path.exists() {
        let existing = fs::read_to_string(&path).context("read AGENTS.md")?;
        if existing.contains("GitCortex - Codex Guide") {
            return Ok(());
        }
        fs::write(&path, format!("{existing}\n\n{AGENTS_SECTION}")).context("update AGENTS.md")?;
    } else {
        fs::write(&path, AGENTS_SECTION).context("write AGENTS.md")?;
    }
    Ok(())
}

fn write_codex_config(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".codex");
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    const CODEX_MCP: &str = r#"[mcp_servers.gitcortex]
command = "gcx"
args = ["serve", "--compact"]
startup_timeout_sec = 30
"#;

    if path.exists() {
        let existing = fs::read_to_string(&path).context("read .codex/config.toml")?;
        if existing.contains("[mcp_servers.gitcortex]") {
            return Ok(());
        }
        fs::write(&path, format!("{}\n\n{CODEX_MCP}", existing.trim_end()))
            .context("update .codex/config.toml")?;
    } else {
        fs::write(&path, CODEX_MCP).context("write .codex/config.toml")?;
    }
    Ok(())
}
