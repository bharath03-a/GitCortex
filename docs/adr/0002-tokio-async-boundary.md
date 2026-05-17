# ADR 0002 — `tokio` lives only at the I/O boundary

**Status:** Accepted (since v0.1)
**Date:** 2026-04-23

## Context

We need async runtime for two things:

1. The MCP server over stdio (`rmcp` is built on `tokio`)
2. The Axum HTTP server for `gcx viz`

We do **not** need async for:

- tree-sitter parsing (CPU-bound)
- git2 differ (CPU-bound, with optional Tokio-flavoured futures we don't use)
- KuzuDB queries (the Rust bindings are not `Send + Sync` across `.await`)

If we pulled `tokio` into the indexer or the store, we'd be paying for an executor and infectious `async fn` annotations everywhere with no actual concurrency to gain.

## Decision

`tokio` appears in `crates/gitcortex-mcp/Cargo.toml` only. The `gitcortex-core`, `gitcortex-indexer`, and `gitcortex-store` crates are entirely synchronous.

When an async handler needs to call into the store or indexer, it uses `tokio::task::spawn_blocking` to move the call onto the blocking-IO thread pool, leaving the async executor free.

## Consequences

**Good**
- The indexer crate is trivially callable from sync code, scripts, build steps, and tests
- The store is trivially callable from sync code; no async colour to propagate
- The async surface is small and reviewable
- `cargo check -p gitcortex-core` is fast — no `tokio` proc-macros to expand

**Bad**
- Every store call from a handler has to be wrapped in `spawn_blocking` — easy to forget
- Mistakes (blocking calls in async context) won't be caught by the type system; they're a runtime issue

## Enforcement

- `crates/gitcortex-mcp/Cargo.toml` has `tokio` in `[dependencies]`. No other crate does.
- Tracing instrumentation on all async routes makes blocking stalls visible at runtime.
- Future work (`tokio-console` integration) would surface these stalls during development.

## See also

- [Tokio docs on bridging sync/async code](https://tokio.rs/tokio/topics/bridging)
- `crates/gitcortex-mcp/src/cmd/viz.rs` — the canonical pattern (post-Phase F)
