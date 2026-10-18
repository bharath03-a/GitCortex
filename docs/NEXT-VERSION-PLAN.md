# GitCortex next version: dependable continuous coding

Updated: 2026-10-18. Delivery lead: Codex in this repository, with the maintainer
retaining product direction. This is the working execution plan, not a record
of completed validation or a promise of release dates.

## Product decision

Target 0.8.0 for the combined R1 release, consistent with the existing PR-5
compatibility policy. Package versions remain 0.7.6 until evidence and release
preparation are ready. Do not silently widen PR 5 into an agent platform.

GitCortex should help a developer or coding agent understand relationships,
make a change, and keep that understanding useful through subsequent edits,
commits, branch switches, and resumed sessions. Source inspection and tests
remain the correctness checks. Our differentiator must be demonstrated useful
relationships in changing code, with explicit scope and uncertainty.

The launch claim should describe supported workflows and measured results.
Do not promise complete dependency discovery, safe changes, universal language
coverage, or token savings from smaller payloads alone.

## Current evidence and debt

Implemented locally: branch/readiness safeguards, array response normalization,
watcher recovery and bounded event handling, persistent dirty-state tracking,
source receipt verification, release evidence checker, and opt-in bounded audit
metadata. These changes have not been published.

The previous checkpoint passed 350 workspace Rust tests. Later audit changes
have targeted checks; this historical count is not a test result for today's
HEAD. A frozen earlier debug build passed seven Cobra retrieval tasks. That is
not a full 35-task run, a baseline comparison, or a continuous-coding result.
See `docs/benchmarks/pr5-development-checkpoint.md` for exact provenance.

Still missing: paired persistent-session benchmark runner and smoke results,
full candidate/baseline retrieval runs, pinned supported competitor comparisons,
packaging/migration matrix, final claims audit, version/changelog updates, exact
candidate reviews, merged-main validation, and published-channel verification.
The historical PR-5 plan and release gates still govern those requirements.

## Delivery order and completion criteria

| Order | Deliverable | Completion evidence |
|---|---|---|
| 1 | Keep observability off the critical path | Disabled path bypasses audit processing; bounded 256-event queue; nonblocking enqueue; disk cap tests; queue-full and writer-failure tests; measured enabled/disabled overhead |
| 2 | Close trust regressions | Real MCP and CLI scope/readiness checks; edit/create/delete/rename/commit/reset/switch/restart scenarios; incremental-versus-rebuild comparison; no silent false-fresh/empty-as-safe result |
| 3 | Build one continuous paired smoke | Same pinned repo/task/client/model, separate arms, same budgets, session retained over several requirements, independent behavioral tests, complete traces and final patches |
| 4 | Freeze and run launch evidence | Candidate release binary and suite checksummed; 35 retrieval tasks plus baseline; native-client/competitor lanes and rounds required by R1 gate; negative and unsupported outcomes retained |
| 5 | Finish release readiness | Install/update/reindex/rollback matrix; docs agree with behavior and results; versions aligned to 0.8.0; required reviews and checks pass on final candidate |
| 6 | Launch and verify | Merge before tag; verify published artifacts and install channels; publish scoped results and known limitations; follow existing release runbook |

Audit metadata uses a 256-entry in-memory queue and two files of at most 1 MiB
each per configured path. It is off by default. Queue saturation discards events,
not user requests. No guarantee of zero CPU overhead or lossless recording is
made. Measure tool-call p50/p95/p99 with logging off, on, and a stalled writer.
Do not count the audit's own calls as product improvements.

## Benchmark execution policy

Keep retrieval, deterministic state transitions, and autonomous coding as
separate evidence lanes. Use the existing continuous-coding plan's four episode
families: evolving feature, cross-module debugging, refactor, and branch/review/
resume. Begin with one inexpensive paired smoke and replayable scoring. Do not
force the agent to call GCX; compare ordinary tools with ordinary tools plus
optional GCX. Existing one-shot retrieval loops do not satisfy this requirement.

