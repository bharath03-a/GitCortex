# Changelog

All notable changes to GitCortex are documented here.

Entries for 0.2.0, 0.2.1, 0.2.2, 0.4.0, 0.6.2, and 0.6.3 were reconstructed
from release tags and commit history after the fact, so they summarise what
shipped rather than what was written at release time. Dates for those entries
are tag dates.

## [0.8.0] - 2026-11-25

**Upgrade note (schema v15):** Existing pre-0.8.0 stores require a one-time rebuild via `gcx init` (or `gcx init --editor none` for CLI-only).

### Added
- **Storage lifecycle & compaction commands (`gcx storage`):** Inspect store ownership and disk footprint with `gcx storage list` (`--all`, `--json`), preview and remove orphaned stores or backups with `gcx storage prune`, reclaim deleted local branches and stale detached snapshots with `gcx storage gc`, and compact `graph.kuzu` payloads via atomic swap with `gcx storage compact`. All destructive operations are dry-run by default and require `--apply`. Verified **83.4–84.6%** disk reclaim across 10 real repositories after 5 deleted feature branches (e.g. `django` `676.2 → 104.1 MiB`, `moby` `852.6 → 139.2 MiB`).
- **Branch indexing from git objects without checkout:** Index any local branch directly from git objects (`a5b790e`), enabling branch-aware comparisons and exact-revision PR impact workflows without mutating the working tree.
- **Persisted uncommitted-edit overlay checkpoint & trust readiness contract:** Every query and export reports explicit index readiness (`ready`, `stale`, `missing`, `degraded`), tracks dirty/overlay state across uncommitted edits, and refuses silent false-fresh or empty-as-safe results. Verified across live MCP and commit-path rebuild-equivalence suites.
- **Token-efficient progressive code search & typed question planning:** Compact/full modes, structured filters, pagination, and envelope token-budget enforcement across deterministic search and graph traversal (`45/45` pass across 10 repos; **Hit@1 = 100%**, **MRR = 1.000**, **NDCG@10 = 0.971**, **File Recall = 1.000**).
- **Portable graph exports (`gcx export`):** Readiness-checked, scoped exports in versioned GitCortex JSON (`--format graph-json`, schema v1) and Mermaid (`--format mermaid`) with shared exit codes and `docs/EXPORT.md` guide.
- **Shared cross-editor Agent Guide & bounded MCP onboarding:** `gcx init` installs a single shared guide across Claude Code, Codex CLI, Cursor, Windsurf, Copilot, and Antigravity, sends bounded server instructions on MCP `initialize`, and emits bounded teaching hints with a long-path daemon socket fallback. In the 48-run unforced adoption study, installing the guide lifted voluntary graph adoption from `40%` to `100%` (`16/16` pass) with `0` false graph calls on restraint tasks.
- **Hardened PR blast-radius workflow (`gcx init --ci`):** Exact-SHA base/head indexing via `GCX_BRANCH_OVERRIDE`, pinned action SHAs, read-only analysis job with `persist-credentials: false`, and same-repo sticky PR comments.
- **Benchmark harness & release evidence archive:** Added Codex CLI adapters across deterministic, paired-agent, competitor (Graphify `0.9.44`, CodeGraph `1.5.0`), unforced adoption, and continuous-coding lanes, archived with SHA-256 provenance under `docs/benchmarks/v0.8.0/`.

### Changed
- **Up to >118× faster cold full indexing & sub-second incremental updates:** Reconciling fresh full-index edges in memory and bulk-copying reference facts cut `ripgrep` cold indexing from `22.07s → 1.68s` (**13.1× faster**) and `django` (3,049 files, 48.5k nodes, 267.5k edges) from `>600s` timeout to `5.06s` (**>118× faster**). Fast incremental reference reconciliation and in-place surviving node updates brought single-file `gcx hook` updates to a **821 ms** median across 10 real repos.
- **Readable `gcx viz` typography & computed-style tests:** Tokenized typography across the Explorer shell, rail, header, graph labels, and detail panels with automated computed-style regression gates.
- **Redesigned GitHub Pages & collapsible release log:** Unified navigation bar with live GitHub star badge, six light-theme self-hosted SVG benchmark charts with plain-English reader's guides, a 30-second 1080p light-theme realistic workflow demo, and a collapsible changelog with only the latest release expanded by default.

