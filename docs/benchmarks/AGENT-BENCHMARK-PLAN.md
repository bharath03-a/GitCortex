# Agent-First Product and Benchmark Plan

Status: proposed

## Executive conclusion

The recent Codex result does **not** establish that Phases 1–3 made GitCortex worse. The run was not an apples-to-apples phase comparison, and—more importantly—the Codex CLI arm bypassed most of the implementations changed in those phases.

The immediate objective is therefore not to tune prompts until the score rises. It is to establish one shared, compact agent-query path, build a controlled harness around it, and only then optimize the workflows that remain weak.

## What the latest run actually measured

The post-Phase 3 exploratory run used `gpt-5.4-mini` through `codex exec`, with the agent instructed to run `gcx query ...` first. It produced a 1.20× total-token and 1.31× uncached-token geomean over 5 repos × 4 tasks.

That run did **not** cleanly test the recent phases:

| Change | Product implementation | Exercised by Codex CLI run? |
|---|---|---|
| Phase 1 resolved confidence | graph/index/store; consumed by MCP caller ranking | Partially indexed, but CLI output does not expose confidence |
| Phase 2 ranked caller head | MCP `find_callers`, depth 1 | No; harness requested CLI depth 2, which emits uncapped hops |
| Phase 2 live watcher | MCP server lifecycle | No; benchmark repos were static |
| Phase 3 RRF hybrid search | MCP `search_code` semantic + lexical path | No; CLI `query search` calls lexical search directly |
| Phase 3 health report | MCP tool | No corresponding benchmark task |

Consequently, the 1.20× result is useful evidence about the **current CLI agent experience**, not a causal before/after result for Phases 1–3.

## Root-cause findings

### P0 — Benchmark validity defects

1. **The subgraph command was invalid.** The harness passed `--limit 30`, but CLI `get-subgraph` has no `--limit`. Every repo incurred at least one failed command and recovery loop.
2. **Codex MCP was unavailable.** The local MCP server appeared in Codex configuration, but ad-hoc tools were not exposed to the ChatGPT-account `codex exec` session. The fallback CLI lane was incorrectly discussed as comparable to the Claude MCP lane.
3. **Different model, client, cache behavior, and interface were compared.** Phase 0 used Claude/MCP; the new run used Codex/CLI. This cannot attribute a regression to product commits.
4. **Agent-picked symbols were unstable and ambiguous.** Examples include `get`, `add`, `match`, `as_bytes`, and a test helper `executeCommand`. These are poor fixed refactor/subgraph targets.
5. **Tool success was not a hard validity gate.** The agent could recover with grep after a graph failure, and the result still entered the aggregate.
6. **Run variance is material.** Targeted reruns flipped `cobra/subgraph` from 0.49× to 1.72× and `hono/tour` from 0.60× to 1.58×.

### P0 — Product/interface defects exposed by the run

1. **CLI and MCP have materially different response contracts.** MCP callers are ranked/capped; CLI callers are raw. MCP subgraph strips sections, summarizes, and caps; CLI subgraph prints every node. MCP search can use semantic RRF; CLI search cannot.
2. **Large graph outputs trigger verification rather than replacement.** Median graph payloads were approximately 761 chars for search, 6,220 for tour, 11,089 for callers, and 11,907 for subgraph. The agent still issued median follow-up commands of 5, 11, 13, and 11 respectively.
3. **Ambiguous names are traversed instead of disambiguated.** `get` in Requests caused the model to inspect multiple unrelated definitions. A graph tool should return a short candidate list before computing blast radius.
4. **Tour ranking does not implement its stated intent.** The code says it picks public entry points, but `global_tour` does not filter visibility, tests, generated code, docs, or vendors. Generic high-fan-in methods can dominate.
5. **Depth-2 callers bypass the Phase 2 ranking and response budget.** Deep caller hops are capped per hop but are not confidence-ranked, production-first, or globally budgeted.
6. **Subgraph budgets are not global.** MCP budgets nodes and edges independently, allowing roughly two list budgets plus summary/metadata.
7. **Compact MCP is not strictly one tool.** `health_report` is not disabled in compact routing, while it is also absent from the `gcx` dispatch action list. This creates contract drift.

