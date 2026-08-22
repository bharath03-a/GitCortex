# GitCortex vs. CodeGraph — Head-to-Head Benchmark

> **Status:** first-pass, run in a single sandboxed session on 2026-08-21. This is a
> structural + CLI-output benchmark, **not** a measured-usage Claude-API run in the
> style of [`RELEASE-GATE.md`](RELEASE-GATE.md). See [What we could and couldn't
> measure](#what-we-could-and-couldnt-measure-and-why) for the exact scope
> limitation and why the numbers below should be read as directional, not final.

Competitor: [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph)
(npm: `@colbymchenry/codegraph`, installed version `1.5.0` at benchmark time).
Prior structural research on this tool is in
[`docs/CODEGRAPH-COMPARISON.md`](../CODEGRAPH-COMPARISON.md) — this document adds
*run* data (real install, real index, real query output) on top of that research.

---

## Methodology

Reused GitCortex's own suite wherever possible for a like-for-like comparison,
per [`tools/agent-bench/suite.toml`](../../tools/agent-bench/suite.toml) and the
task/action taxonomy in [`tools/agent-bench/README.md`](../../tools/agent-bench/README.md).

- **Repos:** `cobra` (Go, pinned `adbc8813901bba65827259daa8e22ff94ec1f30e`) and
  `requests` (Python, pinned `f361ead047be5cb873174218582f7d8b9fcd9f49`) — two of
  the five repos in `suite.toml`. `ripgrep`, `hono`, and `gson` were **not** run
  (see limitations below); cobra + requests were chosen to cover one statically
  typed and one dynamically typed language.
- **Isolation:** both tools installed and run in a scratch directory outside the
  GitCortex repo (`/private/tmp/.../scratchpad/codegraph-bench/`), never touching
  `crates/`. `gcx` used the already-built release binary
  (`target/release/gcx`, v0.7.3) already on `PATH`; CodeGraph was installed fresh
  via `npm i -g @colbymchenry/codegraph --prefix <scratch>/npm-prefix`.
- **Queries:** reused the exact `query`/`required-evidence` strings from
  `suite.toml` tasks `cobra-search-parse-flags`, `cobra-callers-add-command`,
  `requests-search-session`, `requests-callers-send` — same symbols, same repos,
  same pinned commits GitCortex already benchmarks itself against.
- **What's compared per query:** raw CLI output byte count (a byte/4-token proxy,
  the same proxy `RELEASE-GATE.md` itself explicitly flags as **not** trustworthy
  for shipping decisions — see caveat below), tool-call shape (1 call vs. N calls
  to answer the same question), and qualitative content (does the response return
  metadata+ranked evidence, or full verbatim source).
- **Timing:** `/usr/bin/time -p` wall-clock around each cold `init` and one
  single-file incremental update, 1 run each (not medianed over multiple runs —
  see limitations).

---

## Install

Both installed cleanly, no sandbox network issues:

| | GitCortex (`gcx`) | CodeGraph |
|---|---|---|
| Method | pre-built release binary already on `PATH` (also installable via Homebrew/pip/npm/cargo) | `npm i -g @colbymchenry/codegraph` |
| Install time | n/a (binary pre-existing) | ~5s (`added 2 packages in 5s`) |
| On-disk footprint of the tool itself | 43 MB (`target/release/gcx`, single static-ish binary) | **292 MB** (`npm-prefix/lib/node_modules/@colbymchenry/codegraph`, bundled Node runtime + native kernel) |
| Runtime dependency | none (native binary) | Node.js (bundled runtime, per their README claim of "nothing to compile") |

CodeGraph's install is fast and turnkey, exactly as advertised. Its footprint is
~6.8× larger on disk than the GitCortex binary because it vendors its own Node
runtime rather than shipping a single native binary.

---

## Cold index

