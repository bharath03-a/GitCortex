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

## Visual clarity contract

Large-graph design is governed by viewport budgets, not by how many entities happen to be loaded. A 40,000-symbol repository must not appear as 40,000 equally prominent dots.

### Semantic levels of detail

| Level | Typical paint budget | Primary representation |
|---|---:|---|
| Repository | 30–300 groups | Package/domain supernodes and aggregated boundary relations |
| Module/file | 200–2,000 entities | Expanded groups, files, modules, and bundled relations |
| Symbol | Up to roughly 1,000 entities | Exact symbols and relevant relationships |
| Investigation | Up to 500 relationships by default | Exact directional neighborhood with evidence |

These are paint budgets, not data caps. Search, tables, counts, and backend queries continue to cover the complete indexed graph. Selecting a global search result opens its bounded symbol context directly; the user does not need to expand every ancestor manually.

### Clutter prevention

- Overview edges are aggregated by source group, destination group, relationship kind, Git state, and confidence. Width represents count; selection reveals exact members.
- Internal group relationships are summarized inside the group instead of drawn as self-crossing hairballs.
- Labels follow a strict hierarchy: selected/hovered entity, active path, highest-risk groups, then a small spatially distributed context budget.
- Labels do not overlap silently. Lower-priority labels hide or collapse to counts.
- Edge arrowheads appear only in exact detail and investigation views. Direction in architecture overview is communicated by flow treatment and inspection.
- Hover reveals a group summary; click selects; explicit expand or zoom reveals members. Expansion preserves unrelated group positions and provides breadcrumbs, collapse, and undo.
- Search, Git overlays, and insight lenses change emphasis without rearranging the whole repository unless the user requests a different layout.
- Layout follows the task: clustered architecture map, layered call flow, anchored Git diff, circular cycle inspection, and deterministic hierarchy. One global force layout is not used for every question.

### Visual channel ownership

Do not overload node fill with every dimension:

- **fill hue:** semantic code family;
- **shape:** structural or symbol category;
- **stroke:** Git state (staged, unstaged, added, deleted, conflicted);
- **halo/intensity:** the active insight lens such as risk, churn, or impact;
- **opacity:** surrounding context versus active evidence;
- **edge hue/style:** relationship kind and confidence;
- **badges/counts:** aggregated state that cannot be represented honestly by one stroke.

Light and dark themes use separately tuned palettes with equivalent semantic roles and contrast, not simple color inversion. Product typography is self-hosted so local Viz never depends on an external font service.

## Git state is a first-class axis

Viz must model more than a selected branch. A local repository has layered, changing states:

1. a committed base (`HEAD`, another branch, tag, or arbitrary commit);
2. the Git index, including staged additions, modifications, renames, deletions, and conflict stages;
3. the working tree, including unstaged and untracked files;
4. an optional comparison target such as the merge base, another branch, or another commit.

The UI should identify the active state explicitly as a versioned tuple such as:

```text
{ HEAD oid, index tree oid, worktree generation, comparison oid? }
```

Committed branch graphs remain durable Kuzu snapshots. Staged and working-tree changes are lightweight overlays over the committed graph; they must not rewrite a complete durable branch graph on every edit. An overlay contains added, updated, renamed, deleted, and conflicted nodes and edges. The renderer composes the base snapshot and overlays while keeping unchanged node positions stable.

Git status is visual evidence, not only a filter. Node and relationship treatments must distinguish committed, staged, modified, untracked, deleted, renamed, and conflicted state. Every Git-derived result should show its base, head/state, and freshness so stale index data cannot look current.

### Dynamic update protocol

The local Viz server should watch both relevant source files and Git metadata (`HEAD`, refs, index, merge/rebase state). Updates follow this path:

1. debounce a burst of filesystem events and read one coherent Git state;
2. parse only changed files with the existing tree-sitter indexer;
3. derive a versioned graph patch off the request thread;
4. atomically publish the new repository-state tuple;
5. notify the browser over a loopback-only server-sent event stream;
6. let the browser fetch or receive the bounded patch, update worker-owned adjacency data, and preserve unaffected positions.

Events carry monotonic sequence numbers and their source/target state IDs. A missed event or state mismatch triggers manifest reconciliation rather than applying an unsafe patch. Expensive history rollups never run in this live path.

The dynamic overlay may be rebuilt in memory after server restart. Durable Kuzu writes remain tied to explicit indexing and existing fast Git hooks, avoiding write-lock contention for every keystroke.

### Git-aware product surfaces

- **Working set:** visualize staged, unstaged, untracked, deleted, renamed, and conflicted graph changes relative to `HEAD`.
- **Commit preview:** show how the staged index would change architecture and blast radius before committing.
- **Review map:** compare branch or commit against its merge base, rank affected callers and boundary changes, and retain exact diff evidence.
- **Conflict topology:** show base/ours/theirs symbol relationships for conflicted paths and the dependants affected by each resolution.
- **Timeline:** scrub commits/tags while preserving the camera and stable logical symbol identity where reconciliation is available.
- **Regression trail:** combine Git history, co-change evidence, symbol churn, and graph paths to narrow likely change origins.
- **Live investigation:** keep a selected symbol pinned while its file or callers change, and disclose when it was renamed, removed, or made stale.

## Tool responsibilities

Use each local capability for the work it handles best:

| Layer | Responsibility |
|---|---|
| Git plumbing | Exact repository state, merge bases, status, index trees, rename candidates, commit history, and conflict stages |
| tree-sitter indexer | Incremental symbol and relationship extraction for changed source files |
| KuzuDB | Durable branch/commit graph snapshots and graph-native traversal |
| Rust Viz server | Snapshot composition, Git overlays, aggregation, history queries, deterministic layout/cache coordination, and event streaming |
| Web Worker | Decode compact chunks, maintain frontend adjacency/filters, compute paint sets, and keep work off the UI thread |
| Cosmograph/WebGL | High-volume interactive rendering when available |
| Canvas 2D | Cluster maps and bounded exact investigations in compatibility mode—not a raw 100K-node force simulation |
| Virtualized table/search | Complete keyboard-accessible access to all indexed entities regardless of renderer capability |

No essential investigation may depend exclusively on WebGL. No frontend renderer should become the source of truth for graph or Git state.

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
- Working-tree updates never persist a complete Kuzu branch snapshot per keystroke.
- Staged, unstaged, untracked, renamed, deleted, and conflicted fixtures produce exact overlays against the advertised base state.
- Event sequence gaps and snapshot mismatches reconcile through a fresh manifest instead of silently mixing states.
- Checkout, commit, reset, stash apply, merge, rebase, and conflict-resolution transitions update the state strip without a page reload.
- WebGL loss or initialization failure keeps search, tables, Git state, and bounded investigations operational.
