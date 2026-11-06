# GitCortex 0.8.0 Release Program (Overview)

> **For agentic workers:** This is the index, not an implementation plan. Execute the numbered sub-plans below with superpowers:subagent-driven-development or superpowers:executing-plans. Each sub-plan is independently testable and ships as its own PR.

**Goal:** Ship 0.8.0 only after every reported trust issue, the storage-growth problem, graph export, and dependency hygiene are done and proven. Merged code alone is not enough to release.

**Spec (sources this program argues from):**
- Maintainer requests, 2026-11-05: storage growth with many worktrees, exportable connection graph for front ends, other use cases, Dependabot maintenance, and finishing the work before releasing.
- Maintainer feedback, 2026-10-17 (Codex session): "When GCX is weak" table. An empty or stale graph must never be read as "no dependencies". Omitting `branch` silently answered about another branch. MCP array-result rejection. The `codex/roles-cp3` case: stale index, zero nodes.
- Maintainer requests, 2026-09-07 → 2026-10-20 (Hermes + Codex sessions): beat graphify/codegraph; prove with continuous-coding benchmarks, not one-shot; local call audit without storage growth; usage coaching must not delay launch; release documentation best practices.
- `docs/plans/2026-10-16-pr5-combined-release.md` (untracked; governing PR-5 plan)
- `docs/NEXT-VERSION-PLAN.md`, `docs/CONTINUOUS-CODING-PLAN.md`, `docs/RELEASE-POSITIONING-0.8.md`, `docs/benchmarks/R1-RELEASE-GATE.md`, `docs/DOCUMENTATION-STANDARD.md`

## State on 2026-11-05

| Item | State |
|---|---|
| Last release | v0.7.6, 2026-08-26. Nothing since. |
| Merged, unreleased | #77 indexing, #80 query planning, #82 progressive search, #85 adoption/PR workflow |
| PR 5 (`release/r1-combined-validation`) | 8 commits, **not pushed**: readiness (`index` envelope), watcher recovery, audit log, release-gate script, planning docs |
| CI on `main` | Audit workflow failing since ≥ 2026-10-19 (RUSTSEC-2026-0381/0382/0386/0414/0418, npm dompurify critical) |
| Dependabot | 11 open PRs; every cargo PR red because of the audit failure; #78 (rmcp 2.x major) fails to build |
| Branch protection | None on `main` |
| Local store (maintainer machine) | 19 GB in 9,291 repo dirs; ~8,985 are 1–10 MB test leftovers |

## Feedback → owner plan

| Feedback item | Status in code | Owner |
|---|---|---|
| Stale/missing index looks safe | Readiness states exist (`readiness.rs`); untested across lifecycle | Plan 02 T2–T3 |
| Omitted `branch` answers another branch | Current-checkout resolution exists (`helpers.rs`) | Plan 02 T3 regression |
| Zero nodes for non-checked-out branch (`codex/roles-cp3`) | Not solved: indexer reads only the working tree | Plan 02 T4 |
| "Empty graph ≠ no dependencies" agent rule | Missing from `AGENT_GUIDE` | Plan 02 T5 |
| Risk/unused results treated as conclusions | Unverified | Plan 02 T5 |
| MCP array-result rejection | Normalized on PR 5; untested by a real client | Plan 02 T3 + R1 client lanes |
| Audit must not grow storage | Done (2 × 1 MiB cap) | — |
| Storage grows huge with worktrees | Not solved | Plan 03 |
| Export graph for front ends | Partial (`gcx viz` html/dot/svg/graphml/cypher, `gcx export --format json`) | Plan 04 |
| Dependabot maintenance | Low-volume config only | Plan 01 |
| Beat graphify/codegraph, continuous-coding proof | Harness planned, not run | Release step R3 |
| Release docs best practice | `DOCUMENTATION-STANDARD.md` written | Release step R4 |

## Sub-plans and PR order

