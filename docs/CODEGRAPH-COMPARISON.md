# CodeGraph Comparison

Competitive analysis of [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph)
(67,583 stars as of 2026-08-21) against GitCortex (~4 stars). Written to ground a
prioritized improvement list in verifiable facts on both sides — not vibes.

Every CodeGraph claim is cited to its public source. Every GitCortex claim is cited
to a local file/line. Where a source contradicts itself or where I couldn't verify
something (e.g. whether the "kernel" source is actually present in the repo, versus
only a compiled artifact), that is stated explicitly rather than assumed.

---

## 1. CodeGraph — technical approach

### 1.1 Graph data model & storage engine

CodeGraph stores its graph in a **local SQLite database** with FTS5 full-text search,
at `.codegraph/codegraph.db`, described as "100% Local... symbols · edges · files"
with "No data leaves your machine."
Source: [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) (README, fetched 2026-08-21).

This is a materially different storage choice from GitCortex's embedded **KuzuDB**
(a native graph database with Cypher query support) — CodeGraph is relational
tables + full-text index, not a graph-native store. Graph traversal (callers/callees,
call paths) is therefore computed over SQL joins/recursive queries rather than native
graph traversal primitives.

### 1.2 Is the "kernel" open source?

**Unconfirmed / likely not fully open.** The README repeatedly says "Kernel powered
by Rust" and "native Rust kernel," and claims "CodeGraph bundles its own runtime —
nothing to compile, no native build, works the same everywhere." But fetching the
README directly (raw.githubusercontent.com) found **no `src/` or `crates/` paths
referenced anywhere in the document**, which is the strongest available signal that
the distributed artifact is a prebuilt/vendored binary wrapped by an npm/shell
installer, not Rust source you can read or build from this repository. I could not
get a directory listing of the actual repo tree (GitHub API calls from this
environment failed/timed out), so this is inferred from the README's own silence
on source layout, not confirmed by inspecting file contents directly.
Source: [raw.githubusercontent.com/colbymchenry/codegraph/main/README.md](https://raw.githubusercontent.com/colbymchenry/codegraph/main/README.md) (fetched 2026-08-21).

This matters directly for the comparison: GitCortex is source-available end to end —
every crate (`gitcortex-core`, `gitcortex-indexer`, `gitcortex-store`, `gitcortex-mcp`)
is MIT-licensed Rust in this repo (`Cargo.toml:1-16`, `README.md:37`). If CodeGraph's
core extraction engine is a closed binary, "open source" claims about it are, at
best, only true of the CLI/installer wrapper — a distinction worth stating plainly
in any public comparison rather than repeating their framing uncritically.

### 1.3 Indexing strategy

Hybrid full + incremental:
- `codegraph init` performs a full extraction and "builds the full graph in the same step."
- Ongoing changes are captured by a **native OS file watcher** (FSEvents / inotify /
  ReadDirectoryChangesW) with a **debounced auto-sync, default 2000ms** (tunable).
Source: [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) README (fetched 2026-08-21).

Compare: GitCortex's watcher debounces at **500ms** (`crates/gitcortex-mcp/src/mcp/watcher.rs:16`
`const DEBOUNCE_MS: u64 = 500;`), and its git-hook path (the primary sync mechanism
when no daemon is running) targets **sub-500ms incremental updates on changed files**
(`README.md:41`, `docs/ARCHITECTURE.md` data-flow section: steps 4–8). GitCortex also
syncs on git operations directly (`post-commit`, `post-merge`, `post-rewrite`,
`post-checkout` hooks — `docs/ARCHITECTURE.md`), not only on filesystem events, so
branch switches and merges are captured even if a live daemon isn't watching.

### 1.4 Language support mechanism

