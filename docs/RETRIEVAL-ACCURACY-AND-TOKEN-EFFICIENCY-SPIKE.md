# Retrieval Accuracy & Token Efficiency Spike

Re-verification of a prior debugging session's findings against current source
(`main` at the time of writing, working tree `feat/safe-init-homebrew`), plus
independent follow-up. Same spirit as `docs/WINDOWS-KUZUDB-SPIKE.md`: this is
an investigation, not a decision. Every claim below is cited to `file:line`,
a commit, a URL, or a trace file under `tools/agent-bench/results/`. Where a
seeded number could not be independently reproduced, that is stated
explicitly rather than repeated as fact.

---

## 1. Confirmed findings

### 1.1 Confidence-blind risk banding — confirmed, and it's the same formula in three places on purpose

`find_callers` (`crates/gitcortex-mcp/src/mcp/agent.rs:360-366`):

```rust
let total = evidence.len();
let risk_level = match total {
    0..=2 => "LOW",
    3..=10 => "MEDIUM",
    11..=30 => "HIGH",
    _ => "CRITICAL",
};
```

`total` is `evidence.len()` (`agent.rs:360`), a raw count of caller nodes
across all hops — it does not weight by `EdgeConfidence`
(`crates/gitcortex-core/src/schema.rs:151-178`: `Extracted` / `Resolved` /
`Inferred`). An `Inferred` (heuristic, lowest-confidence) edge counts exactly
the same as an `Extracted` (AST-certain) edge toward the risk band.

The identical three-tier-count formula is duplicated verbatim in two more
places, and both call sites now carry an explicit code comment acknowledging
the duplication:

- `crates/gitcortex-cli/src/cmd/blast_radius.rs:113-123` — `risk_band()`,
  comment: *"Shared with `gitcortex_mcp::mcp::agent::find_callers`'s
  risk_level bucketing ... so a risk label means the same thing everywhere
  GitCortex reports one, CLI or MCP."*
- `crates/gitcortex-viz/src/lib.rs:607-615` — inline `risk_level` match,
  comment: *"Matches gitcortex_mcp::mcp::agent::find_callers and
  gitcortex_cli::cmd::blast_radius::risk_band..."*

So this is not three independent bugs — it's one intentionally-shared
formula, confidence-blind in all three places. The seeded finding's
"reportedly also applied to blast_radius.rs and viz/lib.rs" is confirmed
exactly, thresholds included (0-2 / 3-10 / 11-30 / 31+).

**The `rank_callers` claim is also confirmed** — ranking (not banding) *is*
confidence-aware. `agent.rs:890-899`:

```rust
fn rank_callers(...) -> std::cmp::Ordering {
    confidence_rank(ac)
        .cmp(&confidence_rank(bc))
        .then_with(|| candidate_rank(a).cmp(&candidate_rank(b)))
        ...
}
```

`confidence_rank` (referenced via `agent.rs:18`, tested at `agent.rs:990-994`)
orders `Extracted < Resolved < Inferred`, so `Extracted` edges sort first in
the evidence list returned to the caller. The bug is specifically that this
same confidence signal, already computed and already used for ordering, is
discarded when computing the *count* that drives the risk label — a
high-confidence-only finding of 3 extracted callers and a
low-confidence-only finding of 3 inferred callers both say "MEDIUM," even
though the second is much less trustworthy.

### 1.2 Ambiguous bare-name lookup friction — real design tradeoff, cost claim not independently verifiable from available traces

`resolve_symbol` (`agent.rs:813-843`) returns `Resolution::Ambiguous(nodes)`
when an unqualified query matches more than one symbol (`agent.rs:836`,
`agent.rs:842-843`), and all three MCP entry points that call it —
`find_callers` (`agent.rs:281`), `get_subgraph`/subgraph handler
(`agent.rs:425`), and `symbol_context` (`agent.rs:610`) — short-circuit to
`AgentStatus::Ambiguous` with a candidate list instead of an answer, forcing
the calling agent to make a second, qualified-name call.