| # | Plan | PR base | Why this order |
|---|---|---|---|
| 01 | [Dependency maintenance](2026-11-05-01-dependency-maintenance.md) | `main` | Red audit makes every PR untrustworthy; do it first |
| 02 | [Trust closure](2026-11-05-02-trust-closure.md) | PR 5 branch | Release-blocking feedback; PR 5 is the validation PR |
| 03 | [Storage optimization](2026-11-05-03-storage-optimization.md) | `main` after PR 5 | Touches store identity; needs Plan 02's test-store isolation and tree indexing |
| 04 | [Graph export](2026-11-05-04-graph-export.md) | `main` (parallel with 03) | Independent of storage; uses Plan 02 readiness |

This supersedes the PR-5 plan's rule that "exactly one implementation PR remains". The maintainer widened the 0.8.0 scope on 2026-11-05. Record that decision in `docs/NEXT-VERSION-PLAN.md` when Plan 01 lands.

## Release steps (after 01–04 merge)

- [ ] **R1: Track the governing plan.** Move `docs/plans/2026-10-16-pr5-combined-release.md` to `docs/plans/2026-10-16-pr5-combined-release.md`, commit, and fix the links that point to it.
- [ ] **R2: Freeze the candidate.** Tag nothing. Build `target/release/gcx` from merged `main` and record the commit, binary sha256, and suite sha256 in `tools/agent-bench/r1-release-manifest.json`.
- [ ] **R3: Run the evidence lanes** required by `docs/benchmarks/R1-RELEASE-GATE.md`: retrieval 35 tasks + baseline, `codex-graph-cli`, `claude-code-mcp`, `graphify`, `codegraph`. Run the continuous-coding paired smoke from `docs/CONTINUOUS-CODING-PLAN.md`. Derive the cost cap from the smoke before the 72-session run, and get maintainer approval of the spend. Add a storage lane: Plan 03's before/after table on the fixture. Pass: `python3 scripts/r1_release_gate.py` prints `"ready": true`.
- [ ] **R4: Docs per `docs/DOCUMENTATION-STANDARD.md`.** Update REFERENCE, QUICKSTART, ADOPTION, DEMO_SCRIPT, and the landing page per `RELEASE-POSITIONING-0.8.md`. Fill CHANGELOG from `docs/RELEASE-NOTES-TEMPLATE.md`. Every claim must cite an R3 result id.
- [ ] **R5: Version 0.8.0.** Bump the workspace `Cargo.toml`, `npm/`, and `python/` per `RELEASING.md`. Run `just ci`.
- [ ] **R6: Release.** Follow `RELEASING.md`: merge, tag `v0.8.0`, then verify crates.io, npm, PyPI, Homebrew, and the GitHub release artifacts install and run `gcx doctor` on a clean machine.

## Other use cases (ranked backlog, not in 0.8.0)

Each needs its own plan once 0.8.0 ships. They are ranked by reach per effort.

1. **PR impact comment Action.** A GitHub Action that runs `gcx blast-radius` and posts a Mermaid subgraph (Plan 04 T4) on each PR. It reaches teams and orgs without anyone installing locally. Builds on Plan 02 T4: the base branch is indexed from git objects with no checkout.
2. **Agent usage coaching.** A read-only MCP action that summarizes the local audit log ("you called X 12× after stale responses; call `detect_changes` first"). Design: `docs/AUDIT-UI-PLAN.md`. The maintainer said it must not delay launch.
3. **Change-since checkpoint.** A summary of changed definitions since a saved token. Bet 1 in `NEXT-VERSION-PLAN.md`.
4. **Relevant test candidates.** Bet 2 in `NEXT-VERSION-PLAN.md`.
5. **Architecture boundary checks.** `gcx check` fails CI when a forbidden module dependency appears. It needs a new committed config file, which is a change to the "ignore-only config" rule, so it needs an ADR first.
6. **Branch-delta storage.** Store feature branches as overlays on the base branch. Only if Plan 03 measurements show that branch tables, not worktree copies, dominate.
