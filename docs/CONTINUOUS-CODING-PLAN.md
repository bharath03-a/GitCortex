# GitCortex continuous-coding improvement and evaluation plan

Status: proposed, 2026-10-16. No new benchmark results are claimed here.

## Outcome

Make GitCortex a dependable companion throughout a coding session: discover,
edit, query again, test, debug, commit, switch branches, review, and resume.
Success means correct changes with less total exploration, without misleading
answers when the graph is incomplete. Single-query payload savings are a
diagnostic measure, not the primary product outcome.

This supplements the PR-5 combined release plan. Keep release-blocking trust
fixes and release preparation in PR 5 as planned. Establish a baseline before
changing behavior; if this program exceeds that release's scope, record the
remaining work as a subsequent milestone rather than expanding PR 5 silently.

## 1. Establish what is actually broken

Reproduce reported wrong-default-branch and MCP array-result failures before
calling them confirmed bugs. Capture requests, responses, client/version,
checkout identity, and the exact candidate binary hash. Test MCP initialization,
tool discovery, dispatch, and response validation through a real protocol client.

Existing source warrants targeted investigation:

- The watcher handles create/modify events but filters out removal events.
  Reproduce deletes and renames and inspect the resulting graph.
- Its debounce waits for quiet. Exercise sustained edits and determine whether
  freshness can be delayed indefinitely.
- Branch detection can fall back to `main`. Exercise detached HEAD, branch
  switches, and Git failures; never infer that the fallback is correct.
- `doctor` treats an active server as managing freshness without checking that
  its watcher is healthy or caught up. Test watcher failure and pending work.
- Current agent statuses distinguish ok/ambiguous/not-found but do not by
  themselves express index readiness. Audit every response path.

These are investigation targets, not assertions that all paths are defective.

Deliverable: small regression fixtures and a defect ledger with reproduced,
not-reproduced, fixed, and deferred states. Preserve failure traces.

## 2. Make every graph answer carry its scope

Define one shared readiness contract for CLI and MCP. Include repository and
worktree identity, requested/effective branch or detached revision, checkout
HEAD, indexed HEAD, graph generation, and working-tree synchronization state.
Separate graph readiness from query outcome: a healthy empty repository is
different from an unindexed repository or an indexed symbol with no callers.

Model at least missing, building, ready, stale, degraded, and incompatible
states. A matching commit SHA alone does not establish freshness when tracked
or supported untracked files have changed. Explicit historical-branch queries
must identify their historical scope; they need not match the current checkout.

Require these behaviors:

- Resolve omitted branch from the actual checkout; no silent unrelated fallback.
- Never label impact as complete or low risk solely because a graph is empty.
- For unusable scope, return an actionable status and source-verification hint.
- Keep ordinary queries read-only; destructive migration requires explicit init.
- Report partial parse/index failures and truncation separately from no matches.
- Preserve compact budgets; return essential trust metadata consistently.
- Verify object/array response compatibility through real MCP serialization.

Gate: no silent wrong-scope, false-fresh, or empty-index-as-safe answer in the
deterministic regression matrix. Review contract changes for version implications.

## 3. Keep the graph correct while coding

Specify committed-snapshot versus working-tree behavior before implementation.
Prefer an explicit working-tree overlay or equivalent isolation so uncommitted
edits cannot contaminate another branch/worktree's committed graph. Choose the
smallest design that satisfies isolation and incremental performance.

Handle create, modify, delete, rename, tracked/untracked/ignored files, parse
failure and recovery, rapid saves, branch checkout, commit, reset, stash/pop,
rebase, detached HEAD, daemon restart, and concurrent worktrees. Invalidate
semantic data alongside graph changes. Test external-process lock contention.

Bound event queues, deduplication, and processing time. Use a maximum wait as
well as quiet-period debounce. Expose pending/failed batches and last successful
synchronization. Do not rebuild the whole repository on every save.

Gate: at checkpoints, incremental state agrees with an independently rebuilt
graph for supported syntax; hand-reviewed relationships independently verify
semantic correctness. Compare graph equivalence without relying on unstable IDs.
A full rebuild of the same parser is an incremental-consistency oracle, not a
proof that the parser understands the language correctly.

## 4. Support repeated investigations

Use qualified identities and bounded progressive evidence throughout callers,
callees, context, implementations, and impact. Make follow-up pages stable within
a graph generation; detect changed-generation continuation rather than silently
mixing snapshots. Test symbols renamed or removed between consecutive queries.

