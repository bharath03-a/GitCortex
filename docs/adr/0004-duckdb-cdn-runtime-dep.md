# ADR 0004 — DuckDB-WASM is loaded from a CDN at runtime

**Status:** Accepted (with known caveat)
**Date:** 2026-05-15

## Context

`@cosmograph/cosmograph` (transitively pulled in by `@cosmograph/react`) uses DuckDB-WASM internally for its columnar data layer. At runtime, its module bootstrap calls `getJsDelivrBundles()`, which selects WASM files from `https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@<version>/`.

This means our self-contained-binary viz fetches ~10 MB from jsdelivr on first open:

```
GET https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.32.0/dist/duckdb-browser-eh.worker.js
GET https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.32.0/dist/duckdb-eh.wasm
```

This conflicts with GitCortex's "local-first, no data leaves your machine" framing.

## Decision

For v0.3 we **accept the CDN dependency** and document it prominently. Reasons:

1. The data being processed by DuckDB is *already* in the user's browser — it's the graph payload we already serialized to JSON. No source code or repo content goes to jsdelivr.
2. Cosmograph supports passing in a pre-initialized `WasmDuckDBConnection`, but constructing one from local files requires bundling ~10 MB of WASM. Doing that correctly across browsers + workers is non-trivial.
3. The viz is opt-in (`gcx viz`), not always-on. The MCP integration (`gcx serve`) — which is the primary user surface — does not load any web resources.

## Mitigation

- README's `gcx viz` section documents the CDN dependency
- Status code 0 from CDN failure surfaces a clear error in the viz UI rather than a silent hang
- Air-gapped users are advised to use `gcx viz --format dot | dot -Tsvg` as a fallback, or vendor the WASM files locally (see "Future work")

## Future work — vendor DuckDB-WASM locally

If/when we want to close this gap:

1. Add `@duckdb/duckdb-wasm` to `viz/package.json` as a direct dep (already there transitively)
2. Copy `duckdb-eh.wasm` + `duckdb-browser-eh.worker.js` into `viz/public/duckdb/` at build time
3. Construct a custom `DuckDBBundle` pointing at `/duckdb/duckdb-eh.wasm` and `/duckdb/duckdb-browser-eh.worker.js`
4. Pass it to `<Cosmograph duckDBConnection={...} />`
5. Add Axum routes in `viz.rs` to serve `/duckdb/*` from `include_bytes!`
6. Binary size grows ~10 MB

This is a ~half-day of work. Tracked as a future ADR-0004 follow-up.

## Consequences

**Good**
- Cosmograph upgrades cost zero work — we don't pin DuckDB version
- Binary size is small (the WASM is downloaded lazily, not embedded)

**Bad**
- "Privacy-first" framing has an asterisk
- First-open viz is slow on poor connections
- Air-gapped environments need the workaround
