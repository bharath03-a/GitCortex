# Architecture

GitCortex is a local-first, branch-aware code knowledge graph for Git repositories. This document describes how the pieces fit together and where the boundaries are.

## At a glance

```
┌─────────────────────────────────────────────────────────────────┐
│                          User's Editor                          │
│  Claude Code · Cursor · Windsurf · Copilot · Antigravity        │
└───────────────────────────────────┬─────────────────────────────┘
                                    │ MCP (JSON-RPC over stdio)
┌───────────────────────────────────▼─────────────────────────────┐
│                       gcx serve / gcx viz                       │
│                          (binary crate)                         │
│                                                                 │
│  ┌──────────────────┐    ┌──────────────────────────────────┐   │
│  │  stdio proxy     │    │  HTTP viz server (Axum)          │   │
│  │  → local daemon  │    │  /, /assets/*, /data, /api/*     │   │
│  └────────┬─────────┘    └──────────────────┬───────────────┘   │
│           │  many clients / one DB owner    │                   │
│           └───────────────┬─────────────────┘                   │
└──────────────────────────┬┴────────────────────────────────────┘
                           │ GraphStore trait
┌──────────────────────────▼─────────────────────────────────────┐
│                     gitcortex-store                             │
│  KuzuGraphStore — branch-namespaced KuzuDB (embedded)           │
└──────────────────────────▲─────────────────────────────────────┘
                           │ apply_diff(GraphDiff)
┌──────────────────────────┴─────────────────────────────────────┐
│                    gitcortex-indexer                            │
│  tree-sitter parsers · git2 differ · incremental indexing       │
└──────────────────────────▲─────────────────────────────────────┘
                           │ on every git op
┌──────────────────────────┴─────────────────────────────────────┐
│                  git hooks (4 files in hooks/)                  │
│  post-commit · post-merge · post-rewrite · post-checkout        │
└─────────────────────────────────────────────────────────────────┘
```

## Crate layout

| Crate | Role | Async? | Notes |
|---|---|---|---|
| `gitcortex-core` | Shared types (`Node`, `Edge`, `GraphDiff`) and the `GraphStore` trait | **No** | Zero I/O, zero async, zero dependencies on the rest of the workspace |
| `gitcortex-indexer` | tree-sitter parsing + git2 differ + producing `GraphDiff` | **No** | CPU-bound, runs synchronously |
| `gitcortex-store` | `KuzuGraphStore` — local embedded KuzuDB implementation of `GraphStore` | **No** | Blocking calls only; consumers must `spawn_blocking` if invoked from async |
| `gitcortex-mcp` | MCP handlers, repository daemon (`rmcp`), watcher, and semantic index | **Yes** | Async service boundary; one daemon owns each open repository graph |
| `viz/` (under `gitcortex-mcp`) | React + Vite + Cosmograph frontend, embedded via `include_bytes!` | n/a | Built by `build.rs`; output at `crates/gitcortex-mcp/dist-viz/` |

### Dependency graph

```
gitcortex-core   (no internal deps)
      ↑               ↑
gitcortex-indexer   gitcortex-store
      ↑               ↑
             gitcortex-mcp
                  │
                  └──> include_bytes!(viz/dist)
```

There are no cycles. Each crate compiles standalone, and `cargo check -p gitcortex-core` is fast because it doesn't pull KuzuDB's C++ sources.

## The `GraphStore` trait — the extensibility seam

`GraphStore` lives in `gitcortex-core/src/store.rs`. Any new backend (remote, distributed, alternative graph DB) implements this trait. The indexer never touches a concrete store; the MCP layer never touches Cypher. This is the single most important architectural rule in the project.

Key methods:

```rust
pub trait GraphStore: Send + Sync {
    fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()>;
    fn lookup_symbol(&self, branch: &str, name: &str, fuzzy: bool) -> Result<Vec<Node>>;
    fn find_callers(&self, branch: &str, name: &str) -> Result<Vec<Node>>;
    fn find_callers_deep(&self, branch: &str, name: &str, depth: u8) -> Result<CallersDeep>;
    fn find_callees(&self, branch: &str, name: &str, depth: u8) -> Result<CallersDeep>;
    fn find_implementors(&self, branch: &str, name: &str) -> Result<Vec<Node>>;
    fn trace_path(&self, branch: &str, from: &str, to: &str) -> Result<Vec<Node>>;
    fn branch_diff(&self, from: &str, to: &str) -> Result<GraphDiff>;
    fn last_indexed_sha(&self, branch: &str) -> Result<Option<String>>;
    // ... see core/src/store.rs for the full surface
}
```