After trust gates pass, measure demand for a compact change-since-checkpoint
summary: changed definitions, new/removed relationships, impacted production
callers, relevant tests, and uncertainty. Prototype only if repeated sessions
show this removes redundant exploration. Avoid building a broad agent platform.

## 5. Benchmark complete coding episodes

An episode has a pinned starting commit, task brief, observable state
transitions, independent checks, and a final patch/review. Keep a real client
session alive across steps to preserve accumulated context and cache behavior.
Resume a subset in a new session to measure recovery. Report both separately.

Initial suite: three public repositories in different supported languages,
four episode families each, approximately 6–10 checkpoints per episode.
Select repos by maintainable builds, meaningful relationships, and independent
testability; pin exact commits after smoke testing. Expand to all five language
groups before a cross-language claim.

| Episode | Continuous workflow | Independent success evidence |
|---|---|---|
| Feature evolution | Find entry point, add behavior, update caller/type, query before commit, test, commit, extend requirement | Behavioral tests, final diff, checkpoint relationships |
| Debugging | Trace a failing cross-module behavior, propose fix, introduce follow-up constraint, repair | Hidden regression tests, root-cause rubric, no unrelated regression |
| Refactor | Resolve duplicate names, inspect impact, rename/move API, update callers, delete old implementation | Build/type checks, tests, no old references, reviewed relationship expectations |
| Branch/review/resume | Edit branch A, switch to B with conflicting names, change B, return to A, review exact revisions, restart | Correct scope at each query, isolated graphs, seeded impact findings, successful resumed task |

Add deterministic fault episodes for missing/stale index, stopped watcher,
invalid syntax, deletions, event floods, and unavailable transport. Reward
accurate degraded-state disclosure and recovery, not fabricated empty success.

Use two complementary lanes:

1. Scripted state transitions with fixed probes isolate product correctness and
   edit-to-correct-query latency without model variance.
2. Autonomous coding episodes let the model choose tools and produce patches.
   Measure actual usefulness, including when the agent correctly chooses source
   search instead of GCX. Do not require exactly one graph call per episode.

## 6. Fair experimental protocol

Compare ordinary source tools with the same tools plus GCX. Both arms get the
same model, client, reasoning settings, task brief, environment, budgets, and
source/test access. GCX is optional in the treatment arm. Count graph setup,
refresh, failures, retries, and verification in its costs. Run existing forced
graph retrieval tests as a separate diagnostic lane.

Within each arm, preserve its session across episode checkpoints. Between arms,
use isolated checkouts, graph stores, and sessions; reset to the pinned start.
Never transplant one arm's generated patch into the other. Compare at externally
defined milestone requirements because autonomous intermediate edits can differ.

Freeze development and held-out tasks before tuning. Derive ground truth from
source, hidden behavioral tests, and independent review, not GCX output. Use
blinded patch scoring when practical. Score hidden tests after execution without
exposing their contents to the agent. Record ambiguous rubric decisions.

Start with one cheap paired smoke episode. Then run 12 episodes × 3 paired
repetitions × 2 arms = 72 sessions for one client/model. Randomize and balance
arm order with a recorded seed. Treat the episode as the sampling unit; its
checkpoints are correlated and are not independent samples. Increase repetitions
when uncertainty prevents a decision. Confirm on a second client before making
cross-client claims. Report CLI and MCP as distinct interfaces.

Separate controlled cold/warm index conditions from provider cache accounting.
Do not claim a cold provider cache unless the client can enforce it. Record
observed cache usage and any uncontrolled cross-session caching.

Set a spend/time cap from smoke measurements before the full run. No paid full
sweep until deterministic correctness and transport gates pass.

## 7. Measures and decision rules

Primary outcomes: episode completion, final behavioral correctness, regression
rate, and wrong-scope/stale-evidence incidents. Then assess total session tokens,
elapsed time, exploratory reads, tool calls, recovery turns, and manual repair.
Report setup and steady-state costs separately plus their combined totals.

Collect p50/p95 edit-to-correct-evidence latency, query latency, hook latency,
index CPU/memory, queue size, and recovery time under declared workloads. Include
time spent waiting for synchronization. Publish denominators and failures.

Predeclare provisional engineering gates, then freeze them before held-out runs:

- Zero silent wrong-scope/false-fresh incidents in the deterministic suite.
- Zero incremental/rebuild discrepancies for supported state transitions.
- On the declared local medium-repo fixture, target p95 correct evidence within
  2 seconds of the final save; separately measure sustained edits and large repos.
- Transport compatibility passes for every supported public action/response shape.
- No hidden behavioral-test regression accepted as a token-saving win.