## Product strategy

GitCortex should optimize for **decision-ready evidence**, not graph serialization.

The default agent response should answer four questions:

1. What is the likely answer?
2. What source evidence supports it?
3. How complete/confident is the graph result?
4. Is one follow-up necessary?

Raw nodes and edges should be an explicit diagnostic/detail mode, never the default agent payload.

## Target response contract

All agent-facing interfaces—MCP and CLI—should use one shared query service and one compact envelope:

```json
{
  "answer": "3 production callers; change risk is medium.",
  "evidence": [
    {"symbol":"Router.dispatch","file":"src/router.ts","line":91,"relation":"calls","confidence":"extracted"}
  ],
  "coverage": {"total":12,"returned":5,"truncated":true,"confidence_mix":{"extracted":8,"resolved":3,"inferred":1}},
  "ambiguity": null,
  "next_action": null
}
```

Rules:

- One global serialized response budget.
- Exact or qualified symbol identity before traversal.
- Ambiguous names return candidates and no traversal.
- Production/public evidence first; tests summarized separately.
- Raw graph available only through `detail=raw`.
- Stable ordering and explicit truncation/coverage.
- Same golden response semantics through MCP and `gcx query --format agent-json`.

## Workflow changes

### Search

- Preserve lexical search as the reliable base.
- Share hybrid RRF through the common query service so CLI and MCP do not diverge.
- Return top files and top symbols as evidence; target ≤800 response tokens.
- Measure relevance (MRR, precision@5, file recall), not only session tokens.
- Do not claim semantic improvement until RRF beats lexical on a pinned relevance set.

### Tour

- Rank public production entry points, excluding tests/docs/generated/vendor paths.
- Group by package/module rather than treating every root-level file as a component.
- Default to 4–6 components, two key symbols each, and top dependency links.
- Include a compact request-flow narrative so the agent need not read 10–20 files.
- Target ≤1,200 response tokens and ≤3 verification reads.

### Refactor impact

- Resolve to an exact symbol ID/qualified name first.
- Default to depth 1; represent depth 2 as counts and top risky paths, not full hops.
- Apply confidence ranking and production/test separation at every depth.
- Include call-site file/line evidence and an ambiguity response.
- Target ≤1,000 response tokens.

### Neighborhood/subgraph

- Replace raw default output with relation buckets: callers, callees, used types, implementations, imports.
- Return the top five per bucket plus totals.
- Add CLI `--limit` and `--format agent-json`, or remove limit from harness until supported.
- Keep raw nodes/edges behind `detail=raw`.
- Target ≤800 response tokens.

### Freshness and health

- Benchmark watcher freshness separately: edit-to-query latency, correctness after create/modify/delete, CPU use, and lock contention.
- Wire `health_report` into compact dispatch or explicitly remove it from compact mode.
- Give health report its own quality/latency suite; do not mix it into navigation token claims.

## Custom benchmark harness

Create `tools/agent-bench/` as a Python orchestration package. Keep it outside shipping crates; use JSONL artifacts and optional provider dependencies.

### Suite manifest

Each task is pinned and reviewable:

```yaml
id: requests-session-send-impact
repo: https://github.com/psf/requests
commit: <sha>
language: python
question: If I change Session.send, what breaks?
action: find_callers
params: {qualified_name: requests.sessions.Session.send, depth: 1}
ground_truth:
  required_files: [src/requests/sessions.py, tests/test_requests.py]
  required_symbols: [Session.send]
  forbidden_symbols: [RequestsCookieJar.get]
```

No task may derive its target from the product under test.

### Four lanes

1. **Retrieval lane (no model):** relevance, recall, precision, payload tokens, latency, ambiguity correctness.
2. **Controlled answer lane:** one model call receives a fixed context produced by baseline retrieval or GitCortex. No autonomous tools. This isolates context quality.
3. **Provider tool-loop lane:** direct OpenAI Responses/Anthropic API adapters expose one equivalent dispatch schema and execute the real shared query service. This isolates model tool use without client-specific hidden prompts.
4. **Native client lane:** Claude Code MCP and Codex MCP/CLI as real users experience them. If a client cannot expose local MCP, report the lane as unsupported—never silently substitute another interface.

