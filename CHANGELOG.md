# Changelog

All notable changes to GitCortex are documented here.

## [0.6.0] - 2026-06-30

### Added
- **Structural Markdown ingestion (no LLM).** `.md`/`.markdown` files are now
  indexed: headings become `Section` nodes (nested via `Contains` edges), and
  inline code-spans/link text matching identifier shape produce `References`
  edges from the section to the referenced code symbol. Cross-language by
  design — a README can reference any language's symbols. SCHEMA_VERSION
  bumped to 12 (triggers a one-time full re-index).
- **`find_god_nodes` MCP tool + `gcx query find-god-nodes` CLI.** Surfaces
  high-fan-in hub symbols ranked by inbound `Calls` in-degree, deterministic
  across re-runs. Refactors the existing `tour.rs` centrality into a shared
  `centrality.rs` helper.
- **`find_clusters` MCP tool + `gcx query find-clusters` CLI.** Synchronous
  label-propagation community detection over `Contains`+`Calls` undirected
  adjacency. Deterministic (fixed visiting order by qualified_name, lowest-
  qualified_name tie-break). Members per cluster capped at 25; `size` stays
  honest. Zero new dependencies, no LLM calls.
- **Viz accessibility.** `F`/`Space` keyboard shortcuts now wired (were
  documented but not implemented). Icon-only canvas-control buttons have
  `aria-label`. `SearchPalette` uses a proper ARIA combobox pattern
  (role=combobox, role=listbox, role=option, aria-selected). `Inspector` tabs
  have role=tablist/tab/tabpanel/aria-selected; depth selector has
  role=radiogroup/radio/aria-checked. `CosmosCanvas` has an `sr-only`
  description pointing screen-reader users to the search palette.
- **Hub-node overlay in `gcx viz`.** Press `G` (or click the Hubs button in
  the header) to highlight high-fan-in symbols in cyan. Powered by a new
  `/api/god_nodes` endpoint that computes `Calls` in-degree server-side.
  Pairs with the `find_god_nodes` MCP tool — both expose the same centrality
  analysis, one for AI assistants, one for visual exploration.
- **pip-first install path.** README reordered — pip/pipx/uv now leads the
  Installation section ahead of binary downloads. Infrastructure unchanged.
- **Windows (via WSL2) documentation.** README now clearly states that native
  Windows is blocked (KuzuDB MSVC linker bug, upstream archived), and that
  WSL2 + the Linux binary is the supported Windows path today.

### Changed
- `NodeKind::Section` and `EdgeKind::References` added to the graph schema.
  `section` color in the viz: Catppuccin pink (`#f5c2e7`).
- `--color-text-dim` in the viz dark theme corrected from `#5a5a72` (~3:1 WCAG
  AA fail) to `#7e7e99` (~5.1:1, passes WCAG AA).
- Dead `.gitcortex/config.toml` documentation removed from README — the file
  was documented but never parsed anywhere in the codebase.

### Fixed
- **HTML viz export XSS.** `gcx viz --format html` embedded graph JSON into a
  `<script>` block without escaping `</` sequences. A node name or file path
  from an untrusted cloned repo containing `</script>` would break out and
  inject HTML/JS into the exported file when opened in a browser. Fixed with a
  one-line `</` → `<\/` escape. Regression tests added for this and the
  Cypher/DOT/SVG/GraphML export paths (all already correctly escaped).
- `branch_esc` in `build_html` upgraded from ad-hoc `"` escaping to a full
  HTML-safe escape via the existing `svg_escape` helper (defense-in-depth).

## [0.5.1] - 2026-06-21

### Fixed
- **fastembed cache leak (P0):** model weights (`.fastembed_cache/`, ~23 MB) were
  written into the developer's repo root on every `gcx serve`. Cache now lives at
  `$XDG_DATA_HOME/gitcortex/models` — fully machine-local, invisible to developers.
  Added `.fastembed_cache/` to `.gitignore` and `.gitcortex/ignore` as a backstop.
- **Semantic index version check:** format version was silently ignored on load, so
  changing the node text representation had no effect. Version mismatches now force
  a clean rebuild. Format version bumped to 2.

### Changed
- **Richer semantic embeddings:** `node_text` now appends identifier-tokenised words
  (CamelCase/snake_case split into lowercase tokens) alongside the qualified name and
  signature. NL queries like "validate token" now match `validate_token` without
  relying on the model to unsplit glued identifiers.
- **Scaled semantic scoring:** semantic hits are scored by actual cosine similarity
  mapped to `[40‥70]` instead of a fixed 45. A cosine-0.95 hit ranks near a prefix
  match; a cosine-0.51 hit ranks below token matches — proportional confidence.
- **Dedup by node ID:** semantic hits were previously deduplicated by symbol name,
  silently dropping same-named symbols from different modules. Dedup is now by
  qualified name, so all variants surface.

## [0.5.0] - 2026-06-18

### Added
- **7 new MCP tools** (15 → 22): `graph_stats` (per-kind node/edge counts),
  `ast_search` (structural filter by kind/async/visibility/complexity/annotation),
  `type_hierarchy` (supertypes + subtypes), `find_importers`, `find_type_usages`,
  `module_dependencies`, and `get_call_sites` (caller + exact call line).
- **Semantic search** — local embeddings (AllMiniLM-L6-v2 via fastembed),
  merged into `search_code` with graceful text-only fallback.