| Repo | Tool | Wall time | Reported internal time | Nodes | Edges | Files | On-disk index size |
|---|---|---|---|---|---|---|---|
| cobra (Go) | `gcx init` | 0.37s | 281ms | 884 | 2,713 | 41 (36 .go + 5 .yaml counted by CodeGraph; gcx counts differently, see note) | 5.6 MB (single branch) |
| cobra (Go) | `codegraph init` | 2.06s | 1.1s (+ ~1s Node cold-start) | 910 | 4,246 | 41 | 3.9 MB (`DB Size` self-reported, matches `du`) |
| requests (Python) | `gcx init` | 0.43s | 325ms | 1,047 | 3,204 | — | 12 MB *(two branches indexed in this dir — see note)* |
| requests (Python) | `codegraph init` | 0.56s | 172ms | 1,299 | 2,841 | 49 | 3.0 MB |

**Notes:**
- gcx's per-repo store lives outside the repo (`~/.local/share/gitcortex/<hash>/`,
  embedded KuzuDB) and is **branch-namespaced** — each branch gets its own graph
  namespace. The 12 MB `requests` figure includes two branch namespaces (the
  pinned commit's detached-HEAD state plus a `bench-branch` created for the
  incremental test); the clean single-branch cobra figure (5.6 MB, measured
  immediately after the first `init`, before a second branch existed) is the
  fairer per-repo comparison point. This is a real architectural tradeoff worth
  flagging on its own: GitCortex's per-branch storage means a repo with many
  long-lived branches accumulates more on-disk index than CodeGraph's single
  project-relative `.codegraph/` DB, in exchange for instant branch switches
  with no re-index (documented in `docs/ARCHITECTURE.md`).
- CodeGraph's index lives inside the repo at `.codegraph/` (SQLite, `node:sqlite`
  built-in, WAL journal mode) and is not branch-aware — switching branches in a
  CodeGraph-indexed repo is not covered by anything in its CLI surface we found;
  no branch-diff or branch-switch command exists (confirmed via `--help` on every
  subcommand).
- Both tools indexed comparably-sized node/edge counts on the same source; the
  differences (884 vs 910 nodes on identical cobra source) reflect differing node
  taxonomies (`import` and `variable` are CodeGraph node kinds; gcx does not
  surface those as first-class node kinds in `status`) rather than one tool
  missing symbols outright — we did not do a symbol-by-symbol coverage audit
  (out of scope here, and already partially covered qualitatively in
  `docs/CODEGRAPH-COMPARISON.md` §1.4).

## Incremental (single-file change) re-index

Test: append one comment line to `command.go` (cobra) / `sessions.py` (requests),
commit (for gcx, which triggers via git hooks), then time the update.

| Repo | Tool | Wall time | Reported internal | Delta reported |
|---|---|---|---|---|
| cobra | `gcx hook` (same-branch, single file) | 0.35s | 232ms | `+144 nodes +443 edges -1 files` |
| cobra | `codegraph sync` (no commit needed, file-watch/manual) | 0.67s* | 122ms | `Modified: 1 — 153 nodes` |
| requests | `gcx hook` | 0.37s | 262ms | `+1103 nodes +3245 edges -129 files` † |
| requests | `codegraph sync` | 0.54s* | 81ms | `Modified: 1 — 57 nodes` |

\* CodeGraph's wall time is dominated by Node process cold-start (~300-500ms);
its internal reported sync time is consistently faster than gcx's internal
reported time in this sample.

† This requests `gcx hook` run coincided with switching to a newly created
branch namespace (see cold-index note above), so it reflects a **full re-index
into a new branch namespace**, not a pure single-file diff — an artifact of test
setup, not a fair single-file-incremental number. The cobra row is the clean
single-file-incremental sample (same branch, second edit).

**Observed quirk:** `gcx hook` printed `error: git symbolic-ref failed: No such
file or directory (os error 2)` / `warning: GitCortex index update deferred; run
gcx doctor` on **every** invocation in this sandbox, including on a normal named
branch (not just the initial detached-HEAD state) — yet the graph delta still
applied correctly per `gcx status` immediately after. This looks environment-
specific (possibly related to how git hooks resolve `HEAD` in this sandboxed
shell) rather than a real functional bug, but it's worth GitCortex maintainers
checking — a warning that fires on a success path erodes trust in the tool's own
error signal.