### Fairness and accounting

- Build base and head revisions in isolated worktrees; re-index each repo separately.
- Pin repo commits, model IDs, reasoning effort, schemas, prompts, and binary SHA.
- Randomize arm order (AB/BA) and run three rounds for release gates.
- Fail a sample on tool errors, missing required tool calls, forbidden fallback, zero usage, or missing evidence.
- Record total, uncached, cached, output, reasoning, tool payload, turns, commands, wall time, answer, citations, and quality.
- Report quality first. Token savings count only when quality is non-inferior.
- Use medians and confidence intervals; retain geomean only as a secondary aggregate.
- Add replay mode so report generation and deterministic scoring cost nothing.

## Branch and PR strategy

All milestones ship through **one integration PR**:

- Integration branch: `feat/agent-first-product`, created directly from `main`.
- Optional milestone branches are created from the integration branch, not from `main`.
- Milestone branches are merged back into the integration branch only after their local gates pass.
- No milestone PR targets `main`; checkpoints remain commits and test artifacts on the integration branch.
- The single PR from `feat/agent-first-product` to `main` opens only after the complete retrieval, contract, quality, and native-client gates pass.
- If the approach fails, the integration branch can be abandoned without changing `main`.

Suggested child branches when parallel work is useful:

```text
feat/agent-first-product
├── feat/agent-query-contract
├── feat/agent-workflow-quality
├── feat/agent-bench-harness
└── test/agent-release-gates
```

To minimize conflicts, contract types land on the integration branch before workflow and harness branches begin consuming them. The final PR may preserve milestone commits for review and be squash-merged only after approval.

## Delivery plan

### Milestone 0 — Stop measuring noise (1–2 days)

- Label current Codex results exploratory.
- Fix/remove unsupported CLI arguments.
- Add hard validity checks for intended tool call count and exit status.
- Pin task symbols and repo commits.
- Fix compact router/dispatch drift for `health_report`.

**Gate:** 100% valid traces; no fallback-contaminated samples.

### Milestone 1 — One agent query surface (3–5 days)

- Introduce shared compact response types and service functions.
- Add `--format agent-json` and a global response budget to CLI.
- Make MCP and CLI golden snapshots equivalent.
- Add exact symbol identity and ambiguity handling.

**Gate:** the same task returns semantically equivalent MCP/CLI evidence and respects one payload budget.

### Milestone 2 — Repair weak workflows (4–7 days)

- Production/public-aware tour.
- Confidence-ranked callers at all depths.
- Relation-digest subgraph default.
- Shared lexical/hybrid search path with relevance evaluation.

**Gate:** workflow payload limits met; no task has median ratio below 0.95; quality is non-inferior.

### Milestone 3 — Build agent-bench v1 (4–6 days)

- Manifest loader, worktree builder, retrieval scorer, OpenAI/Anthropic controlled adapters, native-client adapters, JSONL schema, replay/report.
- Add a low-cost smoke suite (1 repo/language, 2 tasks) and full release suite.

**Gate:** one command compares a base commit and HEAD with reproducible provenance.

### Milestone 4 — Re-establish claims (budget-dependent)

Run in this order:

1. Retrieval lane on all tasks—free.
2. Controlled lane on cheapest supported model—one round.
3. Targeted reruns for regressions—three rounds.
4. Full native release gate—three rounds only after fixes pass.

**Release target:** on the same model/client/harness, quality non-inferior, overall median/geomean target ≥1.8× for the optimized core workflows, cross-agent floor ≥1.3×, and no workflow median below 0.95×. These are targets, not current claims.

## First implementation slice

The first PR should be deliberately small:

1. Fix benchmark validity and pin manifests.
2. Add exact/ambiguous symbol resolution.
3. Add CLI subgraph limit and compact agent JSON.
4. Unify depth-1 caller ranking between CLI and MCP.
5. Add contract tests proving compact MCP exposes exactly one dispatch tool.

Do not tune prompts or rerun a paid full sweep until this slice passes free retrieval and contract tests.