Tree-sitter based, same technique as GitCortex: "Fast, incremental parsing across
20+ languages — accurate symbols and edges drawn from real ASTs, not guesses."
Source: [colbymchenry.github.io/codegraph](https://colbymchenry.github.io/codegraph) (fetched 2026-08-21).

The README claims a longer list — **32 languages** (TypeScript, JavaScript, ArkTS,
Python, Go, Rust, Java, C#, PHP, Ruby, C, C++, Objective-C, Metal, CUDA, Swift,
Kotlin, Scala, Dart, Svelte, Vue, Astro, Liquid, Pascal/Delphi, Lua, R, Luau, CFML,
COBOL, Visual Basic .NET, Erlang, Solidity, Terraform/OpenTofu, Nix) — with claimed
"full structural extraction and cross-file resolution into one graph" and
per-language coverage benchmarked at 73.8%–100% across their test repos, plus
dynamic-dispatch call-path hops.
Source: [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) README (fetched 2026-08-21).

They also claim **framework-aware route detection** across 17 frameworks (Django,
Flask, FastAPI, Express, NestJS, Laravel, Rails, Spring, SvelteKit, Nuxt, Astro
file-based routing, etc.), emitting `route` nodes linked to handler
functions/classes, and **cross-language bridging** synthesis (Swift↔Objective-C via
`@objc`, React Native's legacy bridge/TurboModules, Expo Modules DSL, native event
emitters, Fabric/Paper view linking).
Source: [raw.githubusercontent.com/colbymchenry/codegraph/main/README.md](https://raw.githubusercontent.com/colbymchenry/codegraph/main/README.md) (fetched 2026-08-21).

Compare: GitCortex ships **5 languages** — Rust, Python, TypeScript/JavaScript, Go,
Java (`README.md:38`, `Cargo.toml:33-38` lists `tree-sitter-rust/python/typescript/javascript/go/java`)
— with an explicit, honestly-graded coverage matrix per language (`README.md:640-651`
"Supported languages" table) rather than a single aggregate coverage percentage. Call
resolution is stated as **syntactic, no type inference**: ambiguous same-named calls
are deliberately left unlinked rather than fanned out (`README.md:654`,
`docs/AGENT-BENCHMARK-PLAN.md`). GitCortex has **no framework-aware route detection**
and **no cross-language bridging** — confirmed absent by grep across
`crates/gitcortex-indexer/src/parser/*.rs` and the MCP tool surface; there is no
`route` node kind or framework-decorator handling in any parser
(`crates/gitcortex-indexer/src/parser/typescript.rs`, `python.rs`, `go.rs`, `java.rs`, `rust.rs`).

### 1.5 MCP tool surface — direct comparison

CodeGraph exposes **one primary tool by default**, `codegraph_explore`, described as
answering "almost any question in one call... returning the relevant symbols'
verbatim source grouped by file, plus the call paths between them and a
blast-radius summary." Seven more tools (`codegraph_node`, `codegraph_search`,
`codegraph_callers`, `codegraph_callees`, `codegraph_impact`, `codegraph_files`,
`codegraph_status`) exist but are **unlisted/disabled by default**, re-enabled via
the `CODEGRAPH_MCP_TOOLS` env var.
Source: [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) README (fetched 2026-08-21).

GitCortex's MCP surface (`crates/gitcortex-mcp/src/mcp/tools.rs`) is much larger and
more granular — **27 individual tools** plus a **single-dispatch `gcx` tool**
that wraps all of them under one schema for the compact server mode
(`README.md:596-598` "one schema covers all operations below... 27 separate
schemas"). The compact-mode default disables the 27 individual routes and exposes
only `gcx` (`crates/gitcortex-mcp/src/mcp/tools.rs:260-296`, `tool_router_for_mode`),
which is functionally the same "single entry point by default, granular tools
available" design CodeGraph uses — GitCortex arrived at the same shape independently,
via its own token-cost benchmarking (`docs/benchmarks/RELEASE-GATE.md`, "Fixed MCP
tax" section: "15 tool schemas ride in every turn... Shrinking tool-schema size...
or a single dispatch tool — lifts every cell at once").

Tool-for-tool, where CodeGraph has one bundled tool (`codegraph_explore` = source +
call paths + blast-radius summary in one shot), GitCortex splits the same
functionality across several purpose-built tools with tighter, ranked contracts:

| Capability | CodeGraph | GitCortex |
|---|---|---|
| Symbol lookup | `codegraph_node`, `codegraph_search` (unlisted) | `lookup_symbol`, `search_code` (hybrid lexical+semantic RRF — `crates/gitcortex-mcp/src/mcp/hybrid.rs`) |
| Callers/callees | `codegraph_callers`, `codegraph_callees` (unlisted) | `find_callers`, `find_callees`, `get_call_sites` |
| Pre-edit blast radius | `codegraph_impact` (unlisted) / folded into `codegraph_explore` | `pre_edit_impact` — dedicated tool, explicit tool description: "Call this BEFORE editing, renaming, or removing a function... risk_level (LOW/MEDIUM/HIGH/CRITICAL)" (`crates/gitcortex-mcp/src/mcp/tools.rs:369-378`) |
| File-level listing | `codegraph_files` (unlisted) | `list_definitions`, `list_symbols_in_range` |
| Whole-repo health | `codegraph_status` (unlisted) | `find_unused_symbols`, `find_god_nodes`, `find_clusters`, `find_cycles`, `health_report` (`README.md:614-618`) |
| Guided onboarding | not offered | `start_tour` — centrality-ranked entry points (`README.md:610`) |
| Docs generation | not offered | `wiki_symbol` — Markdown page: signature, doc-comment, top callers/callees (`README.md:611`) |
| Branch-aware diff | not offered (no evidence of branch namespacing) | `branch_diff_graph`, `detect_changes` — nodes added/removed between branches, no re-index on switch (`README.md:610`, `docs/ARCHITECTURE.md`) |
| PR/CI automation | none found in README | `gcx init --ci` writes a GitHub Actions workflow that posts blast-radius as a sticky PR comment (`README.md:546-551`, `docs/REFERENCE.md:122`) |

GitCortex's **branch-namespaced graph** (`docs/ARCHITECTURE.md`, "Per-branch graphs...
switching branches is instant, no re-index") has no stated CodeGraph equivalent in
any fetched source — CodeGraph's docs describe a single project-relative `.codegraph/`
DB with no mention of branch isolation.

### 1.6 Distribution/installer strategy

CodeGraph: `curl`/`PowerShell` standalone bundles (no Node.js required), `npm i -g`,
or `npx` one-off. Upgrade via `codegraph upgrade`. Windows supported natively via
PowerShell installer. Release artifacts carry **npm trusted publishing (OIDC,
provenance attestations)** and **GitHub bundle SLSA v1.0 Build Level 2 attestations**,
independently verifiable with `npm audit signatures` / `gh attestation verify`.
Source: [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) README (fetched 2026-08-21).

GitCortex: Homebrew, pip/pipx/uv, npm/pnpm/yarn, direct binary download + one-line
installer script, or `cargo install` — all documented with explicit commands
(`README.md:474-538`). GitCortex ships **no signed/attested releases**: grep of
`.github/workflows/release.yml` found only a checksums comment (`release.yml:193`,
"Get all the local artifacts for the global tasks to use (for e.g. checksums)") —
no `cosign`, `sigstore`, `slsa`, or `provenance` references anywhere in the release
workflow or README. **Windows is explicitly unsupported natively** — KuzuDB 0.11.3
"upstream archived Oct 2025... does not link cleanly under MSVC," WSL2 is the only
documented path (`README.md:527-534`). CodeGraph supports Windows natively via
PowerShell; this is a real gap for GitCortex, not just an optics one.

### 1.7 Hosted product roadmap ("PR impact analysis")

CodeGraph's repo advertises: "The CodeGraph platform is coming — for every PR, know
exactly what to test, what could break, which flows are affected, and whether
business logic is compromised," with a waitlist at getcodegraph.com for early beta
access. Source: [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph)
README (fetched 2026-08-21). I was unable to fetch getcodegraph.com directly
(HTTP 403), so no further detail (pricing, scope, launch date) is available from a
primary source.

**What this signals:** the gap CodeGraph is teasing — hosted, PR-level blast-radius
and test-impact analysis — is functionality GitCortex **already ships today, open
source, for free**: `pre_edit_impact` (per-function blast radius with risk levels),
`detect_changes` (changed symbols + blast radius vs. a base branch), `branch_diff_graph`,
and the `gcx init --ci` GitHub Actions bot that posts blast-radius as a sticky PR
comment (`README.md:546-551`). CodeGraph is *building toward* what GitCortex already
has running in production as an open, local-first CI bot. See §3(c) for the
recommendation this implies.

---

## 2. GitCortex — current ground truth

- **Crates:** `gitcortex-core` (types/`GraphStore` trait, zero I/O), `gitcortex-indexer`
  (tree-sitter parsing + git2 differ, sync/CPU-bound), `gitcortex-store` (`KuzuGraphStore`,
  embedded KuzuDB), `gitcortex-mcp` (async MCP handlers, daemon, watcher, semantic index),
  `gitcortex-viz` (React/Cosmograph viz, embedded via `include_bytes!`), `gitcortex-cli`
  (`Cargo.toml:5-11`, `docs/ARCHITECTURE.md` "Crate layout" table).
- **MCP tool count:** 27 individual tools (`crates/gitcortex-mcp/src/mcp/tools.rs:264-290`,
  compact-mode disable list) plus the single-dispatch `gcx` wrapper.
- **Indexing:** git-hook-triggered incremental diff-based indexing, sub-500ms on
  changed files; full index of Django (520k LOC) ~4s (`README.md:41`). Independent
  live file watcher with 500ms debounce for the MCP daemon path
  (`crates/gitcortex-mcp/src/mcp/watcher.rs:16`).
- **Languages:** 5 — Rust, Python, TypeScript/JavaScript, Go, Java. Per-language
  coverage matrix published and honestly graded, e.g. Go interface satisfaction not
  inferred, Java annotations/fields not modeled (`README.md:640-660`).
- **Blast-radius / PR bot:** `pre_edit_impact` MCP tool (risk_level LOW–CRITICAL,
  `crates/gitcortex-mcp/src/mcp/tools.rs:369-397`), `detect_changes`/`branch_diff_graph`
  tools, `gcx blast-radius` CLI command wired into a GitHub Actions workflow via
  `gcx init --ci` that posts a sticky PR comment (`README.md:546-551`,
  `docs/REFERENCE.md:122,417`).
- **Benchmarks (real, not proxy):** measured via actual Claude API `usage` tokens,
  not chars/4 estimates (`docs/benchmarks/RELEASE-GATE.md` "Why it exists"). Latest
  published real-session numbers (median of 3 runs, 5 repos × 4 questions, compact
  MCP): **+7.7% aggregate token savings, geomean 1.06×**; `search_code` wins 1.30×,
  `find_callers`/`get_subgraph` are near or slightly below break-even (0.96×/0.94×)
  (`README.md:453-461`). The v0.7.0 changelog entry separately reports a 3-client
  live-agent benchmark (Codex, Claude Code, Antigravity) across harder tasks showing
  3.27×/3.26×/1.89× savings respectively (`CHANGELOG.md`, `[0.7.0]` entry) — these
  are two different benchmark generations, not directly comparable; the README's
  numbers are the current, most-defended figures per `docs/benchmarks/RELEASE-GATE.md`'s
  explicit "only trust measured usage" stance.
- **Known internal gaps** (from `docs/benchmarks/AGENT-BENCHMARK-PLAN.md`, "Product
  strategy" and "Root-cause findings" sections, still marked in-progress): no unified
  compact response envelope across MCP/CLI yet; CLI and MCP have materially different
  response contracts (MCP is ranked/capped, CLI is raw); ambiguous symbol names are
  traversed rather than disambiguated up front in some paths; `health_report` isn't
  yet excluded from compact-mode dispatch consistently.
- **Distribution:** Homebrew, pip/pipx/uv, npm/pnpm/yarn, direct binaries + installer
  script, cargo — all no-Rust-required except source builds (`README.md:474-538`).
  No signed/attested releases (checksums only per `release.yml:193`; no cosign/SLSA
  found). Windows unsupported natively (WSL2 only, `README.md:527-534`).

---

## 3. Prioritized recommendations

### (a) Quick wins — days

1. **Publish signed/attested releases.** CodeGraph's SLSA + npm-provenance story is a
   trust signal that costs a security-conscious evaluator nothing to check and costs
   GitCortex nothing structural to add — `.github/workflows/release.yml` already
   builds and checksums artifacts; add `cosign`/`gh attestation` signing to the
   existing job. Closes a concrete, checkable gap (§1.6) with a CI-only change.
2. **Publish `gcx init --ci` as a headline feature, not a buried README subsection.**
   It is the exact capability CodeGraph is *only advertising as a future paid
   product* (§1.7). Right now it's one paragraph at `README.md:546-551`; it deserves
   a demo GIF, its own doc page, and a mention in the top-of-README highlights list
   alongside "sub-500ms incremental indexing." This is a positioning fix, not a code
   change — the leverage is high because it directly undercuts CodeGraph's most
   visible unbuilt promise.
3. **Add a one-line comparison callout in the README** ("open-source PR blast-radius
   bot today vs. a waitlisted hosted product elsewhere") — factual, cites CodeGraph's
   own public roadmap language, no disparagement needed since the facts already favor
   GitCortex here.
4. **Tighten the benchmark story's presentation.** The README's own honesty about
   variance (`README.md:459` "run-to-run variance is large... ±70pp") is a strength,
   not a weakness, if framed correctly — CodeGraph's "88% fewer tool calls" headline
   number has no visible methodology/variance disclosure in what's public. Add one
   sentence contrasting rigor, since GitCortex already has the harness
   (`docs/benchmarks/RELEASE-GATE.md`) CodeGraph's page doesn't show having.

### (b) Medium bets — weeks

1. **Windows native support.** Confirmed real gap (§1.6): KuzuDB doesn't link under
   MSVC, WSL2-only today. This is the single most concrete adoption blocker vs.
   CodeGraph, which ships a PowerShell installer. Options: (i) track KuzuDB's Windows
   fix / fork+patch the archived upstream, or (ii) — bigger lever — treat this as the
   forcing function to prototype the `GraphStore` trait against a Windows-friendly
   embedded store (the trait boundary already exists per `docs/ARCHITECTURE.md`,
   "The extensibility seam"). Medium effort because the trait seam is real, but a
   second backend is real work, not a config flag.
2. **Grow language count where it's cheap, not where it's showy.** CodeGraph's 32
   languages are mostly low-value for the target buyer (COBOL, Nix, Terraform aren't
   where AI-coding-agent usage concentrates). Adding 2-3 languages that overlap
   GitCortex's actual audience — C/C++, C#, or Ruby — following the existing
   `LanguageParser` pattern (`CONTRIBUTING.md`, "self-contained task") is more
   defensible than chasing CodeGraph's breadth. Each language is roughly the size of
   the existing `parser/go.rs` (~1,000 LOC) — days-to-low-weeks per language, not a
   quick win.
3. **Framework-aware route detection for the top 3-5 frameworks** (Express/FastAPI/
   Django at minimum) is the one CodeGraph feature (§1.4) that's genuinely useful and
   currently fully absent from GitCortex. It's incremental on top of the existing
   TS/Python parsers rather than a new subsystem — a `route` node kind plus
   decorator/call-pattern detection in `parser/typescript.rs`/`parser/python.rs`.
   Medium bet: valuable, bounded scope, doesn't touch the store schema much beyond
   one new node kind.
4. **Close the internal contract-consistency gaps already identified.** The
   `AGENT-BENCHMARK-PLAN.md` "Product strategy"/"Target response contract" work
   (unified compact envelope, ambiguity-first disambiguation, global response budget)
   is already scoped and partially designed in-repo — finishing it isn't about
   CodeGraph at all, but it directly improves the numbers (`find_callers`/`get_subgraph`
   currently near/below break-even at 0.96×/0.94×) that any future comparison piece
   will be judged on. Do this before publishing more benchmark claims externally.

### (c) Larger differentiation bets

1. **Ship the PR-impact/blast-radius wedge as GitCortex's flagship differentiator,
   before CodeGraph ships it as a paid product.** This is the biggest strategic
   opening in this whole comparison: CodeGraph, at 67k stars, is *only teasing*
   hosted PR-impact analysis behind a waitlist (§1.7) — no public timeline, no free
   tier confirmed. GitCortex already has the graph primitives for it
   (`pre_edit_impact`, `detect_changes`, `branch_diff_graph`) and a working open-source
   CI bot (`gcx init --ci`) today. The wedge is: **be the open-source, self-hosted,
   free answer to the exact product CodeGraph is building a waitlist for.** Concretely:
   - Package `detect_impact`/`pre_edit_impact`/`branch_diff_graph` into a single,
     polished "PR risk report" (risk_level rollup across all changed symbols in a PR,
     not just one function at a time) — the current tools operate per-symbol; a PR-level
     aggregation view doesn't exist yet per the MCP tool table in §1.5.
   - Make the GitHub Actions bot output richer: test-impact hints ("these test files
     exercise the changed blast radius"), not just a symbol list — this needs a
     test-file-to-symbol mapping GitCortex doesn't currently build (no evidence in
     `crates/gitcortex-indexer` of test-association tracking).
   - Market this explicitly against CodeGraph's waitlist messaging: "why wait for a
     hosted beta when the open version ships today."
   This is a larger bet because the PR-level rollup and test-impact mapping are new
   engineering, not repackaging — likely 2-4 weeks of focused work — but it's the one
   place GitCortex has a working *product*, not just a smaller version of CodeGraph's
   product, and it directly attacks the one thing CodeGraph hasn't shipped yet.
2. **Lean into "graph-native, not SQLite" as a technical differentiator once it's
   actually paying off in query depth.** CodeGraph's SQLite+FTS5 model (§1.1) is
   simpler to ship but weaker for genuinely graph-shaped queries (multi-hop
   traversal, path-finding, community detection). GitCortex already has
   `find_clusters` (label-propagation) and `find_cycles` (Tarjan SCC) — real graph
   algorithms a relational-table store makes awkward. This bet only pays off if
   GitCortex keeps deepening graph-specific queries CodeGraph structurally can't
   match cheaply (KuzuDB gives Cypher; recursive SQL CTEs get ugly fast at depth).
   Don't lead with this in marketing yet — it's a moat only once 1-2 more
   graph-native features exist that competitors visibly can't replicate quickly.
3. **Open-source-native positioning vs. an unclear-provenance "kernel."** If §1.2's
   inference holds up under further scrutiny (CodeGraph's core extraction engine not
   actually being open despite "open source" framing), that's a durable trust
   argument GitCortex can make honestly and permanently, since every GitCortex crate
   really is inspectable MIT-licensed Rust. This needs one confirming step before
   being stated publicly as fact: get an actual GitHub API directory listing of
   `colbymchenry/codegraph` (blocked in this environment) to see whether Rust source
   files are present, rather than relying on README silence alone.

---

## Sources

**CodeGraph:**
- [github.com/colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) — README, fetched 2026-08-21 (stars, forks, install methods, MCP tools, release verification, hosted-product waitlist copy).
- [raw.githubusercontent.com/colbymchenry/codegraph/main/README.md](https://raw.githubusercontent.com/colbymchenry/codegraph/main/README.md) — raw README text, fetched 2026-08-21 (language list, framework route detection, cross-language bridging, kernel/source-layout inference, benchmarking claims, telemetry).
- [colbymchenry.github.io/codegraph](https://colbymchenry.github.io/codegraph) — docs/landing page, fetched 2026-08-21 (tree-sitter parsing description, supported agent list).
- getcodegraph.com — **not accessible** (HTTP 403 on fetch, 2026-08-21); waitlist/pricing detail beyond the GitHub README's own description could not be verified from this source directly.

**GitCortex** (all paths relative to `/Users/bharathvelamala/Documents/Open Source/GitCortex`):
- `Cargo.toml` — workspace members, dependency versions (tree-sitter grammars, KuzuDB).
- `README.md` — highlights, benchmark tables, supported-languages matrix, MCP tool table, install methods, limitations/roadmap, CI bot section.
- `CHANGELOG.md` — v0.7.0–v0.7.3 entries (benchmark history, recent fixes).
- `docs/ARCHITECTURE.md` — crate layout, data flow (git-commit path, MCP query path), storage layout, `GraphStore` trait.
- `docs/benchmarks/RELEASE-GATE.md` — measured-usage benchmark methodology, "why it exists," known structural costs.
- `docs/benchmarks/AGENT-BENCHMARK-PLAN.md` — in-progress product/contract gaps, target response contract design.
- `docs/REFERENCE.md` — CLI flag reference (`--ci`, `github-comment` format).
- `crates/gitcortex-mcp/src/mcp/tools.rs` — full MCP tool list, compact-mode routing, `pre_edit_impact`/`lookup_symbol`/etc. implementations and descriptions.
- `crates/gitcortex-mcp/src/mcp/watcher.rs` — file-watcher debounce constant.
- `crates/gitcortex-indexer/src/parser/*.rs` — per-language parser files (existence/absence of route/framework detection).
- `.github/workflows/release.yml` — release artifact/checksum handling (absence of signing).