Both tools' incremental updates are file-granularity, not line-granularity —
appending one line still touches the whole file's node set in both tools' output,
which is expected and matches what both projects document.

---

## Query / answer comparison

Same symbols, same pinned commits, using each tool's most directly comparable
CLI output. Byte counts are **UTF-8 output bytes**, used only as a chars/4-style
proxy — see the explicit caveat in the next section before treating these as
token numbers.

| Task (from `suite.toml`) | GitCortex command | GitCortex bytes | CodeGraph command | CodeGraph bytes |
|---|---|---|---|---|
| `cobra-search-parse-flags` | `gcx query search ParseFlags --format agent-json` | 2,310 | `codegraph query ParseFlags -j` (unlisted-by-default tool) | 681 |
| " | " | " | `codegraph explore ParseFlags` (**default-enabled** MCP tool) | 11,908 |
| `cobra-callers-add-command` | `gcx query find-callers Command.AddCommand --format agent-json` | 722 | `codegraph callers AddCommand -j` (unlisted-by-default) | 2,812 |
| — (no `suite.toml` id; ad hoc impact probe) | `gcx query get-subgraph Command.AddCommand --format agent-json` | 1,531 | `codegraph impact AddCommand -j` (unlisted-by-default) | 20,686 |
| `requests-search-session` | `gcx query search Session --format agent-json` | 2,058 | `codegraph query Session -j` (unlisted-by-default) | 5,714 |
| " | " | " | `codegraph explore Session` (default-enabled) | 19,707 |
| `requests-callers-send` | `gcx query find-callers SessionRedirectMixin.send --format agent-json` | 2,231 | `codegraph callers send -j` (unlisted-by-default) | 1,409 |

### Reading this table honestly

- **`gcx search`/`find-callers` output is metadata-and-signature only** — symbol
  name, qualified name, kind, file, line, one-line doc comment, a relevance
  score. It never inlines full function bodies; the agent is expected to `Read`
  specific lines if it needs the body.