### Fixed
- **Cross-file evidence diversity in `find_callers` and `symbol_context`:** Diversified returned relation evidence across files so large hub symbols no longer starve callers from other modules.
- **Python class instantiation edges & envelope token budgets:** Fixed missing Python constructor/instantiation call edges and strictly enforced MCP response token budgets.
- **Truthful startup, status, and editor diagnostics:** `gcx serve` surfaces the actual startup cause instead of opaque `CONNECTION_CLOSED` failures; `gcx status`, `gcx doctor`, and `gcx blast-radius` accurately resolve symbolic refs, distinguish commit currency from working-tree edits, explain busy-graph recovery steps, and diagnose installed editor registrations reliably.
- **`gcx viz` asset and manifest loading:** Fixed font-route registration, stuck `"Reading graph manifest..."` state, and false `"No indexed symbols"` empty-state display.
- **Security advisories & dependency hygiene:** Updated `url` (`RUSTSEC-2026-0592`), `minimatch`, `eslint`, `typescript-eslint`, and `pytest`, added CodeQL scanning across Rust/JS/TS/Python/Actions, and added advisory-ignore drift checks.

## [0.7.6] - 2026-08-25

### Fixed
- **`gcx init --editor antigravity` never registered the MCP server without `--global-editor-config`**, leaving the assistant with no tool access by default even though skills, rules, and hooks were written. Antigravity's real customization format is a self-contained plugin bundle (`.agents/plugins/gitcortex/{plugin.json,mcp_config.json,hooks.json,rules/AGENTS.md,skills/}`), confirmed against the `agy` CLI's own embedded docs; MCP config is now written project-locally and unconditionally. `gcx deinit` cleans up the new bundle. Global config (`~/.antigravity/mcp.json`, `~/.gemini/config/mcp_config.json` for the standalone `agy` CLI) remains opt-in behind `--global-editor-config`.
- **The interactive editor prompt asked every single `gcx init` run**, including runs that only rebuild the graph store after a schema bump. The choice (including "none") is now persisted per repo per machine, so it only asks once; add an assistant later with `gcx init --editor <name>`.
- **The prompt only let you pick one assistant**, even though using more than one at once (e.g. Claude Code and Antigravity together) is common. Switched from single-select to a checkbox multi-select (space to toggle, enter to confirm).

### Changed
- `gcx init` with no `--editor` in a real terminal now shows a `[GCX]` banner and a checkbox editor picker instead of a typed free-text prompt; a spinner covers the initial index (previously a silent terminal for several seconds); the closing summary is colorized. All of it respects `--color never` / `NO_COLOR` like the rest of the CLI.
- README and GitHub Pages install instructions updated to describe the one-time interactive setup instead of "editor config is a separate opt-in step."

## [0.7.5] - 2026-08-23

### Fixed
- **Claude Code skills were never actually discoverable.** `gcx init --editor claude` wrote them to `.claude/skills/gcx/<name>.md`, but Claude Code only discovers skills at `.claude/skills/<name>/SKILL.md` with YAML frontmatter — none of the four shipped skills (exploring, debugging, impact-analysis, refactoring) were ever picked up. Fixed, and the same corrected skill set is now shared with Antigravity's onboarding (see Added).
- **Bulk indexing crashed on files containing multiline quoted string literals** (e.g. TypeScript template literals, multi-line docstrings) — KuzuDB's `COPY` doesn't default its CSV `ESCAPE` character to match our RFC-4180 quoting, so `csv_quote()`'s doubled-quote escaping was rejected on the first full index of any repo with this kind of source.
- **Risk labels (`find_callers`, `blast-radius`, and the viz UI) didn't distinguish call-graph confidence.** A symbol with 1000+ low-confidence "inferred" callers (common on generic method names like `filter` or `get` in large repos) and one with the same count of high-confidence "extracted" callers both reported the same risk band — misleadingly inflating risk on exactly the kind of common, overloaded names most likely to trip it. Risk is now confidence-weighted (extracted × 1.0, resolved × 0.6, inferred × 0.3) at all three call sites, kept in sync.

