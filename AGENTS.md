# GitCortex - Codex Guide

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