I searched `tools/agent-bench/results/*.jsonl` for `"Ambiguous"` /
`"status": "Ambiguous"` occurrences to quantify the extra-turn cost cited in
the seeded summary. `grep -l "Ambiguous"` matched
`big-repos-validate-moby-20260822T223408Z.jsonl`,
`big-repos-validate-nextjs-20260822T222801Z.jsonl`, and
`graphify-compare.jsonl`, but none of those files contain a literal
`"status":"Ambiguous"` (or spaced variant) JSON field when grepped precisely
— the earlier hits were substring matches elsewhere in longer strings. **I
could not independently verify a concrete extra-turn/token cost for
ambiguous lookups from the trace data currently in
`tools/agent-bench/results/`.** This doesn't mean the friction isn't real —
`resolve_symbol`'s branching structure guarantees an extra round trip
whenever `qualified.len() > 1` or `exact.len() > 1` — only that I can't cite
a measured number for it right now, so the seeded per-call cost estimate
should be treated as unconfirmed until a fresh bench run tags ambiguous
resolutions explicitly.

The tradeoff itself is real, not a clear bug: `resolve_symbol` already has
the machinery to rank candidates (`candidate_rank`, `agent.rs:880-887`,
weights visibility and non-test-file status) and callers by confidence
(`rank_callers`, above). Auto-picking the top-ranked candidate on ambiguity
and returning it with alternates surfaced (rather than forcing a
disambiguation round trip) is architecturally straightforward given what
already exists, but it trades a guaranteed-correct-symbol contract for a
probably-correct one — see recommendation 3.2.

### 1.3 MCP branch-resolution issue — confirmed fixed, but the fix lives in the harness, not gcx

`git show 59de43c` confirms: the fix is entirely in
`tools/agent-bench/agent_run.py`'s `claude_dispatch()` (21 lines changed,
14 insertions / 7 deletions, single file). It hardcodes
`branch = "gcx-bench"` and threads it into every dispatch payload
(`agent_run.py`, per the diff, all six `task.action` branches). The commit
message states the root cause explicitly: *"Root cause is likely how Claude
Code spawns the MCP server for an inline `--mcp-config` vs a project
`.mcp.json`, external to gcx itself — sidestepped by passing branch
explicitly rather than relying on detection."*

That means `detect_current_branch` itself was **not** changed. Current
source, `crates/gitcortex-mcp/src/mcp/helpers.rs:94-111`:

```rust
pub(crate) fn detect_current_branch(repo_root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        ...
}
```

Still shells out to `git symbolic-ref --short HEAD` against the server
process's `current_dir`. This is unreliable specifically in the
`--mcp-config` inline-spawn topology the benchmark harness uses (Claude
Code apparently doesn't guarantee the spawned server's cwd matches the
workspace being explored in that mode); it is presumably fine when gcx is
launched via a project `.mcp.json` from within the repo, which is the normal
end-user path. **The finding as seeded ("reportedly already fixed") is
half-right: the harness's symptom is fixed, the underlying detection
mechanism in gcx is unchanged and remains fragile under the exact spawn
topology that triggered it.**

