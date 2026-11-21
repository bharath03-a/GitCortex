# Graph Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Anyone can export the whole graph, or a scoped "what leads to X / what X leads to" subgraph, as:
- a documented, versioned JSON that any front end can load,
- Mermaid for GitHub and docs,
- the existing DOT/SVG/GraphML/Cypher/HTML formats,
- a static copy of the GitCortex viz app that can be hosted anywhere.

**Architecture:** One `ScopedGraph` is resolved from the store with an `ExportScope` (seed, depth, direction, edge kinds, cap), and every format renders from it. The format code moves out of `gitcortex-viz/src/lib.rs` (1,545 lines) into `export/`. The CLI builds `ExportMeta` including readiness, so a stale or missing index is never exported silently. The static bundle reuses the embedded viz assets with an in-browser data source.

**Tech Stack:** Rust (gitcortex-viz, gitcortex-cli), serde_json, `jsonschema` (dev-dep), React/Vite/TypeScript + Vitest (viz).

**Spec:** [Program overview](2026-11-05-00-release-program.md). Maintainer, 2026-11-05: "export the connections graph … a front end page we can see what leads to what". Selected: versioned JSON, scoped subgraph, Mermaid, static viewer bundle. Viz principle 4 in `docs/VIZ-ROADMAP.md`: exact IDs for traversal; never merge ambiguous names.

## Global Constraints

- `gcx viz --format html|dot|svg|graphml|cypher` output for an unscoped graph is byte-identical before and after the T1 refactor, **at any graph size**: legacy callers use `ExportScope::legacy_full()` (no cap, no re-sort, store order from `list_all_nodes`/`list_all_edges`). The 5000-node cap and deterministic sort apply only to the new `gcx export` path. T5 intentionally changes the embedded web assets (relative base); `gcx viz --format web` must keep working at `/`.
- `gcx export --format json` (legacy symbols + edges) and `--claude-md` keep their current output.
- Exports contain names, kinds, paths, and lines only, never source text.
- If readiness is not `ready`, the CLI prints the state to stderr and exits 3, unless `--allow-stale` is passed. Even then, `meta.readiness` records the state.
- One process exit policy for the whole CLI: T3 (layer D3) owns `crates/gitcortex-cli/src/exit.rs` (`CliExit`, `exit_code`), and [Plan 05](2026-11-05-05-pr-impact-action.md) E2/E3 consume it. Codes: invalid input (ambiguous/missing seed, invalid ref or revision) = 2; not ready = 3; any other error = 1; clap argument errors stay 2.
- Graph adaptation boundary with Plan 05: `ScopedGraph` and the graph-v1 serializer accept explicit nodes and edges (T1 `ScopedGraph::from_explicit`) and never re-resolve IDs against a live store, so base-only nodes survive. Impact classes (multiple changed seeds, caller, deleted), hops, and base/head presence are **not** graph-v1 fields: they live in Plan 05's impact envelope `node_evidence` and are rendered only by Plan 05's JS renderer (X7). `to_mermaid` stays a single-seed export renderer; no second Rust impact-comment renderer is added here. `meta.scope` is always an `ExportScope`, never readiness.
- Directions: `up` = incoming edges (what leads to X; maps to store `"in"`), `down` = outgoing (`"out"`), `both`.
- Defaults: `--depth 2` (max 5, as the store), `--direction both`, `--max-nodes 5000` (Mermaid: 100).
- Edge-kind filters constrain traversal, not just output: a hop is followed only over an allowed edge kind.
- The static bundle works from any URL prefix (GitHub project Pages serves under `/<repo>/`): no root-relative URL in HTML, JS, CSS, lazy chunks, or worker URLs.

## Review Focus