Record exact source/binary/suite/repo/client/model identity, cache conditions,
setup time, all failures and retries, tool traces, total task time, tokens when
available, hidden-test outcomes, and patch quality. Score correctness before
speed or tokens. Run a second client before making cross-client claims.

Before the full 72-session primary experiment, derive a monetary and runtime
cap from the smoke's measured costs. Enforce that cap in the runner; if account
spend authorization is not already established, finish the local harness and
cost estimate first. Do not initiate open-ended paid experiments. No invented
success manifests or normalized summaries without underlying traces.

## Product bets after launch gates

1. **Change-since checkpoint.** Prototype a bounded, generation-aware summary
   of changed definitions/relationships after a saved checkpoint. Use opaque
   scope-bound tokens, bounded retention, and explicit expired/scope-changed
   responses. Validate against fresh source and graph rebuilds. Ship only if
   paired episodes show reduced repeated exploration without weaker patches.
2. **Relevant test evidence.** Return candidate tests with dependency paths and
   confidence. This requires inline test metadata and source verification;
   static graph reachability cannot guarantee a sufficient test suite.
3. **Bounded impact traversal.** Profile high fan-out fixtures, batch adjacency
   where supported, preserve strongest-confidence paths deterministically,
   and expose truncation/lower-bound counts. Establish fixtures before changing
   algorithms; do not infer a speedup from design alone.
4. **Activity and usage insights.** Share one bounded metadata summary between
   the local UI and a read-only MCP action. Report observed behavior and
   suggested alternatives. Avoid raw arguments, background model calls,
   automatic agent intervention, and a second analytics database.

These are ranked hypotheses, not committed launch blockers. If evidence shows
low demand or no benefit, defer them. The detailed UI/insights design is in
`docs/AUDIT-UI-PLAN.md`.

## Adoption and feedback

Keep the first-use path short: initialize, see readiness, ask a relationship
question, inspect evidence, change code, query again. Make branch and freshness
visible wherever users might interpret missing relationships as safety.
Provide one repeatable demo covering edits and branch changes, not just a static
repository tour. Update QUICKSTART, PILOT-GUIDE, ADOPTION, and DEMO_SCRIPT only
with behavior actually verified on the candidate.

After launch, use an opt-in 2–5 developer pilot over 1–2 weeks. Track setup and
recovery effort, repeated use, completed tasks, and whether participants keep
using GCX. Local audit counts describe usage, not retention or correctness;
combine them with explicit feedback and behavioral outcomes.

## Operating cadence

At each completed work batch, update evidence with commit identity, checks,
what changed, what remains uncertain, and the next blocking deliverable. Keep
working on authorized reversible implementation without repeated approvals.
Do not hide release blockers behind new feature work. When changing scope,
record the decision here and keep the original release requirements visible.

The immediate next batch after background audit validation is the persistent
paired-smoke harness and a frozen episode specification. Activity UI and usage
coaching remain subsequent work so observability does not delay the launch.

## 2026-10-18 execution checkpoint

Implemented background audit delivery: a 256-event nonblocking queue, one worker
per enabled daemon, cumulative drop/write-failure counters, and no flush wait
on shutdown. Unknown actions remain sanitized; advertised action names are
cached at server creation rather than rebuilding tool schemas for each audit
event. Schema v2 measures existing response text bytes without copying or
serializing the whole response. Disabled logging bypasses audit processing.

Validation for this batch: all 171 MCP crate tests passed (83 unit, 66 pipeline,
1 protocol, 21 search evaluation), all 24 release-gate tests passed, workspace
clippy passed with warnings denied, formatting and diff checks passed. Audit
coverage includes full/disconnected queues, real background writes, asynchronous
write failures, privacy, and repeated rotation within the 2 MiB total cap.
Enabled/disabled end-to-end overhead remains unmeasured; do not call it zero.