## Async / sync boundary

> **Rule:** `tokio` stays at the MCP and CLI/server boundaries. The indexer and store are entirely synchronous.

Why: the indexer is CPU-bound (tree-sitter parsing), while async helps at the I/O boundary — MCP stdio proxies, the Unix-socket repository daemon, and the Axum viz server. Blocking semantic/index work is explicitly moved to `tokio::task::spawn_blocking`.

See `docs/adr/0002-tokio-async-boundary.md` for the full rationale.

## Data flow — what happens on `git commit`

1. The user runs `git commit`.
2. `hooks/post-commit` (installed by `gcx init`) shells out to `gcx hook`.
3. If the repository daemon is active, the hook exits immediately; its Git-aware watcher owns synchronization and avoids a second Kuzu process.
4. Otherwise, `gcx hook` reads `last_indexed_sha` for the current branch from the store.
5. If `last_sha == HEAD`, it exits — idempotent no-op.
6. Otherwise, `gitcortex-indexer::run_incremental` diffs against `last_sha`, parses changed files, and produces a `GraphDiff`.
7. `KuzuGraphStore::apply_diff` updates the branch tables and advances `last_sha` to `HEAD`.
8. The hook prints a short summary line and exits, typically in <500ms.

## Data flow — what happens when an AI editor queries

1. The editor sends a JSON-RPC MCP request to `gcx serve` (stdio).
2. The lightweight stdio process connects to, or starts, the repository's local Unix-socket daemon.
3. One daemon owns KuzuDB and serves all connected editor sessions; each connection independently selects compact or full tool mode.
4. `crates/gitcortex-mcp/src/mcp/tools.rs` dispatches the tool call to the matching `GraphStore` method.
5. Results serialize back through the daemon and stdio proxy as MCP JSON.

The `gcx viz` HTTP server is a parallel surface that exposes the same store data to a React UI in the browser via JSON routes (`/data`, `/api/callers/:name`, etc.).

## Storage layout — machine-local

```
<platform-data>/gitcortex/{repo_id}/
    graph.kuzu          # one DB per repo, branch-namespaced tables inside
    main.sha            # last_indexed_sha for branch "main"
    feat__auth.sha      # last_indexed_sha for branch "feat/auth"
    serve.lock          # advisory ownership lock for every graph-opening process
    mcp.sock            # user-only socket while MCP clients are connected
    daemon.log          # most recent daemon startup diagnostics

<platform-cache>/gitcortex/models/
    # replaceable semantic-model weights shared across repositories
```

Path lookup is deterministic from a stable BLAKE3 hash of the absolute working-tree path. Linux follows XDG data/cache roots and macOS uses native Application Support/Cache locations. One Git repo has one graph file; branches are table-namespaced within it.

## Frontend integration

`crates/gitcortex-mcp/viz/` is a standalone Vite + React + TypeScript + Tailwind v4 project. It builds to `crates/gitcortex-mcp/dist-viz/`, which the `build.rs` in `gitcortex-mcp` includes via `include_bytes!` so the final `gcx` binary is self-contained.

For active frontend development, run `npm run dev` against a running `gcx viz` backend — Vite's proxy forwards `/data` and `/api/*` to `:5678`, avoiding rebuild cycles.

> **Runtime packaging:** Viz assets, fonts, and renderer dependencies are bundled into `gcx`; opening the graph does not require a CDN or external font service.

## Where to make changes

| If you want to… | Edit… |
|---|---|
| Add support for a new language (e.g. C++) | `crates/gitcortex-indexer/src/parser/cpp.rs` + register in `parser/mod.rs` |
| Add a new MCP tool | `crates/gitcortex-mcp/src/mcp/tools.rs` + add a method to `GraphStore` if needed |
| Add a new CLI subcommand | `crates/gitcortex-mcp/src/cmd/<name>.rs` + register in `main.rs` |
| Change how a query is computed | `crates/gitcortex-store/src/kuzu.rs` (the Cypher) |
| Modify the viz UI | `crates/gitcortex-mcp/viz/src/` (React + Vite) |
| Add a new HTTP endpoint for the viz | `crates/gitcortex-mcp/src/cmd/viz.rs` (Axum routes) |
| Change the graph schema | `crates/gitcortex-core/src/schema.rs` (then update everything downstream) |

## Further reading

- `docs/adr/` — design decisions with rationale
- `CONTRIBUTING.md` — how to set up the dev loop
- `DEV_SETUP.md` — fresh-clone bootstrap in 3 commands
- `.claude/CLAUDE.md` — agent-facing project guide