I confirmed `roots` is unused anywhere in the MCP server:
`grep -rn "roots" crates/gitcortex-mcp/` returns nothing. Per the MCP spec
(fetched 2026-08-23,
<https://modelcontextprotocol.io/specification/2025-06-18/client/roots>):
a client that supports `roots` declares the capability during
`initialize` (`{"capabilities":{"roots":{"listChanged": true}}}`), and a
server can then send a `roots/list` request to get back
`{"uri": "file://...", "name": "..."}` entries — i.e., the client's actual
workspace root(s), independent of whatever `cwd` the server process happened
to be spawned with. This is a more robust signal than `cwd` + `git
symbolic-ref` for exactly the failure mode this finding describes, *if* the
spawning client declares the capability. I could not verify from this
sandbox whether Claude Code's `--mcp-config`-based spawn path declares
`roots` support during `initialize` — that's an open question (§4).

### 1.4 CSV bulk-import crash — confirmed fixed; `PARALLEL=false` looks structurally necessary, not just cautious

`git show 361cbbf` confirms the fix: both `COPY` statements in
`crates/gitcortex-store/src/kuzu/bulk.rs` gained `ESCAPE='"'` (2 files
changed → actually 1 file, `bulk.rs`, 4 lines, 2 insertions/2 deletions).
Current source, `bulk.rs:150-161`, both `COPY` calls now read
`(HEADER=false, PARALLEL=false, ESCAPE='"')`. Commit message: Kuzu's `COPY`
doesn't default `ESCAPE` to `"`, and the project's own `csv_quote()` (RFC
4180 doubled-quote escaping, used because signatures/docstrings can contain
quotes and embedded newlines — e.g. TS template literals) produced CSV that
Kuzu's default-escape parser rejected. Reproduced against this repo's own
`crates/gitcortex-store`'s `view.ts` source snippet per the commit message.

`PARALLEL=false` (`bulk.rs:152`, `bulk.rs:159`) predates this fix and is
still hardcoded with no inline comment explaining why. I could not reach
`docs.kuzudb.com` from this sandbox (`getaddrinfo ENOTFOUND
docs.kuzudb.com` on every attempt, both `WebFetch` and follow-up domain
variants — the domain appears unreachable here, possibly related to the
archival noted in `WINDOWS-KUZUDB-SPIKE.md §2`, possibly a sandbox DNS gap;
I can't tell which). Falling back to `gh api search` against
`kuzudb/kuzu`'s issue tracker (same technique `WINDOWS-KUZUDB-SPIKE.md`
used for the same reason): **[kuzudb/kuzu#5778](https://github.com/kuzudb/kuzu/issues/5778)**
— *"export-import(csv): support parallel imports for multiline CSV files"* —
states plainly: *"Currently, the parallel CSV importer does not support CSV
files with quoted strings containing new lines."* Filed and closed
2025-09-04 (before the 2025-10-10 archival), with no linked commit in its
timeline (`gh api .../issues/5778/timeline`), so I cannot confirm from the
issue alone whether a fix actually shipped in 0.11.3 (the version
GitCortex pins, per `WINDOWS-KUZUDB-SPIKE.md §1`) or whether it was closed
without a code change.

Given GitCortex's node/edge CSVs routinely contain multiline strings
(docstrings, multi-line signatures — the exact class of data that motivated
the `ESCAPE='"'` fix above), `PARALLEL=false` is very likely load-bearing
for correctness right now, not just a conservative default: turning it on
risks reintroducing a crash class adjacent to the one 361cbbf just fixed,
on a dependency that can no longer take a bug report. I could not verify
this from KuzuDB's own docs (unreachable), so treat it as inference from
the GitHub issue evidence, not a confirmed fact — recommend re-testing on a
GitCortex-shaped CSV with real multiline fields before ever flipping this
flag (see §2.4).

### 1.5 Benchmark harness reliability

**Repo lock race (`gitcortex-store::branch::RepositoryLock`)** — **the seeded
framing appears wrong on the mechanism.** `RepositoryLock::try_acquire`
(`crates/gitcortex-store/src/branch.rs:118-139`) uses `fs2::try_lock_exclusive`
on `serve.lock` — an OS-level advisory `flock`, not a PID/stale-file scheme.
The doc comment at `branch.rs:117` states this explicitly: *"The operating
system releases it on crashes."* That is accurate for `flock`: the kernel
releases the lock automatically when the holding process's last file
descriptor closes, including on a hard crash or `kill -9` — there is no
"detect a stale lock and retry" step needed because the OS already prevents
staleness for this specific class of lock. So the seeded claim ("stale lock
not detected/retried when owning process exited") describes a bug pattern
that doesn't match how `RepositoryLock` is actually implemented; if it's
real, the failure is more likely a genuine contention race (two `gcx`
invocations legitimately racing for the same repo, e.g. `gcx clean` and
`gcx hook` in `prepare_repo` racing a leftover `gcx serve`/`gcx viz`
process from a prior run — see `serve_lock.rs:19-29`'s error path, whose
message literally says *"close editor MCP sessions and stop `gcx viz`, then
retry"*) — not a stale-lock bug in the OS-backed primitive itself.

`tools/agent-bench/bench.py`'s `prepare_repo` (`bench.py:145-170`) has
**no retry/backoff logic** around any of its subprocess calls, including
`gcx clean` and `gcx hook` (`bench.py:165-166`) — a single `require_ok`
call each, no retry wrapper anywhere in the function. So if a prior bench
run's `gcx serve`/`gcx viz` is still holding `serve.lock` when a new run's
`prepare_repo` starts, the mutation-side call fails immediately with
`serve_lock.rs:21-28`'s error and the whole task aborts — that part of the
seeded finding (harness has no retry) is confirmed as-is.

**`codegraph_compare.py`'s "invoked 2 times" — confirmed as a systematic,
100%-reproducible artifact, strengthening the "not a real bug" read.**
`tools/agent-bench/results/codegraph-compare.jsonl`: `grep -o "codegraph
command invoked [0-9]* times[^\"]*"` returns **`codegraph command invoked 2
times, expected 1` on all 10 of 10 lines**. A literal 2/2 (not intermittent)
rate across every task strongly supports the "environment artifact, not a
CodeGraph/model bug" theory — a flaky race would show partial reproduction,
not 10/10. The check itself, `codegraph_compare.py:154-177`, counts any
Bash tool_use event whose command string contains `"codegraph"`
(`codegraph_compare.py:168-169`) and flags `>1` as an error
(`codegraph_compare.py:175-177`); it has no tolerance for a harness-side
double-fire (e.g. a local `PreToolUse` hook that logs/repeats the same
Bash invocation) versus the model actually invoking the tool twice — see
recommendation 4.3.

**`--dangerously-skip-permissions` — confirmed fixed for the `claude` arm,
confirmed still present for `agy`, and a real scoped alternative exists.**
`run_claude_arm` (`agent_run.py:351-417`) uses `--allowed-tools` /
`--disallowed-tools` (`agent_run.py:384-385`, `393-394`) — no
`--dangerously-skip-permissions` anywhere in that function. `run_agy_arm`
(`agent_run.py:526-587`) still passes `--dangerously-skip-permissions`
literally at `agent_run.py:566`. `codegraph_compare.py`'s own `claude`
invocation (`codegraph_compare.py:112-130`) also uses
`--allowed-tools`/`--disallowed-tools` only, consistent with the "already
fixed for claude" half of the seeded claim.

Per Antigravity's own docs (`https://antigravity.google/docs/permissions`,
fetched 2026-08-23) and confirming WebSearch results: `skipPermissions: true`
maps to `--dangerously-skip-permissions`, described as needed because
*"in headless `--print` mode agy otherwise blocks on a permission prompt
and the job hangs."* But the same docs describe a scoped allow-list syntax:
**`mcp(server/tool)` or `mcp(server/*)`** — e.g. `mcp(linter/*)` to
auto-approve all tools from one named MCP server without a blanket skip.
Applied here that would be `mcp(gcx/*)`, scoping auto-approval to only the
`gcx` MCP server's tool calls while still gating Bash/file-write for
everything else `agy` might otherwise be tempted to do unsupervised. I did
not test this end-to-end against `agy` from this sandbox (no `agy` binary
available here) — flagging as a promising, doc-sourced alternative, not a
verified fix (see recommendation 4.4).

**`agy` command-budget mismatch — the *shape* of the seeded finding
(agy blows the shared budget) confirms against fresher trace data than the
summary anticipated, but the specific numbers and error text don't match.**
`tools/agent-bench/results/big-repos-agy-20260823T075925Z.agent.jsonl`
(created `2026-08-23T08:34:00Z`, i.e. the same day as this investigation —
newer than whatever run the seeded summary was written from) shows:

```
summary: valid=0/10, command_budget_met=2/10, quality_non_inferior=10/10
per-task gcx.commands: 16, 10, 10, 20, 1, 3, 17, 17, 13, 5
```

`quality_non_inferior=10/10` while `valid=0/10` confirms the seeded framing
that failures are not wrong-answer failures — the agent's answers were
judged non-inferior to baseline every time, but the run still failed
validity. However, the actual `error_messages` in this file are **not**
literally "exceeded command budget" — they're `context canceled`,
`Find command timed out ... context deadline exceeded`, and one MCP
permission/tool-schema error (`"declaring permissions: cortex tool
view_file: ... invalid_args ... failed to read file"`). The 16/17-call
figures the seeded summary cited are present (16, 17, 17, 20 all appear),
so the order of magnitude is right, but "expected ≤4 budget" is a derived
threshold (`command_budget_met = graph.commands <= 4`,
`agent_run.py:751`, mirrored in `agent_report.py:105`), not an error the
tool itself raises — `agy` doesn't fail *because* it exceeds a budget, it
runs long enough (many commands, each apparently slow/timing out) that the
harness's own timeout (`agent_run.py:578`, `timeout=330`) or agy's own
context cancels it first. The budget-exceeded framing is the harness's
diagnostic label for a run that was already failing for other reasons, not
agy's own failure mode.

**Budget constant is a single universal number.** Confirmed at two call
sites: `agent_run.py:751` — `command_budget_met = graph.commands <= 4` —
and `agent_report.py:105` — `row["command_budget_met"] = row["gcx"]["commands"] <= 4`.
Both hardcode `4` inline (not a named constant, not client-parameterized).
Every client (`claude`, `codex`, `agy`) is held to the same ≤4 figure
regardless of documented per-client tool-call overhead (e.g. `agy`'s own
MCP dispatch pattern in `run_agy_arm` costs at least 1 call just to invoke
`gcx`, same as `claude`, but the trace above shows `agy` runs routinely
land in the 10-20 range even when not erroring on the tool itself,
suggesting either genuinely more retries/verification steps per task or a
different definition of "command" in the parser for that client).

---

## 2. Recommendations for the confirmed findings

### 2.1 Confidence-weighted risk banding — **High priority, Medium effort**

Replace the raw `evidence.len()` bucketing in all three sites
(`agent.rs:360-366`, `blast_radius.rs:116-123`, `viz/lib.rs:610-615`) with a
confidence-weighted score — e.g. `extracted*1.0 + resolved*0.6 +
inferred*0.3` bucketed against adjusted thresholds, or a two-axis label
(`count` × `min-confidence-in-set`) if a single scalar loses too much
signal. `ConfidenceMix` (referenced at `agent.rs:336, 349-352`) already
tracks the three counts per call in `find_callers` — the data needed for
this is already computed and discarded, not something that needs new
plumbing to gather.

Tradeoff: this changes the meaning of "HIGH"/"CRITICAL" for every existing
caller of `find_callers`/`blast-radius`/viz risk badges — anyone who has
learned to trust the current raw-count thresholds (including any existing
benchmark pass/fail scoring keyed on risk_level text) needs to be
re-baselined. Because the three formulas are explicitly documented as
synchronized (§1.1's comments), all three must change together in the same
PR or the "same label means the same thing everywhere" invariant the
comments assert breaks.

### 2.2 Ambiguous-lookup auto-resolution — **Medium priority, Medium effort**

Add an opt-in (not default) `auto_resolve: bool` parameter to the three
ambiguity-prone MCP calls. When set and `resolve_symbol` returns
`Ambiguous`, pick the top candidate by `rank_candidates`
(`agent.rs:873-877`, already ranks non-test + `Pub` visibility first) and
proceed, but keep `next_action`/an `alternates` field in the response
listing the runners-up so the caller can course-correct without a second
full round trip. Do **not** silently change default behavior — ambiguity
resolution errors are exactly the kind of silent-wrong-answer risk the
project's own error-handling conventions (`.claude/CLAUDE.md`'s "no
`.unwrap()` in library code," `~/.claude/rules/common/security.md`'s
input-validation-at-boundaries) argue against defaulting to.

Tradeoff: auto-resolution trades guaranteed correctness for reduced latency
on the common case; get the threshold wrong (e.g. auto-resolving when the
top two candidates are close in rank) and you introduce silent wrong-symbol
answers, which is a worse failure mode than one extra round trip. Ship
behind the opt-in flag, then use a bench run that explicitly tags
`Ambiguous` outcomes (recommendation 4.1) to measure whether it actually
saves tokens/turns before flipping any default.

### 2.3 `detect_current_branch` robustness via MCP roots — **Medium priority, Medium-Large effort**

Have the gcx MCP server issue a `roots/list` request during/after
`initialize` when the connecting client advertises the `roots` capability,
and prefer a root's path over `cwd`-based `git symbolic-ref` when the two
disagree (or when `cwd`-detection fails outright, which `helpers.rs:99-110`
already returns `None` for on any git-command failure). Fall back to the
current `cwd` mechanism when the client doesn't support `roots` — this is
additive, not a replacement, so it doesn't risk the normal `.mcp.json`
end-user path.

Tradeoff: requires (a) confirming Claude Code's inline `--mcp-config` spawn
path actually declares `roots` support during `initialize` — unverified
here (§1.3) — spend a small research/instrumentation pass confirming this
before committing to the design; if it doesn't declare `roots` in that
spawn mode, this fix helps nothing and the harness workaround
(`59de43c`'s explicit `branch` param) remains the only lever. (b) `roots`
gives a *directory*, not a *branch* — gcx would still need its own
`git symbolic-ref` (or reading `.git/HEAD`) against the root path instead
of `cwd`, so this only fixes the "cwd doesn't match workspace" half of the
problem, not "branch detection is inherently a git shell-out."

### 2.4 CSV bulk-import PARALLEL flag — **Low priority (keep as-is), Small effort to document**

Recommend: leave `PARALLEL=false` (`bulk.rs:152`, `159`) as-is; add the
inline comment it currently lacks, citing kuzudb/kuzu#5778's multiline-CSV
limitation and this repo's own multiline-field CSVs
(`csv_quote()`/docstrings) as the reason, so a future contributor doesn't
"optimize" it away without re-deriving this research. Before ever
considering flipping it, write a small reproduction test with a
GitCortex-shaped CSV row containing an embedded newline against the pinned
`kuzu 0.11.3` and confirm parallel import doesn't silently corrupt or
reject it — do not rely on the GitHub issue alone, since its resolution
status is unconfirmed (§1.4).

Tradeoff: `PARALLEL=false` presumably costs bulk-load throughput on large
repos (this wasn't independently benchmarked here — no timing comparison
available), but given KuzuDB is archived and any regression here has no
upstream to report it to, correctness should dominate over throughput
unless someone has already measured the throughput cost as significant for
GitCortex's actual repo sizes.

---

## 3. Independent findings beyond the 5 seeded items

### 3.1 `MIN_BUDGET_TOKENS` floor could mask genuinely tight budgets

`agent.rs:276` clamps `options.budget_tokens.max(MIN_BUDGET_TOKENS)` for
`find_callers`'s query options — I did not chase down the exact value of
`MIN_BUDGET_TOKENS` in this pass, flagging as an open question: if a
caller intentionally requests a very small budget (e.g. to force
truncation and cheaply probe result shape), the floor silently overrides
that intent. Worth a quick grep for the constant's value and a decision on
whether it should be documented as a hard floor in the MCP tool's schema
description, not just enforced silently.

### 3.2 `resolve_symbol`'s qualified-vs-exact fallback order (`agent.rs:818-843`) is undocumented

The function tries a qualified-name match first (`agent.rs:818-836`), then
falls back to exact-name matching with dedup (`agent.rs:840-843`). This
two-tier resolution order is exactly the kind of "why" logic the project's
own comment conventions ask to be explained
(`.claude/CLAUDE.md`: "Minimal. Only comment non-obvious *why*") but
currently isn't — there's no comment at `agent.rs:818` or `840` explaining
why qualified-match is tried before exact-match, or what class of query
each branch is meant to catch. Low-risk doc gap, not a functional issue.

### 3.3 `codegraph_compare.py`'s reused-baseline assumption is a silent staleness risk

`codegraph_compare.py:52-58`'s `find_baseline_source()` globs for
`{BASELINE_LABEL}-*.agent.jsonl` and picks the most recent file with a
summary line — it reuses whatever `big-repos-claude-fixed` run happens to
be on disk rather than re-running the baseline alongside the CodeGraph arm.
If `gcx`'s source, the task suite, or the underlying repos (`tokio`,
`django`, `nextjs`, `moby`, `spring-boot` pinned commits) change between
when that baseline was captured and when `codegraph_compare.py` runs, the
comparison silently mixes stale baseline numbers with fresh CodeGraph
numbers. No staleness check (e.g. comparing `gcx_sha256` in the baseline
file's header against the currently-built `gcx` binary) exists at
`codegraph_compare.py:52-91`.

### 3.4 `error_arm_result` and the timeout ceiling interact to hide root cause

`error_arm_result` (`agent_run.py:590-...`) exists specifically, per its
own docstring, so "one flaky/slow client call must not discard an
otherwise-completed benchmark batch" — but this means a genuine `agy`
context-cancellation/timeout (§1.5) and a hard crash both surface
identically as a zero-usage error row in the summary. The per-task
`error_messages` field (used in §1.5's analysis) is the only thing that
distinguishes them, and it isn't currently surfaced in the printed
summary tables (`agent_report.py`) — someone reading only the Markdown
report, not the raw JSONL, would see "0/10 valid" with no hint that every
failure is a timeout, not a wrong-answer or tool-error.

---

## 4. Harness robustness recommendations

### 4.1 Tag `Ambiguous` outcomes explicitly in bench output

`bench.py`'s `RetrievalAdapter` (`bench.py:173-`) and `agent_run.py`'s
parsers currently classify a run as `error`/`valid`/`quality_non_inferior`
but nothing in the parsed `ArmResult` distinguishes "the MCP call returned
`AgentStatus::Ambiguous` and needed a follow-up" from any other outcome.
Add an `ambiguous: bool` (or `resolution_status: str`) field threaded from
the parsed `agent-json`/MCP response through to the summary line, so §1.2's
"could not independently verify" gap can actually be closed by a future
bench run instead of staying open indefinitely.

### 4.2 Add retry/backoff around `prepare_repo`'s mutating `gcx` calls

`bench.py:145-170`'s `gcx clean` (`bench.py:165`) and `gcx hook`
(`bench.py:166`) calls should retry with backoff on the specific
`serve_lock.rs:21-28` error text ("repository graph is active") before
failing the whole task — a bounded retry (e.g. 3 attempts, 2s/4s/8s
backoff) covers the case where a previous run's `gcx viz`/`gcx serve`
process is still tearing down. This does not require changing
`RepositoryLock` itself (§1.5 established the OS-level lock is already
correct) — it's purely a harness-side "don't give up on the first
`WouldBlock`" fix.

### 4.3 Make `codegraph_compare.py`'s exactly-once check tolerant of duplicate tool-call events

Given the 10/10 reproduction rate (§1.5) strongly suggests an event-stream
artifact rather than the model actually running the command twice, change
`codegraph_compare.py:154-177`'s counting logic to deduplicate consecutive
identical Bash `tool_use` events (same `command` string, adjacent in the
stream) before comparing against the `>1` threshold, or log the raw
`tool_use` IDs so a human can distinguish "genuinely called twice" from
"same call logged twice." Keep the check itself (catching a model that
actually retries against instructions is still valuable) — just don't let
a harness-local hook artifact fail every single run.

### 4.4 Replace `agy`'s `--dangerously-skip-permissions` with a scoped MCP allow-list

Per §1.5's finding, Antigravity's docs describe `mcp(server/tool)` /
`mcp(server/*)` allow-list syntax. Change `run_agy_arm`
(`agent_run.py:526-587`) to pass a permissions config granting
`mcp(gcx/*)` instead of the global `--dangerously-skip-permissions` at
`agent_run.py:566`, keeping Bash/file-write gated for everything else `agy`
might attempt. This is unverified end-to-end (§1.5 — no `agy` binary in
this sandbox) — treat as a concrete next step to test, not a drop-in fix;
confirm the exact CLI flag or config file `agy` expects this permissions
block in (docs referenced a config file, not necessarily a CLI flag) before
wiring it in.

### 4.5 Make the command budget per-client, not a shared literal `4`

Both `agent_run.py:751` and `agent_report.py:105` hardcode
`graph.commands <= 4` inline. Extract a `COMMAND_BUDGET_BY_CLIENT: dict[str,
int]` (or similar) keyed by client name, sourced from each client's actual
dispatch pattern (e.g. `claude`/`codex` at the currently-implied 4, `agy` at
whatever a clean, non-timing-out run's typical command count turns out to
be once §4.1-style tagging separates timeout failures from genuine
over-budget runs). Until §1.5's "agy's failures are timeouts, not budget
overruns" finding is resolved, raising `agy`'s number alone won't fix
0/10 valid — the timeout/context-cancellation problem is upstream of
the budget check and needs its own investigation (possibly a longer
`--print-timeout`, per `agent_run.py:567`'s existing `300s`, or a look at
why `agy`'s own commands are taking 10-20 steps for tasks other clients
finish in ~4).

---

## Open questions

1. Does Claude Code's inline `--mcp-config` spawn path declare the `roots`
   client capability during `initialize`? Unverified from this sandbox —
   needed before committing to recommendation 2.3.
2. Was kuzudb/kuzu#5778 (parallel multiline-CSV import) actually fixed in
   any release, including the pinned `0.11.3`? Its GitHub timeline shows no
   linked commit. `docs.kuzudb.com` was unreachable from this sandbox for
   every attempted path (`getaddrinfo ENOTFOUND`) — worth a retry from an
   environment with working DNS to that host before deciding on
   recommendation 2.4.
3. What exact CLI flag or config-file location does `agy` read a
   `permissions.allow`/`mcp(server/*)` block from in headless `-p` mode
   specifically? The Antigravity docs snippet found describes the syntax
   but this wasn't tested against the actual `agy` binary.
4. Why does `agy`'s `gcx` arm run 10-20 commands per task (per
   `big-repos-agy-20260823T075925Z.agent.jsonl`) when the dispatch prompt
   (`agent_run.py:537-548`) instructs "make exactly that one gitcortex call
   ... never call any MCP tool again"? Worth pulling the full
   `big-repos-agy-20260823T075925Z.agent-logs` directory (not just the
   summarized `.jsonl`) to see what those extra commands actually are —
   out of scope for this pass but the single highest-value next
   investigation for making the `agy` lane usable at all.
5. What is `MIN_BUDGET_TOKENS`'s actual value (§3.1), and is it documented
   anywhere a caller would see it before hitting it?
