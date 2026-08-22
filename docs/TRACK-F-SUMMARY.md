# Track F — Viz scale-readiness: summary

Scope: `viz/` and `docs/VIZ-HANDOFF-PLAN.md`/`docs/VIZ-ROADMAP.md` only, per
Track F (items 16-17) of the approved plan at
`~/.claude/plans/prancy-leaping-sparrow.md`. No changes made outside that
scope.

## Step 0 — branch/PR mergeability (item 16)

`docs/VIZ-HANDOFF-PLAN.md` claimed a branch named `feat/viz-investigation-foundation`
existed, was rebased onto `main`, green on CI, pushed to origin, and just
needed its PR opened. **That branch does not exist** — not locally, not on
`origin`, not on the `no-mistakes` remote (verified with `git branch -a` and
`git fetch --all`).

What actually happened: the described work (complexity-score Web Worker,
synthetic-graph fixtures, `viz/scripts/generate-fixtures.ts`, the handoff-plan
edits marking fixtures done) shipped under a **different branch name**,
`feat/viz-perf-and-architecture`, as **PR #63** — "perf(viz): move complexity
scoring off main thread via Web Worker" — which **merged into `main` on
2026-08-06** (verified with `gh pr list --state all --search "head:feat/viz-perf-and-architecture"`).

So step 0 is not "blocked" or "needs a human to click one button" — **it's
already done**, just under the wrong name in the doc. There is no PR to draft
or open. `docs/VIZ-HANDOFF-PLAN.md` has been corrected in place to say this
plainly instead of repeating the stale claim.

No PR title/description draft is included here because there is no PR left
to open — the work already reached `main`.

## Step 1 implemented (item 17, Workstream A sub-step 1)

Per `docs/VIZ-HANDOFF-PLAN.md`'s own Workstream A list, sub-step 6
(benchmark fixtures) was already done; sub-steps 1-5 remained. Sub-step 1 was
first in listed order: **"Move decode/adjacency/filter transforms off the
main thread. Web Worker boundary around `viz/src/graph/view.ts` and
`density.ts`."**

Implemented:

- `viz/src/graph/view.ts` — was a one-line type file (`ViewMode`). Now also
  exports a pure `computeView(input: ViewInput): GraphData` that does exactly
  what used to be inline in `App.tsx`'s `data` useMemo: view-mode source
  selection (atlas vs. investigate), diff-overlay merge, `applyDensity`
  reduction, then node-kind/visibility/flag filtering and edge-kind/confidence
  filtering. Pure function, no worker/DOM dependency, independently testable.
- `viz/src/graph/viewWorker.ts` — new. Wraps `computeView` as a Web Worker
  entry point, mirroring the existing `complexityWorker.ts` pattern exactly.
- `viz/src/hooks/useFilteredGraph.ts` — new. Mirrors `useComplexityScores.ts`:
  spins up the worker on input change, posts the `ViewInput`, resolves the
  result into state. Deliberately keeps the previous filtered graph visible
  while a new one computes (returns `input ? result : null` rather than
  clearing to `null` synchronously in the effect) so toggling a filter
  doesn't flash the canvas empty during the round trip.
- `viz/src/App.tsx` — the old inline synchronous filter pipeline (58 lines)
  is now a `viewInput` memo (cheap object construction) plus
  `useFilteredGraph(viewInput)`. No behavior change to what gets filtered,
  only where the filtering runs.
- `viz/src/graph/__tests__/view.test.ts` — new, 8 cases covering
  `computeView`: atlas passthrough, investigate-mode source selection and its
  no-focused-data-yet fallback, diff-overlay merge with dedup, diff overlay
  correctly skipped in investigate mode, kind/visibility filtering,
  async/unsafe flag filtering, and edge-kind/confidence filtering combined
  with node drop-through.

Verified: `npx tsc --noEmit` clean, `npx eslint .` clean (0 errors after
fixing two `no-unnecessary-type-assertion` and one
`react-hooks/set-state-in-effect` finding introduced along the way),
`npx prettier --check` clean, `npx vitest run` — 30/30 tests pass (22
pre-existing + 8 new), `npm run build` succeeds and emits `viewWorker.js` as
its own chunk in `crates/gitcortex-viz/dist-viz/assets/`, alongside the
pre-existing `complexityWorker.js`.

**Not done as part of this**: an actual Chrome DevTools performance trace
against the 10k-node fixture to confirm the acceptance criterion ("no long
task >50ms on the main thread"). That requires driving the running dev build
in a browser, which wasn't done here — build/lint/test/typecheck all passing
is necessary but not sufficient evidence for that specific acceptance bar.
Flagged in `docs/VIZ-HANDOFF-PLAN.md` as still open.

## What remains (Workstream A, in the doc's own order)

1. Done (this task).
2. **Typed-array/columnar chunk format** for graph transport
   (`crates/gitcortex-viz/src/lib.rs` backend + frontend decode) — next up,
   not started. This is a wire-format change (backend and frontend move
   together), out of this task's single-step scope and touches `crates/`,
   which this task was told not to touch.
3. Incremental GPU buffer updates in `CosmosCanvas.tsx` — not started.
4. Separate loaded/visible/total counts in `StatusBar.tsx` — not started.
5. Memory + frame-time telemetry with adaptive quality — not started.

Workstream B (temporal index) and C (layout/interaction) untouched, per plan
scope and the handoff doc's own sequencing (B needs its own design pass
first; C is blocked on A finishing).

## Files touched

- `viz/src/graph/view.ts` (rewritten: added `computeView`)
- `viz/src/graph/viewWorker.ts` (new)
- `viz/src/hooks/useFilteredGraph.ts` (new)
- `viz/src/App.tsx` (filter pipeline moved to the new hook)
- `viz/src/graph/__tests__/view.test.ts` (new)
- `docs/VIZ-HANDOFF-PLAN.md` (corrected branch/PR status, marked sub-step 1 done)

Everything left unstaged/uncommitted, nothing pushed, no changes to
`crates/`, `README.md`, `.github/workflows/`, or any `docs/TRACK-*-SUMMARY.md`
other than this file.