### Added
- **Antigravity now gets full onboarding**, not just an MCP config entry: `.agents/rules/AGENTS.md`, four `.agents/skills/<name>/SKILL.md` skills, and a `.agents/hooks.json` PreToolUse hook that injects call-graph context on file reads — matching what Claude Code already had.
- **Codex gets a PreToolUse hook** (`.codex/hooks.json`) doing the same file-read context injection, alongside its existing `AGENTS.md` and MCP config.
- **`gcx blast-radius` reports per-item risk** for every affected caller, not just one aggregate risk band for the whole change.
- **Express.js route detection**: `app.get()`/`app.post()`-style registrations are now indexed as `Route` nodes linked to their handlers via a `HandledBy` edge, surfaced through `ast_search`.
- **Java**: enum and record annotations are now extracted; `static final` fields are indexed as `Constant` nodes (previously invisible to the graph).
- Release artifacts are now signed with cosign (keyless).

### Performance
- The viz UI's graph filter/decode pipeline moved off the main thread into a Web Worker, reusing one worker instance for the component's lifetime instead of re-instantiating per filter toggle.
- Route detection's AST walk is now skipped entirely for files with no Express-style verb-call patterns, avoiding a redundant full-file traversal on non-route files.

## [0.7.3] - 2026-08-20

### Fixed
- **Every CLI command defaulted `--branch`/`--base` to the literal string
  `"main"`**, hardcoded at the clap arg level, instead of detecting the
  repo's actual current branch. On any repo whose default branch is named
  something else (`master`, `trunk`, `develop`, ...) — extremely common —
  `gcx viz`, `gcx status`, `gcx query *`, and `gcx blast-radius` all
  silently queried a branch with zero data and reported empty results,
  even immediately after a successful `gcx init`. The MCP server already
  did this correctly (`detect_current_branch`); the CLI's 16 affected
  flags now use the same git-detected default. Reported by a user who ran
  `gcx init` in a fresh project and got "No indexed symbols on this
  branch" in `gcx viz`; reproduced (repo on `master`, indexed fine,
  viz's `/api/branches` reported `active: "main"` regardless) and
  verified fixed before releasing.

## [0.7.2] - 2026-08-20

### Fixed
- **`gcx viz` (installed via pip) threw a Python traceback on Ctrl-C** instead
  of exiting cleanly. The PyPI wrapper's `subprocess.call` didn't catch
  `KeyboardInterrupt`; the Rust binary itself already exits fine on SIGINT.
  Now exits quietly at the conventional 128+SIGINT code. Reported by a user
  hitting it in a fresh, unindexed project.

## [0.7.1] - 2026-08-12

### Security
- **`dompurify` XSS advisory** (GHSA-55q2-fjhq-7xh7, medium severity):
  pinned via an npm override to 3.4.13 — the transitive dependency (through
  `@cosmograph/react`) was on the vulnerable 3.4.12. Requires a non-default
  `IN_PLACE` + element-removal-hook configuration to exploit; viz doesn't use
  that pattern, but patched regardless.
- Two unrelated high-severity `npm audit` findings (`brace-expansion` DoS,
  `nanoid` DoS) resolved via `npm audit fix`.

## [0.7.0] - 2026-08-12

Release driven by a full 3-client (Codex, Claude Code, Agy/Antigravity) live-agent
benchmark against 15 new harder task types (deep blast-radius, cross-file
architecture, refactor-safety) across 5 pinned repos. Measured token savings:
3.27x (Codex), 3.26x (Claude Code), 1.89x (Agy), with quality held or improved
on nearly every task.

### Fixed
- **`symbol_context` (MCP + CLI) had no ambiguity handling.** A common short
  name (e.g. `beginArray`, matching 4 methods in one benchmark repo) could be
  silently resolved to the wrong symbol. It now goes through the same
  `resolve_symbol` candidate-surfacing path as `find_callers`/`get_subgraph`,
  and its response is built from `get_subgraph_by_id` so callers/callees/
  used_by are scoped to the exact resolved node rather than by name.
- CLI `symbol-context`/`trace-path` gained `--format agent-json
  --budget-tokens`, matching every other `gcx query` subcommand.

### Added
- `agy` (Antigravity CLI) MCP handshake compatibility, plus `gcx init`/
  `deinit`/`doctor` support for its config path (shipped as #66/#67, first
  release to include it).
- `docs/benchmarks.html`: real, measured benchmark numbers on the public site.
- Benchmark harness: a working `agy` client, 15 new task definitions (impact/
  architecture/refactor x 5 repos), and per-arm failure isolation so one slow
  or hung client call no longer aborts an entire run.