1. Short seed name matching several symbols → exit 2, listing the candidates' `qualified_name`s and ids; never merged. Test: T3 `ambiguous_seed_lists_candidates`.
2. Stale or missing index → no silent empty export. Test: T3 `missing_index_exits_3`, T2 `meta_records_readiness`.
3. Names containing `"`, `<script>`, `` ` ``, `|`, brackets, or unicode → escaped in Mermaid/HTML/DOT. Test: T4 `mermaid_escapes_hostile_names`, T1 golden fixture includes such a name.
4. Large graph → new exports capped with `truncated: true` and the total counts (Mermaid adds a `%% truncated` line), while legacy `gcx viz` formats stay uncapped and byte-identical; a mixed-kind path is never followed through a filtered-out kind. Test: T1 `legacy_full_scope_uncapped_over_5000`, `edge_kind_filter_applies_during_traversal`, T2 `cap_sets_truncated_and_totals`, T4 `mermaid_truncation_comment`.
5. Bundle opened through `file://`, hosted under a subpath, or started without a server → the page shows "Serve this folder over HTTP, e.g. `python3 -m http.server`" for `file:`, loads every asset under `/GitCortex/`, and boots its branch from static metadata. Test: T5 `bundle_has_file_protocol_guard`, `scripts/test_bundle_subpath.py`, Vitest `app boots in static mode`.
6. Impact graph loses base-only (deleted) nodes or gets re-resolved through the head store; exit policy forks between export and blast-radius. Test: T1 `from_explicit_keeps_unresolvable_ids`, `from_explicit_rejects_dangling_edge`, T2 `explicit_union_preserves_base_only_nodes`, T3 `exit_code_downcasts_through_context`, `invalid_branch_ref_exit_2`.

---

### Task 1: ScopedGraph and format extraction (no behavior change)

**Files:**
- Create: `crates/gitcortex-viz/src/export/mod.rs`, `export/scope.rs`, `export/formats/{dot,html,svg,graphml,cypher}.rs`, `crates/gitcortex-viz/tests/export_golden.rs`, `crates/gitcortex-viz/tests/golden/` (one file per format)
- Modify: `crates/gitcortex-viz/src/lib.rs` (`build_*` at lines ~1097–1410 move out; `run` calls into `export`)

**Interfaces:**
- Produces:
  - `pub enum Direction { Up, Down, Both }`, with `fn as_store(&self) -> &'static str`.
  - `pub enum NodeOrder { Store, Deterministic /*(file, start_line, id)*/ }`.
  - `pub struct ExportScope { pub seed: Option<String>, pub depth: u8, pub direction: Direction, pub edge_kinds: Option<Vec<EdgeKind>>, pub max_nodes: Option<usize> /*None = uncapped*/, pub order: NodeOrder }`, with `impl Default` (cap `Some(5000)`, `Deterministic`) and `ExportScope::legacy_full()` (`seed: None`, `max_nodes: None`, `NodeOrder::Store`), used by every existing `gcx viz` renderer call (`lib.rs:80–96`).
  - `pub struct ScopedGraph { pub branch: String, pub nodes: Vec<Node>, pub edges: Vec<Edge>, pub truncated: bool, pub total_nodes: usize, pub total_edges: usize, pub seed: Option<Node> }`.
  - `impl ScopedGraph { pub fn from_explicit(branch: String, nodes: Vec<Node>, edges: Vec<Edge>) -> Result<ScopedGraph> }`: no store access. Keeps nodes and edges as given (including IDs that no store on `branch` holds, such as base-only deleted nodes), `seed: None`, `truncated: false`, totals = lengths. Errors on an edge whose endpoint is not in `nodes` (fail fast, no silent drop). Plan 05 E3 calls it with the impact base∪head union and the head SHA as `branch`. The single-seed `resolve_scope` path and every renderer signature stay unchanged.
  - `pub enum SeedError { NotFound(String), Ambiguous(Vec<Node>) }`.
  - `pub fn resolve_seed(store: &dyn GraphStore, branch: &str, seed: &str) -> Result<Node>`, matching in order: exact id → exact `qualified_name` → exact `name`, unique or `Ambiguous`.
  - `pub fn resolve_scope(store: &dyn GraphStore, branch: &str, scope: &ExportScope) -> Result<ScopedGraph>`. Unseeded: all nodes/edges, then edge-kind filter. Seeded: an in-memory BFS over `list_all_edges` (not `get_subgraph_by_id`, which follows every edge kind — `kuzu/mod.rs:2228–2240`) that only follows edges passing `fn edge_allowed(&Edge, &ExportScope) -> bool` in the requested direction, up to `depth`; returned edges are those between kept nodes that pass the same predicate. Then the cap, if any, in `scope.order`.
  - `pub fn to_dot/to_html/to_svg/to_graphml/to_cypher(g: &ScopedGraph) -> String`.

