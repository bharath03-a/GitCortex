# ADR 0003 — Cosmograph (GPGPU) for the viz renderer

**Status:** Accepted (since v0.3 — commit `55c2c86`)
**Date:** 2026-05-15

## Context

The original viz was a 973-line Cytoscape.js monolith embedded as `viz.html` and served via `include_str!`. After GitNexus shipped a slick Sigma.js + Graphology graph explorer, ours felt dated:

- Cytoscape.js is Canvas/SVG — sluggish above 600 nodes, no WebGL path
- Single-file HTML, no React, no Tailwind, no component split
- Every CSS tweak required `cargo build` + binary reinstall

We considered three replacement libraries:

| Library | Rendering | License | Scale | Notes |
|---|---|---|---|---|
| **react-force-graph-2d** | Canvas, d3-force | MIT | ~5k nodes | Drop-in React component, beautiful defaults |
| **Sigma.js v3 + Graphology** | WebGL, FA2 worker | MIT | ~10k nodes | What GitNexus uses; we tried this first and the FA2 tuning was painful |
| **Cosmograph (`@cosmograph/react` v2)** | GPGPU (compute shaders) | MIT | 100k+ nodes | Wraps `@cosmos.gl/graph`; smoothest visual, simplest config |

## Decision

We chose **Cosmograph**. Reasons:

1. **GPU simulation** means we never hand-tune force parameters — the layout converges in <1s on any modern GPU
2. **API is simpler** than Sigma's — no separate worker setup, no warmup-then-stop dance
3. **Visual quality** is the strongest of the three — smooth motion, anti-aliasing, clean clustering
4. **Headroom for scale** — we're nowhere near saturating it at our typical 500–3000 node graphs

## Consequences

**Good**
- 60 fps on any reasonable GPU
- Built-in features: hover labels, focused-point rings, drag-to-pin
- Less code in `CosmosCanvas.tsx` than the equivalent Sigma version would need

**Bad**
- The library is young (v2 has frequent betas) — some API surfaces are not stable
- **Cosmograph loads DuckDB-WASM from `cdn.jsdelivr.net` at runtime** — see ADR 0004
- Software WebGL (headless CI) can't render in reasonable time, limiting our screenshot-test options
- TypeScript types are loose in places (`unknown` accessor values everywhere)

## Migration path if Cosmograph stops working for us

The plug-and-replace candidate is **`react-force-graph-2d`**:

- Same data shape (array of nodes + array of links)
- No DuckDB-WASM dependency
- Pure Canvas, no GPGPU, but plenty fast for our scale
- Simpler API surface — fewer breaking changes from upstream

If we ever need to swap, the affected file is `crates/gitcortex-mcp/viz/src/components/CosmosCanvas.tsx`. Everything else (FilterRail, Inspector, SearchPalette, etc.) is rendering-library-agnostic.

## See also

- [Cosmograph docs](https://cosmograph.app)
- ADR 0004 — DuckDB CDN runtime dependency
- `crates/gitcortex-mcp/viz/src/components/CosmosCanvas.tsx`