- **CodeGraph's `codegraph_explore` — the *only* MCP tool enabled by default** —
  inlines full verbatim source of every matched file ("The code below is the
  verbatim, current on-disk source... re-read from disk on this call... do not
  Read a file shown here"). That's a deliberate design choice to save a
  follow-up `Read` call, and it shows in tool-call count (see below), but it
  costs 5-10× the bytes of gcx's ranked-evidence response for the same query on
  these two repos. This is exactly the "high-fan-out dumps lose" failure mode
  `RELEASE-GATE.md` already documents internally for GitCortex's own
  `get_subgraph`/`find_callers` at depth 2 — CodeGraph's default tool exhibits
  the same pattern by design, for every query, not just deep ones.
- **Tool-call count**: a single `codegraph_explore` call answers "what is
  ParseFlags and what calls it" in one shot (search + callers + source, ~12-20 KB).
  Getting the same combined answer from GitCortex's granular tools costs 2-3 MCP
  calls (`search_code` + `find_callers`, or + `get_subgraph`) but each call is
  far smaller (0.7-2.3 KB) and returns metadata an agent can select from before
  deciding whether to `Read` a body at all. Which pattern wins on *total* tokens
  depends entirely on whether the agent actually needs the full source on that
  turn — a question this CLI-level benchmark cannot answer (that requires a real
  agent transcript, see limitations).
- CodeGraph's non-default tools (`codegraph_node`, `codegraph_search`,
  `codegraph_callers`, `codegraph_callees`, `codegraph_impact`, `codegraph_files`,
  `codegraph_status`) — reachable only via `CODEGRAPH_MCP_TOOLS` env var, not
  exposed to an agent out of the box — are more comparable in shape to GitCortex's
  granular tools and were smaller than `codegraph_explore` on 2 of 4 probes here,
  but larger on the other 2 (`callers AddCommand`: 2,812 vs gcx's 722;
  `impact AddCommand`: 20,686 vs gcx's `get-subgraph` 1,531). Sample size is too
  small (4 probes) to call a consistent winner on the granular-tool comparison.

---

## What we could and couldn't measure, and why

**Could measure (and did, above):**
- Real install of both tools in an isolated sandbox directory, no GitCortex
  source touched.
- Real cold-index time, incremental-update time, and on-disk index size for both
  tools against the same two pinned repos from GitCortex's own suite.
- Real CLI/query output for the same symbols on the same pinned commits, with
  byte counts as a rough proxy.
- Structural MCP tool-surface comparison (already done in more depth in
  `docs/CODEGRAPH-COMPARISON.md`, cross-referenced above).

**Could not measure — explicit scope cut, not a silent skip:**
- **Real measured-usage tokens via the Claude API**, i.e. the actual
  `RELEASE-GATE.md` methodology (`docs/benchmarks/tools/agent-bench/agent_run.py`
  / `real-harness.sh`, running both a baseline and graph arm through a live
  Claude session and reading `usage.{input,cache_creation,output}_tokens` off
  each). This needs a funded Anthropic API key and costs real money per
  `RELEASE-GATE.md`'s own estimate (~$1-1.5/repo on haiku). Given this session's
  cost was already flagged as over budget before this benchmark task even
  started, spinning up a second paid API-benchmarking pass was not a reasonable
  trade — the byte-count proxy above is reported explicitly as a **proxy**, with
  the same caveat `RELEASE-GATE.md` itself states about the historical
  `token-savings-v0.3.md` proxy: *"the proxy benchmark over-states savings by
  100-1000× because it assumes the baseline reads whole files... the only number
  we trust for shipping decisions is measured usage."* Treat every byte number
  in this document the same way — directional, not a shipping-decision number.
- **`ripgrep`, `hono`, `gson`** — the other three repos in `suite.toml` — were not
  indexed by either tool. Two repos (one statically typed/compiled, one
  dynamically typed/interpreted) were judged sufficient for a first directional
  pass given the cost pressure above; a full release-gate-quality comparison
  should run all five.
- **Only 1 run per timing measurement**, not medianed over 3+ runs the way
  `real-sweep.sh` does. `RELEASE-GATE.md` itself notes run-to-run variance can be
  large (±70pp) for the real-usage lane; single-shot wall-clock numbers here are
  even more exposed to that noise (disk cache state, Node cold-start jitter).
  Treat single-digit-percent differences in the timing tables as noise.
- **No native-MCP-client run** (Claude Code / Codex / Agy actually calling each
  tool's MCP server end-to-end, the way `agent_run.py` does for GitCortex). We
  ran each tool's CLI directly, which is the same underlying code path each MCP
  tool wraps, but does not capture MCP-protocol framing overhead, multi-turn
  agent tool-selection behavior, or real tool-call counts an autonomous agent
  would actually make.
- **No symbol-coverage/correctness audit** (did CodeGraph or GitCortex miss any
  ground-truth symbols/callers on these repos). We report each tool's own status
  output at face value; a `relevance.py`-style precision/recall pass (comparing
  both tools' ranked output against the `relevant_files`/`relevant_symbols`
  ground truth already pinned in `suite.toml`) is future work, not done here.

## Bottom line (from what was actually run)

- Both tools install and index cleanly in a sandbox; no reason to distrust
  either's basic functionality claims on these two repos.
- CodeGraph's install footprint is far larger (bundled Node runtime, 292 MB vs.
  GitCortex's 43 MB single binary); its cold-index and incremental-sync times
  were competitive with or slightly slower than GitCortex's on these small repos
  (cobra, requests are both under 50 files), dominated by Node process startup
  rather than indexing work itself.
- The single clearest, most reproducible finding: **CodeGraph's only
  default-enabled MCP tool (`codegraph_explore`) returns full verbatim source
  inline, which is 5-10× the byte count of GitCortex's metadata-only default
  tools for the same query** on both probed repos. Whether that's a net win
  depends on how often the agent would have `Read` the source anyway — a
  question only a real measured-usage run (not done here, see above) can
  actually answer.