- [ ] **Step 1: Write golden tests** on the **current** code. (a) `tests/integration/fixtures/sample.rs` plus a symbol named `` "x<script>`|" ``: capture each format into `tests/golden/`. (b) `legacy_full_scope_uncapped_over_5000`: a generated fixture with 5,001+ functions; store the sha256 of each format's output in `tests/golden/large.<fmt>.sha256` (not the full output). Assert current outputs match. Expected: PASS.
- [ ] **Step 2: Write failing unit tests** in `scope.rs`: `resolve_seed_prefers_id_then_qualified_then_name`, `resolve_seed_ambiguous`, `up_returns_callers_only`, `down_returns_callees_only`, `edge_kind_filter_applies_during_traversal` (A `Imports`→ B `Calls`→ C, seed A, depth 2, `edge_kinds=[Calls]` → only A; `[Imports]` → A, B; no C), `cap_is_deterministic`, `legacy_full_preserves_store_order`, `from_explicit_keeps_unresolvable_ids` (a node absent from every store survives with its ID), `from_explicit_rejects_dangling_edge`.
- [ ] **Step 3: Run** `cargo test -p gitcortex-viz`. Expected: unit tests FAIL.
- [ ] **Step 4: Implement** the scope and move the renderers to take `&ScopedGraph`; `gcx viz` calls them with `resolve_scope(.., &ExportScope::legacy_full())`.
- [ ] **Step 5: Run** `cargo test -p gitcortex-viz`. Expected: all PASS, goldens and large-fixture hashes unchanged.
- [ ] **Step 6: Commit.** `refactor: render all graph formats from a scoped graph`

### Task 2: Versioned graph JSON + JSON Schema

**Files:**
- Create: `crates/gitcortex-viz/src/export/graph_json.rs`, `docs/schemas/gitcortex-graph-v1.schema.json`
- Modify: `crates/gitcortex-viz/Cargo.toml` (dev-dep `jsonschema`)

**Interfaces:**
- Produces:
  - `pub const GRAPH_JSON_SCHEMA_VERSION: u32 = 1;`
  - `pub struct ExportMeta { pub gcx_version: String, pub repo_name: String, pub commit: Option<String>, pub readiness: String, pub generated_at: String /*RFC 3339 UTC*/, pub scope: ExportScope }`
  - `pub fn to_graph_json(g: &ScopedGraph, meta: &ExportMeta) -> serde_json::Value`
  - Document shape:
    ```json
    {"format":"gitcortex.graph","schema_version":1,
     "meta":{"gcx_version":"0.8.0","repo":"GitCortex","branch":"main","commit":"<sha>","readiness":"ready",
             "generated_at":"2026-11-05T00:00:00Z",
             "scope":{"seed":null,"depth":2,"direction":"both","edge_kinds":null,"max_nodes":5000},
             "truncated":false,"total_nodes":0,"total_edges":0},
     "nodes":[{"id":"","name":"","qualified_name":"","kind":"Function","file":"","start_line":1,"end_line":2,
               "visibility":"pub","is_async":false,"is_unsafe":false}],
     "edges":[{"source":"<node id>","target":"<node id>","kind":"Calls","line":12,"confidence":"Extracted"}]}
    ```
    `source` and `target` follow d3, Cytoscape, vis-network, and Cosmograph conventions.
  - `to_graph_json` is a pure serializer of the given `ScopedGraph` (no store, no ID re-resolution). The schema allows any non-empty string for `meta.branch` (Plan 05 passes the head SHA), `meta.scope.seed: null`, and `meta.scope.max_nodes: null` (uncapped). graph-v1 has no impact class, hop, or presence fields; Plan 05 keeps those in its envelope `node_evidence`, keyed by graph node `id`.

