# Viz Hand-off Plan

Execution plan for finishing `docs/VIZ-ROADMAP.md`. That document is the
product spec (principles, scale contract, workflows); this document is the
sequencing — what to build in what order, where, and how to know each step
is done. Written so a different engineer or a future session can pick this
up cold.

## Where things stand

`feat/viz-investigation-foundation` shipped Phase 0's correctness work and
is rebased onto `main`, green on build/test/fmt/clippy, pushed to origin.
**Not yet a PR.** Opening that PR is step 0 below — do it before anything
else in this doc, because every later step needs a stable base to branch
from and CI's read on the rebase.

Frontend lives in `viz/src/` (Vite + React + TypeScript, Cosmograph
renderer). Backend lives in `crates/gitcortex-viz/src/lib.rs` (single file,
1386 lines, 48 functions — flag for a split during Workstream A, since it
already exceeds the project's 800-line file ceiling).

Key existing files:
- `viz/src/graph/view.ts`, `viz/src/graph/density.ts` — graph transform/render logic
- `viz/src/components/CosmosCanvas.tsx` — Cosmograph wrapper
- `viz/src/api.ts` — backend client
- `crates/gitcortex-viz/src/lib.rs` — Axum server: paged reads, manifest, branch diff, churn overlay

## Workstream A — Finish Phase 0 (scale correctness)

Roadmap's own "Remaining foundation work" list. This is the highest-leverage
work: Atlas mode is currently correct but will stall the main thread on
anything past a few thousand nodes.

1. **Move decode/adjacency/filter transforms off the main thread.**
   Web Worker boundary around `viz/src/graph/view.ts` and `density.ts`.
   Acceptance: loading a 10k-node fixture produces no long task
   (>50ms) on the main thread, verified with Chrome DevTools performance trace.

2. **Typed-array/columnar chunk format** for graph transport, replacing
   whatever JSON shape `crates/gitcortex-viz/src/lib.rs`'s paged endpoints
   currently emit. Backend and frontend change together — this is a wire
   format change, not additive.
   Acceptance: transport bytes for a 10k-node fixture measured before/after,
   documented reduction.

3. **Incremental GPU buffer updates** in `CosmosCanvas.tsx` — append new
   chunks to existing Cosmograph buffers instead of rebuilding the renderer
   per chunk.
   Acceptance: loading N chunks does not re-render already-painted nodes;
   frame time stays flat as chunks arrive rather than spiking per chunk.

4. **Separate loaded/visible/total counts** in the UI (`StatusBar.tsx` is
   the likely home). These are currently conflated per the roadmap.

5. **Memory + frame-time telemetry with adaptive quality.** Simplest
   version: sample `performance.memory` (or a frame-time rolling average)
   and drop label/edge density when frame time degrades.

6. **Benchmark fixtures at 1k/10k/50k/100k nodes.** These don't exist yet —
   generate synthetic graphs (e.g. a script producing N nodes/edges in the
   manifest wire format) and check them into `tests/integration/fixtures/`
   or a viz-specific fixtures directory. Every acceptance criterion above
   depends on these existing first — build this sub-step before 1–5, not after.

**Release gates this workstream must hit** (from `VIZ-ROADMAP.md`):
- first useful investigation view does not wait for full-atlas load
- no main-thread stalls during a representative 10k-node load
- large-graph tests measure transport bytes, decode time, memory, FPS, interaction latency

## Workstream B — Temporal index (Phase 1 foundation)

Bigger, schema-level, should be scoped as its own design pass before code —
don't start implementation without a short design note answering the open
questions below.

The roadmap specifies the event types directly:
```
CommitEvent { sha, timestamp, parents }
FileChangeEvent { commit, path, status, additions, deletions }
SymbolChangeEvent { commit, symbol_key, change_kind, signature_hash, body_hash }
EdgeChangeEvent { commit, src_symbol_key, edge_kind, dst_symbol_key, change_kind, confidence }
```

Open questions to resolve before coding:
- Where does this live — new KuzuDB tables (`gitcortex-store`), or a
  separate store entirely? The main graph is branch-namespaced and
  snapshot-based; history is append-only and cross-branch. These have
  different lifecycles and probably shouldn't share a table design.
- `SymbolKey` (language, normalized file, qualified name, kind) needs
  rename/move reconciliation — this is the hard part. Decide the matching
  heuristic (e.g. signature-hash continuity across a rename) before writing
  the indexer.
- Indexing trigger: roadmap says "optional bounded initial scan followed by
  compact incremental events," explicitly NOT on every git hook. Needs a
  new `gcx` subcommand (e.g. `gcx index-history`) separate from the hook path.

Acceptance: temporal fixtures reproduce file/symbol/edge event counts
exactly across modifications, renames, deletions, and merges (this is
already stated as a release gate in the roadmap — treat it as the spec for
this workstream's test suite).

Only after this index exists do the roadmap's "true most-changed" features
(relationship volatility, signature vs. body churn, co-change coupling,
churn × complexity × fan-in hotspots, timeline playback) become buildable.
Don't attempt those without the index — they'd be re-deriving the same
proxy data the roadmap explicitly says can't be done honestly from a single
snapshot.

## Workstream C — Layout and interaction

Lowest priority of the three; depends on A being done (no point adding
layouts to a renderer that stalls at scale) and is largely independent of B.

- Deterministic position seeding + position preservation during expansion
- Additional layouts: force, layered call-flow, module-map, hierarchy
- Semantic zoom: repository → package/module → file → symbol, with edge
  aggregation at overview and exact edges at detail zoom
- Minimap, breadcrumbs, undo/collapse, selection history
- Synchronized virtualized result table (keyboard-accessible canvas alternative)
- URL-serialized state: branch, query, selected IDs, filters, layout

No fixed acceptance criteria given in the roadmap for this workstream —
define them per-feature when picked up, following the same
measure-before/measure-after discipline as Workstream A.

## Suggested order

1. Open the PR for `feat/viz-investigation-foundation` (blocking everything else)
2. Workstream A, sub-step 6 first (fixtures), then 1–5 in listed order
3. Workstream B design note, then implementation
4. Workstream C, feature by feature, lowest priority

## Non-goals for whoever picks this up

- Don't touch the retrieval/agent-bench work (`tools/agent-bench/`,
  `crates/gitcortex-mcp/src/mcp/agent.rs`) — that's Track 1, tracked
  separately in `docs/benchmarks/AGENT-BENCHMARK-PLAN.md`.
- Don't add abstractions ahead of Workstream C needing them — Workstream A's
  typed-array format and worker boundary should be sized for what A and B
  need, not speculatively generalized for layouts that don't exist yet.