For usefulness, target at least 15% lower median exploratory reads and 10% lower
median total session tokens with non-inferior completion/correctness. These are
targets, not claims. Predeclare a non-inferiority margin and paired analysis
before collection; use episode-level paired bootstrap intervals and publish raw
counts. If intervals are inconclusive, say so and collect more data. Aggregate
over all attempted episodes; also report quality-matched costs separately.
Never drop failed episodes to improve savings. Show workflow/client losses.

## 8. Harness and evidence artifacts

Extend tools/agent-bench instead of replacing historical lanes. Proposed files:

- `continuous-suite.toml`: pinned episodes, transitions, probes, checks, budgets.
- `continuous_run.py`: isolated environments, persistent sessions, checkpoints.
- `continuous_score.py`: behavioral and relationship scoring plus paired report.
- focused unit/fixture tests for orchestration, failure retention, and replay.
- checksummed run manifest, event traces, patches, test logs, and summary JSON.
- `scripts/r1_release_gate.py`: consume approved evidence and reject missing or
  mismatched provenance; link to the existing PR-5 gate requirements.

Record product commit/binary hash, suite hash, repo commits, client/model resolved
versions, configuration, seed, round, interface, indexed state, cache accounting,
setup costs, tokens, timings, failures, final patches, and scoring decisions.
Replay scoring without rerunning models. Preserve raw artifacts privately when
necessary; publish safe checksummed summaries without claiming full public
reproducibility if required inputs remain unavailable.

## 9. Competitors and real users

After GCX beats or meaningfully complements ordinary tools, evaluate pinned
Graphify/CodeGraph revisions on their documented supported episode workflows.
Smoke-test each adapter and use recommended setup. Give every arm equal source
tools and budgets. Report unsupported capabilities separately. Existing comparison
scripts can inform adapters but do not establish this experiment's provenance.

Run a 1–2 week opt-in pilot with 2–5 developers doing repeated real tasks.
Capture task success, errors, adopted workflows, setup/recovery burden, and
whether they retain GCX. Keep private source and telemetry local unless users
explicitly authorize sharing. Label observational findings separately from
controlled results. Use the existing pilot guide and update it with episode logs.

## 10. Delivery checkpoints

1. **Baseline:** reproduce feedback, freeze smoke episode and collect current
   behavior; approve contract and benchmark rubric.
2. **Trust:** implement readiness/scope/transport fixes with regressions.
3. **Continuity:** implement demonstrated watcher/isolation/recovery fixes and
   pass deterministic transition tests and measured overhead gates.
4. **Usefulness:** run paired smoke, inspect failures, freeze held-out suite,
   then complete the 72-session primary evaluation within the agreed cap.
5. **Confirmation:** second client, supported competitor episodes, and real-user
   pilot; keep uncertainty and negative results visible.
6. **Release:** reconcile PR-5 evidence, version/migration policy, packaging,
   documentation, reviews, merged-main checks, and published-channel verification.

Do not release merely because the benchmark is cheaper. Ship when graph scope
is trustworthy, changing-code workflows pass, and all required release gates are
satisfied. Do not require speculative roadmap features to finish this program.

### Local call audit for experiments

Set `GCX_AUDIT_LOG=/absolute/path/gcx-calls.jsonl` in the MCP server's
startup environment and restart the server. The parent directory must already
exist. Logging is disabled when this variable is absent or empty. Each completed
MCP call records schema version, timestamp, process ID, advertised action name,
compact/full mode, duration in microseconds, serialized response bytes, outcome,
and available index state and graph generation. Unknown actions are recorded as
`unknown`. Arguments, queries, symbols, paths, source, response contents, and error
messages are excluded. Nothing is uploaded.

The log rotates at 8 MiB, retaining one `.1` backup. Use a distinct path per
server process: rotation is synchronized within a daemon, not across processes.
New files use owner-only permissions on Unix; existing file permissions are
preserved. Writes are synchronous and best effort; write failures warn on stderr
without failing queries. Call latency excludes the audit write itself. A crash
or cancellation before completion may leave no event. This is diagnostic usage
history, not a tamper-proof security audit or a record of ordinary shell/CLI calls.

For paired continuous-coding runs, keep one audit log per treatment session next
to the client transcript. Compare action frequency, error/readiness outcomes,
latency, response bytes, and generation changes against task completion and test
results. These logs alone cannot establish correctness, token savings, or causal
productivity improvements. CLI call auditing and an aggregate report remain
follow-up work.