- [ ] **Step 1: Write failing tests:** `graph_json_validates_against_schema`, `edges_reference_existing_nodes`, `meta_records_readiness` (passing `readiness: "stale"` round-trips), `cap_sets_truncated_and_totals`, `explicit_union_preserves_base_only_nodes` (schema pin: `from_explicit` with a base-only node and an edge to it, `branch` = a 40-hex SHA, `scope.seed = null`, `max_nodes = None`; output validates, keeps the node and edge, `truncated == false`, totals equal the input counts).
- [ ] **Step 2: Run.** Expected: FAIL. **Step 3: Implement** the function and the schema file. **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat: add versioned gitcortex graph JSON format`

### Task 3: CLI scoping flags

**Files:**
- Create: `crates/gitcortex-cli/src/exit.rs` (shared exit helper; this layer is its only owner)
- Modify: `crates/gitcortex-cli/src/main.rs` (`Export` variant; replace the blanket `std::process::exit(1)` at ~line 366 with `exit::exit_code`), `crates/gitcortex-cli/src/cmd/export.rs`
- Test: `crates/gitcortex-cli/tests/export_cli.rs` (spawn the binary with `GCX_STORE_PATH`, as `doctor_exit.rs` does)

**Interfaces:**
- Consumes: T1 `resolve_scope`, T2 `to_graph_json`, and `gitcortex_mcp::mcp::readiness::inspect` for `ExportMeta.readiness`.
- Produces:
  - `gcx export --format markdown|json|graph-json|mermaid|dot|svg|graphml|cypher|html|bundle [--branch B] [--seed S] [--depth N] [--direction up|down|both] [--edge-kinds calls,imports,...] [--max-nodes N] [--out PATH] [--allow-stale] [--force]`. Writes to stdout unless `--out` is given; `bundle` requires `--out`.
  - `exit.rs`: `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum CliExit { InvalidInput, NotReady }` with manual `Display` + `std::error::Error` (the CLI crate has no `thiserror` dependency; do not add one) and `pub fn exit_code(error: &anyhow::Error) -> i32`, which walks the context chain (`error.chain()` + `downcast_ref::<CliExit>`): `InvalidInput` → 2, `NotReady` → 3, anything else → 1. `main` prints `error: {e:#}` as today, then exits with `exit_code(&e)`. Export maps `SeedError::{NotFound, Ambiguous}` and an unresolvable `--branch` ref to `InvalidInput` (message keeps the candidate list) and non-ready readiness to `NotReady`. Plan 05 E2 (`gcx index --rev` invalid revision → 2) and E3 (blast-radius invalid ref → 2, not ready → 3) consume this helper and add no parallel policy. If E2 is ready before this layer merges, it waits for the merged D3 SHA (Plan 07); it does not create its own `exit.rs`.

- [ ] **Step 1: Write failing tests:** `graph_json_full_export`, `seed_down_depth1_only_callees`, `seed_up_lists_what_leads_to_it`, `ambiguous_seed_lists_candidates` (exit 2; stderr has ≥ 2 qualified names), `unknown_seed_exit_2`, `invalid_branch_ref_exit_2`, `missing_index_exits_3`, `unrelated_error_exits_1` (store open failure), `allow_stale_exports_with_meta_state`, `legacy_json_unchanged` (compare with a golden). Unit test in `exit.rs`: `exit_code_downcasts_through_context` (`CliExit::NotReady` wrapped in two `.context(..)` layers → 3; `InvalidInput` → 2; plain `anyhow!` → 1). This is the contextual-downcast pin that later layers keep. Existing CLI pins stay green unchanged: `doctor_exit.rs` (nonzero doctor exit, missing-graph tour), clap usage errors exit 2.
- [ ] **Step 2: Run.** Expected: FAIL. **Step 3: Implement.** **Step 4: Run.** Expected: PASS, including `cargo test -p gitcortex --test doctor_exit`.
- [ ] **Step 5: Commit.** `feat: scoped graph export from the CLI`

### Task 4: Mermaid

**Files:**
- Create: `crates/gitcortex-viz/src/export/formats/mermaid.rs`

**Interfaces:**
- Produces: `pub fn to_mermaid(g: &ScopedGraph) -> String`:
  - `flowchart LR`; node ids `n0..nK` in ScopedGraph order.
  - Label `"<name><br/><kind>"`, escaped: `"`→`#quot;`, `<`→`#lt;`, `>`→`#gt;`, `|`→`#124;`, `` ` ``→`#96;`.
  - Edge `nA -->|Calls| nB`; one `classDef` per NodeKind; the seed gets class `seed`.
  - When truncated, the first line after the header is `%% truncated: showing X of Y nodes`.
  - Export-only (single `seed` class). This layer (D4) is optional export functionality, not a prerequisite for Plan 05: impact comments are rendered by Plan 05 X7 JS from graph-v1 + `node_evidence`, and `to_mermaid` gains no impact classes.

- [ ] **Step 1: Write failing tests:** `mermaid_header_and_edges`, `mermaid_escapes_hostile_names`, `mermaid_stable_output` (two runs are identical), `mermaid_truncation_comment`, `mermaid_default_cap_100` (CLI test in `export_cli.rs`).
- [ ] **Step 2: Run.** Expected: FAIL. **Step 3: Implement.** **Step 4: Run.** Expected: PASS. Then paste one output into a GitHub comment preview and confirm it renders.
- [ ] **Step 5: Commit.** `feat: export graphs as Mermaid`

### Task 5: Static viewer bundle

**Files:**
- Create: `crates/gitcortex-viz/src/export/bundle.rs`, `crates/gitcortex-viz/src/assets.rs` (move the `VIZ_*` `include_bytes!` statics here), `viz/src/staticSource.ts`, `viz/src/__tests__/staticSource.test.ts`, `viz/src/__tests__/appStatic.test.tsx`, `scripts/test_bundle_subpath.py`
- Modify: `viz/vite.config.ts` (`base: "./"`), `viz/index.html` (favicon `./favicon.svg`; today `/favicon.svg` and `/src/main.tsx` are root-relative, lines 7, 20), `viz/src/api.ts` (`/data`, `/api/*` → relative `./data`, `./api/*`), `viz/src/App.tsx` (branch bootstrap, lines ~176–186), the panels that use branch-diff or hot-files (hide them when `capabilities.static`), `.github/workflows/ci.yml` (python-tools runs the subpath test)

**Interfaces:**
- Produces:
  - Rust: `pub fn write_bundle(dir: &Path, graph_json: &serde_json::Value, force: bool) -> Result<()>`. It writes all embedded assets at their served paths (relative to the bundle root) plus `graph.json`, and injects `<meta name="gcx-static-data" content="graph.json">` and an inline non-module `file:`-protocol guard before `</head>` in `index.html`. It fails on a non-empty dir unless `force`.
  - TS: `loadStatic(url: string): Promise<StaticGraph>`. `StaticGraph` provides `manifest()`, `branches()`, `nodesPage(offset, limit)`, `edgesPage(offset, limit)`, `neighborhood(id, direction, limit)`, `callersById(id, depth)`, `godNodes(limit)`, `unused()`, all with the same return shapes as the current API. `api.ts` routes to it when the meta tag is present (URL resolved against `document.baseURI`).
  - `branches()` returns `BranchListResult` built from `graph.json` `meta` (`active: meta.branch`, `last_sha: meta.commit`, `branches: [meta.branch]`), so `App.tsx`'s `fetchBranches()` bootstrap sets `activeBranch` and the graph loads. Only branch-diff and hot-files throw `StaticUnsupportedError`; the manifest returns `capabilities: { static: true }`.
  - All emitted URLs are relative: Vite `base: "./"` covers entry, CSS, fonts, the lazy `CosmosCanvas` chunk (`App.tsx:28`), and both workers (`new URL(..., import.meta.url)` in `useComplexityScores.ts`/`useFilteredGraph.ts`).

- [ ] **Step 1: Write failing tests:**
  - Rust `bundle_contains_assets_and_meta`, `bundle_refuses_non_empty_dir`, `bundle_has_file_protocol_guard`, `bundle_has_no_root_relative_urls` (scan `index.html`, `*.js`, `*.css` for `"/assets/`, `src="/`, `href="/`, `url(/`, `fetch("/`).
  - Rust `viz_web_still_serves_at_root` (existing `gcx viz` server loads `index.html` and every referenced asset at `/`).
  - `scripts/test_bundle_subpath.py` (unittest): write a bundle to `<tmp>/GitCortex/`, serve `<tmp>` with `http.server`, fetch `/GitCortex/`, then every URL referenced from `index.html`, `main.js` (dynamic imports, worker URLs), and `main.css`, resolved against `/GitCortex/` → all 200, and none hit `/assets/` at the server root.
  - Vitest `neighborhood matches server semantics` (fixture graph; `up` = incoming), `paging covers all nodes exactly once`, `unsupported endpoints throw StaticUnsupportedError`, `static branches come from graph meta`.
  - Vitest `app boots in static mode` (`appStatic.test.tsx`: render `App` with the meta tag and a mocked `fetch` for `graph.json` only; asserts the graph loads for `meta.branch`, no `/api/branches` request, no error banner).
- [ ] **Step 2: Run** `cargo test -p gitcortex-viz`, `cd viz && npx vitest run`, and `python3 -m unittest scripts/test_bundle_subpath.py`. Expected: FAIL.
- [ ] **Step 3: Implement**, then `just viz-build` so `dist-viz` refreshes.
- [ ] **Step 4: Run** all three suites. Expected: PASS. Manual: `gcx export --format bundle --out /tmp/gcx-site/GitCortex && python3 -m http.server -d /tmp/gcx-site 8000`, open `http://localhost:8000/GitCortex/`. Search a symbol, open its neighborhood (exercises the lazy canvas and workers), and confirm branch-diff and hot-files panels are hidden. The viz bundle size check (2.5 MB ceiling) still passes.
- [ ] **Step 5: Commit.** `feat: export a self-hostable static graph viewer`

### Task 6: Docs

**Files:**
- Create: `docs/EXPORT.md`
- Modify: `docs/REFERENCE.md` (export flags, exit codes), `docs/VIZ-ROADMAP.md` (link), `README.md` (one line + link)

- [ ] **Step 1:** Write `EXPORT.md`:
  - format table: use case, command, consumer;
  - link to the schema;
  - a 15-line d3-force snippet that loads `graph.json`;
  - a Mermaid-in-PR example;
  - hosting the bundle on GitHub project Pages (served under `/<repo>/`; works because every URL is relative, per T5 `test_bundle_subpath.py`);
  - the privacy note (no source text; paths and names included).
- [ ] **Step 2: Run** `typos docs/` and check the links with `lychee docs/EXPORT.md` if available. Expected: clean.
- [ ] **Step 3: Commit.** `docs: graph export guide`