- Viz: the repository name is now shown in the header instead of a generic
  "Local code atlas" label; a loading indicator now covers Cosmograph's
  multi-second init so a slow first paint doesn't look like a broken canvas.

## [0.6.3] - 2026-07-12

Pure refactor release. No behaviour change from 0.6.2.

### Changed
- **`tools.rs` split** into `params`, `helpers`, and `git_helpers` modules —
  the MCP tool surface had outgrown a single file.
- Canonical `FromStr` implementations for `NodeKind` and `Visibility` moved
  into `gitcortex-core`.
- Versions bumped across `Cargo.toml`, the npm packages, and the Python sdist.

## [0.6.2] - 2026-07-07

Released from `main` without its own tag; recorded here for completeness.

### Fixed
- **Section nodes no longer pollute graph traversal.** A README heading sharing
  a name with a class (for example a `Gson` section) dragged 2,000+ extra nodes
  into `get_subgraph` responses and triggered 20+ turn exploration spirals.
  `get_subgraph` now strips `Section` nodes from both traversal and seed
  resolution, and `lookup_symbol` filters them from results.
- **Definitive stop-searching responses.** A missing `get_subgraph` seed
  returned an empty result, which models read as "try again". `get_subgraph`
  and `find_callers` now return an explicit terminal message, so a leaf
  function no longer costs 20–33 turns of exploration.
- **Benchmark symbol selection.** `pick_symbols()` used a regex that never
  matched the markdown tour format, so every session silently fell back to the
  symbols `Main` and `init`, which exist in none of the benchmark repos. This
  was the root cause of benchmark regressions previously attributed to product
  changes.

### Added
- Pre-traction versioning rule in `RELEASING.md`.

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

## [0.4.0] - 2026-06-06

### Added
- **New `gcx viz --format` targets.** `html` writes a self-contained
  vis-network page that opens offline and can be shared or embedded in docs;
  `svg` writes a static kind-grouped concentric layout for pasting into
  Markdown, PRs, and issues; `graphml` imports into Gephi, yEd, and Cytoscape;
  `cypher` emits Neo4j bulk `CREATE` statements. The existing `web`
  (Cosmograph WebGL) and `dot` (Graphviz) formats are unchanged.
- **Token-savings benchmark harness** asking seven realistic developer
  questions per repository.

### Fixed
- **TypeScript and Go interfaces are `NodeKind::Interface`**, not `Trait`.
  Rust keeps `Trait`. The store's `deferred_uses` Cypher filter matches the
  `interface` kind, and tour scoring ranks `Interface` in the same tier as
  `Trait`.
- **Java nested records are indexed** — `visit_record_nested` now runs in both
  class and nested-class bodies.

### Changed
- PyPI and npm publishing auto-trigger from the release workflow, using
  `manylinux2014` platform tags for Linux wheels.

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

## [0.2.2] - 2026-05-05

### Fixed
- Pinned `Cargo.lock` to restore a working `cxx-build` version.

## [0.2.1] - 2026-05-05

### Changed
- Added the `readme` field to the crates.io manifest.

## [0.2.0] - 2026-05-04

### Added
- **MCP depth.** `find_callers` takes a `depth` parameter (1–5) for multi-hop
  BFS call-graph traversal, and `lookup_symbol` takes `fuzzy` for substring
  matching. Two new tools: `context` returns definition, callers, callees, and
  used-by in one call; `detect_changes` maps a staged or `HEAD` diff to the
  affected symbols and a risk assessment.
- **Language parity — every parser now emits `Uses`, `Implements`, and
  `Imports` edges.** Python gains type annotations, base-class `Implements`,
  import statements, and decorators. TypeScript/JavaScript gain
  `extends`/`implements` clauses, type annotations, named imports, and
  decorators. Go gains parameter and return types, structural interface
  assertions, import declarations, and interface method signatures. Java is a
  new parser covering classes, interfaces, enums, records, `extends` and
  `implements`, field types, annotations, and imports.

### Changed
- Published crate renamed from `gitcortex-mcp` to `gitcortex`.
- Crate descriptions and keywords reworked for crates.io discoverability.
- `Cargo.lock` and the generated `context.md` are tracked; local Claude
  settings are ignored.

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
