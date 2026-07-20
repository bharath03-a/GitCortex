# GitCortex Viz Roadmap

GitCortex Viz should be a local-first developer investigation workbench with two complementary modes:

- **Atlas:** progressively load and navigate the complete repository graph, including graphs with tens of thousands of nodes.
- **Investigation:** start from one exact symbol and inspect a bounded, directional neighborhood with source evidence.

The goal is not to hide large graphs. It is to keep every node and edge addressable while changing representation by zoom level and task.

## Product principles

1. Search or a developer question should lead; the graph should answer it.
2. Full graph coverage, loaded data, and currently painted detail are separate concepts.
3. Large graphs require progressive transport and semantic zoom, not arbitrary hard caps.
4. Exact symbol IDs are mandatory for traversal; ambiguous short names must never merge neighborhoods.
5. Graph, relationship table, source evidence, and editor navigation should stay synchronized.
6. Branch comparison and Git history are core product capabilities, not generic graph overlays.
7. Hook and incremental-index paths must remain fast; expensive history analytics run on demand or in a dedicated indexing command.

## Phase 0: correctness and scalable transport

Initial foundation implemented on `feat/viz-investigation-foundation`:

- deterministic paged node and edge reads pushed into Kuzu;
- graph manifest with snapshot SHA, total counts, kind counts, and chunk limits;
- progressive complete-graph loading with abort support and visible progress;
- snapshot validation to reject mixed-version pages;
- true active-branch switching;
- branch comparison that includes added/removed nodes and edges;
- exact-ID caller traversal and bounded exact-ID one-hop neighborhoods;
- global search independent of the filtered canvas;
- filters remove nodes and edges from the simulation instead of merely making them transparent;
- lazy loading of the large Cosmograph renderer chunk;
- Atlas and Investigation modes;
- on-demand most-changed-file overlay from local Git history.

Remaining foundation work:

- move decode, adjacency construction, and filter transforms into a Web Worker;
- use compact typed-array or columnar graph chunks for large atlas loads;
- incrementally update GPU buffers instead of rebuilding the renderer for every chunk;
- expose loaded/visible/total counts separately;
- add memory and frame-time telemetry with adaptive quality;
- benchmark 1k, 10k, 50k, and 100k node fixtures.

## Scale contract

| Tier | Backend | Frontend | User experience |
|---|---|---|---|
| Focused (up to roughly 500 visible relationships) | One bounded exact-ID query | Full labels, arrows, and source synchronization | Precise expansion and traversal |
| Repository (1k–20k nodes) | Versioned chunked nodes and edges | Worker decode, typed arrays, GPU buffers, label budgets | Complete graph loads progressively |
| Large monorepo (20k–100k+ nodes) | Hierarchical package/module/file partitions | Semantic zoom, cluster supernodes, edge aggregation | Overview remains legible; every member is searchable and revealable |
| Beyond certified budget | Manifest reports estimated graph and memory size | Adaptive labels, edges, simulation, and quality | No silent crash; loading can pause, resume, or cancel |

A cluster supernode represents its members; it does not discard them. At 100% atlas coverage, every symbol and relationship remains available for search and detail zoom even if the overview paints aggregated glyphs.

## Core workflows

### Explore symbol

1. Search globally using ranked exact and qualified-name results.
2. Resolve one exact symbol ID.
3. Load a bounded incoming/outgoing neighborhood.
4. Expand relationship classes independently.
5. Inspect source and open the exact location in an editor.

### Change impact

1. Select a symbol, file, commit range, or branch comparison.
2. Rank direct and transitive production callers ahead of tests.
3. Display coverage and truncation explicitly.
4. Distinguish static impact, observed Git co-change, and uncertain inferred edges.

### Trace path

1. Resolve exact source and destination symbols.
2. Show shortest or alternative paths in a layered layout.
3. Explain each edge with kind, confidence, source line, and branch status.

### Architecture map

1. Start with package/module/file groups.
2. Aggregate edges between groups.
3. Zoom into a group without moving unrelated regions unnecessarily.
4. Highlight cycles, unstable boundaries, and high-change hubs.

## Change intelligence

### Available first step

The Viz server can compute an on-demand, branch-specific most-changed-file ranking from local Git history. It records:

- commit touch count;
- additions and deletions;
- last changed timestamp;
- generated/dependency tree exclusions.

The UI maps file churn onto symbol size/color and change-weights relationships connected to those files. This is intentionally described as a **change-weighted current relationship**, not historical relationship volatility.

### Temporal index required next

True “most changed relations” requires recording graph changes over time. It cannot be inferred honestly from only the current graph snapshot.

The temporal model should contain:

- `CommitEvent { sha, timestamp, parents }`;
- `FileChangeEvent { commit, path, status, additions, deletions }`;
- `SymbolChangeEvent { commit, symbol_key, change_kind, signature_hash, body_hash }`;
- `EdgeChangeEvent { commit, src_symbol_key, edge_kind, dst_symbol_key, change_kind, confidence }`;
- aggregate rollups for time windows and co-change pairs.

Node UUIDs are not stable temporal identities because reparsing a changed file may generate new IDs. History must use a stable logical `SymbolKey`, based on language, normalized file, qualified name, and kind, with explicit rename/move reconciliation.

With that index, Viz can provide:

- most changed files, symbols, modules, and relationships;
- relationship add/remove volatility;
- signature churn versus body-only churn;
- files/symbols that repeatedly change together;
- production/test co-change coupling;
- churn × complexity × fan-in hotspot risk;
- timeline playback and commit/tag/branch comparisons;
- commit evidence for every ranking.

History indexing should be an optional bounded initial scan followed by compact incremental events. Co-change matrices and large rollups run on demand or in a background command, never on every Git hook.

## Layout and interaction roadmap

- Preserve Cosmograph as the GPU rendering engine.
- Add deterministic position seeding and position preservation during expansion.
- Add force, layered call-flow, module-map, and hierarchy layouts.
- Add semantic zoom: repository → package/module → file → symbol.
- Aggregate edges at overview zoom; reveal exact edges at detail zoom.
- Add minimap, breadcrumbs, undo/collapse, and selection history.
- Add synchronized virtualized result tables as a keyboard-accessible canvas alternative.
- Serialize branch, query, selected IDs, filters, and layout into URL-safe local state.

## Release gates

- First useful investigation view does not wait for full-atlas loading.
- Atlas loading reports manifest, progress, loaded counts, visible counts, and total counts.
- No main-thread stalls during representative 10k-node loading.
- Large-graph tests measure transport bytes, parse/decode time, memory, FPS, and interaction latency.
- Existing node displacement remains bounded after neighborhood expansion.
- Every canvas result is accessible through a synchronized keyboard-operable list/table.
- Ambiguous short names never trigger traversal.
- Temporal fixtures reproduce file, symbol, and edge event counts exactly across modifications, renames, deletions, and merges.
