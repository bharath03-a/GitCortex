# PR 5 development checkpoint

This is an implementation checkpoint, not a completed release gate or a token
savings claim. Product changes are committed as `0c30320` plus the bounded-state follow-up
`e6841bf` on
`release/r1-combined-validation`; no PR has been opened or release published.

## Implemented

- MCP queries resolve the current checkout immediately and pin that scope for
  each request. Explicit historical branches are checked against their own Git
  revisions. Detached HEAD no longer silently selects main.
- Both MCP modes return object-shaped structured content, including legacy list
  results. Responses carry observed index state, revisions, and generation.
- Missing, stale, unavailable, or failed synchronization cannot produce ordinary
  empty-success or low-risk impact answers. Useful unverified working-tree
  impact evidence is retained with UNKNOWN risk and incomplete-coverage warnings;
  absence-dependent unused-symbol conclusions remain blocked.
- CLI queries and blast-radius analysis reject unusable index scopes. Doctor
  probes the active daemon rather than treating its lock as proof of freshness.
- Watcher ingress and pending batches are bounded. Renames/deletions are handled,
  sustained events have a maximum batch wait, and changed-source probes recover
  coalesced creation events. Startup/scope-change/event-loss recovery reconciles
  disk state; ordinary committed updates remain incremental.
- Exact parser-input blob receipts distinguish existing, deleted, and oversized
  sources. Verification against committed content prevents a dirty parse from
  becoming falsely fresh after a concurrent revert. Overlay uncertainty persists
  separately from commit SHAs and survives daemon shutdown.
- Oversized files invalidate old definitions instead of retaining obsolete code.
- AGENTS.md explicitly forbids interpreting an empty/stale graph as no dependencies.

Readiness describes an observed indexed scope, not exhaustive static or dynamic
coverage. Ignored, oversized, unsupported, and runtime-resolved code may remain
outside the graph. A synchronized dirty tree still carries conservative
unverified status; a richer working-tree checkpoint contract remains future work.

## Validation evidence

- Rust workspace: **350 tests passed**, including initialized compact/full MCP
  sessions, live daemon diagnostics, dirty-parse/revert verification, oversized
  source invalidation, and a continued edit/rename/create/delete/commit/branch/
  restart sequence.
- Clippy with warnings denied and Rust/frontend formatting checks passed.
- Frontend lint, **30 tests**, and production build passed.
- Existing benchmark harness: **50 tests passed**.
- Combined release validator: **24 tests passed**.
- Cargo audit passed against the local advisory database (`--no-fetch`). npm's
  configured high-severity gate passed; one low-severity advisory remains.

These counts reflect this checkpoint; later changes need appropriate revalidation.

## Frozen-binary retrieval smoke

**7/7 Cobra tasks passed** their required-evidence, response-contract, and payload
checks: search, tour, callers, neighborhood, impact, architecture, and refactor.
The single search task reported MRR, precision@5, and file recall of 1.0.

The run used a frozen **debug** binary from `0c30320`, one model-free round, and
a fresh index. The later `e6841bf` resource-bound fix is covered by workspace
tests; this smoke was not repeated for that follow-up.
The current full suite contains **35 tasks**; older 20-task reports are historical
and must not be treated as this candidate's validation.

See the [machine-readable checkpoint](pr5-development-checkpoint.json) for exact
source/binary/suite/repository hashes and environment. The retained local raw
trace is `tools/agent-bench/results/pr5-frozen-debug-cobra-20261017.jsonl`, SHA-256
`1a134141adbc21ecee7cbefe5dc1f45a90f4c89ec9522f07ffeed5c1970c30e2`.
It remains ignored under the existing raw-trace policy and is not available in a
fresh clone. Preserve it in a release evidence archive before using it publicly.

Replay locally:

```bash
python3 tools/agent-bench/bench.py replay \
  tools/agent-bench/results/pr5-frozen-debug-cobra-20261017.jsonl
```

An earlier exploratory full-suite run overlapped mutable binary rebuilds and was
aborted. It is excluded from this evidence. There is no baseline comparison,
causal improvement estimate, total-session token result, or competitor win claim.

## Remaining PR-5 work

1. Finish pinned competitor adapters, installation/runtime manifests, smoke runs,
   and fair results with losses and unsupported workflows retained.
2. Build the immutable release candidate and run the complete pinned retrieval
   suite and baseline comparison. Review current 35-task ground truth explicitly.
3. Run native-client comparisons with at least three rounds and declared cache
   accounting. Add autonomous continuous-coding episodes; the current live-edit
   regression is a deterministic fixture, not proof of end-to-end productivity.
4. Complete old-version migration/re-index, package installation, and rollback
   matrices in disposable environments.
5. Reconcile public claims, bump all package surfaces after validation, and write
   migration/release notes. The compatibility-first target is 0.8.0; package files
   intentionally remain 0.7.6 at this development checkpoint.
6. Populate the real release manifest, pass the combined gate and independent
   reviews, open/merge PR 5, verify merged inputs, and only then publish.

The release validator deliberately fails while candidate evidence is missing.
Its implementation checks attestations, pins, coverage, and checksums; reviewers
still need to inspect scoring and raw evidence.

## Subsequent product experiments

The independent investigation ranked batched/bounded impact traversal with
strongest-path confidence, a generation-bound `changes_since` checkpoint tool,
and evidence-backed `relevant_tests` discovery as the next opportunities.
They remain proposals. Validate them against the
[continuous-coding plan](../CONTINUOUS-CODING-PLAN.md) after trust gates pass.