- **Richer graph data:** cyclomatic complexity (all 5 languages),
  decorator/annotation metadata (queryable even for external decorators),
  exact call-site lines, and **edge confidence** (extracted vs inferred).
- **Configurable response token budget** (`GCX_RESPONSE_BUDGET`, default 2000) —
  every list tool truncates to fit, so a high-fan-out symbol never out-costs grep.
- No-seed `start_tour` now emits a component-level **architecture summary**
  (files grouped by directory, key symbols with `file:line`, cross-component deps).

### Changed
- **Search rewrite:** CamelCase/snake_case tokenisation, token-overlap scoring,
  Levenshtein typo tolerance, revised ranking ladder (exact > prefix > semantic
  > substring).
- Rust files now get a file-level module node (consistent with the other 4
  languages) so imports attach to a real node.
- Schema version 6 → 11 (auto-wipes + re-indexes on first run).
- **Honest benchmark methodology:** median-of-N with rate-limit retries,
  throttling, and errored-session exclusion. Reported result is a net
  **+7.7 % token saving** (geomean 1.06×), with `search_code` at 1.30× and ~half
  the turns of grep — replacing earlier single-run numbers that were too noisy.

### Fixed
- Rust `Imports` edges were silently dropped (placeholder source id → dangling
  edge); they now attach to the file module node and persist.
- Search handled neither space-separated multi-token queries nor typos.
- Semantic search hits were resolved by name instead of id, dropping every hit.

## [0.3.0] - 2026-05-27

### Added
- **Discovery surface:** `gcx query wiki` (markdown symbol page), `search`
  (ranked fuzzy), and `tour` (centrality-ranked or seeded codebase walk),
  exposed as MCP tools (`wiki_symbol`, `search_code`, `start_tour`) and slash
  commands.
- **Two more languages:** Go and Java parsers (now Rust, Python, TS/JS, Go,
  Java) with a documented coverage matrix in the README.
- **Cosmograph visualizer** (`gcx viz`) — GPU graph viewer with search,
  inspector, density modes, branch-diff overlay; Host-header allowlist guards
  against DNS rebinding.
- **`gcx export --format json`** — committable, CI-consumable symbols+edges.
- **`gcx export --claude-md`** — idempotent top-symbol table injected into
  CLAUDE.md for zero-tool-call context.
- `DefinitionText` (signature, body, doc-comment, byte range) captured per node.

### Changed
- **Full index ~100× faster** — CSV `COPY` bulk load, O(E) edge dedup, and a
  call-resolution fan-out cap. Django (520k LOC): 413s → ~4s.
- Symbol resolution is kind-ranked (a type wins over a same-named method/file),
  so `wiki <Type>` resolves correctly on Go/Java.
- Schema version bumped to 6 (auto-wipes + re-indexes on first run).

### Fixed
- MCP server stayed up only for the `initialize` response (missing
  `.waiting()`) — all subsequent tool calls now work.
- Multi-line docstrings collapsed in storage (Kuzu escape round-trip).
- TypeScript visibility now reflects `export`; Python captures all module-level
  bindings (not just ALL_CAPS); Java `find-implementors` resolves generic
  `extends Foo<T>`.

### Distribution
- Published to crates.io (6 crates), npm, and PyPI on tag; pre-built binaries
  for macOS (arm64/x86_64) and Linux (x86_64/aarch64). Windows dropped
  (KuzuDB/MSVC link incompatibility).

> 0.2.x was an internal iteration line; its changes are folded into 0.3.0.

## [0.1.0] - 2026-04-30

Initial release.

### Features

**Incremental indexing**
- tree-sitter AST parsing for Rust, TypeScript, Python, and Go
- Indexes only changed files on every commit — <500ms on typical diffs
- Branch-namespaced graph: switching branches instantly gives you that branch's graph

**Graph schema**
- Node kinds: File, Folder, Module, Struct, Enum, Trait, TypeAlias, Function, Method, Constant, Macro
- Edge kinds: Contains, Calls, Implements, Uses, Imports
- Cross-file edge resolution for all edge kinds

**Git hooks (drift-proof)**
- `post-commit`, `post-merge`, `post-rewrite`, `post-checkout` installed by `gcx init`
- Hook prints a live graph summary after each commit

**CLI commands**
- `gcx init` — install hooks, run initial index, register MCP server globally
- `gcx hook` — incremental update triggered by git hooks
- `gcx serve` — MCP server on stdio
- `gcx query` — one-shot CLI queries (lookup-symbol, find-callers, list-definitions)
- `gcx viz` — interactive force-directed graph in the browser; DOT export
- `gcx blast-radius` — BFS transitive caller risk report (text / github-comment / json)
- `gcx export` — writes `.gitcortex/context.md` codebase map
- `gcx status` — node and edge counts by kind
- `gcx clean` — wipe graph store for fresh re-index

**MCP server**
- 4 tools: `lookup_symbol`, `find_callers`, `list_definitions`, `branch_diff_graph`
- Registered globally in `~/.claude.json` — works across all Claude Code sessions
- 4 agent skills and 4 slash commands installed into `.claude/`

**CI integration**
- `gcx init --ci` writes `.github/workflows/gcx-blast-radius.yml`
- Posts blast-radius report as a sticky PR comment on every pull request
