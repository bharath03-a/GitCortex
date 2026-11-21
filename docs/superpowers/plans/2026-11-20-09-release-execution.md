# 0.8.0 execution checkpoint — 2026-11-20

This checkpoint updates the status and execution order in [08](2026-11-08-08-ship-and-measure.md). The existing scope and maintainer decisions remain in force. A date is set only after the candidate passes its gates; no tag or public announcement is authorized by this plan.

## Verified state

- The release checkout is `release/r1-combined-validation`. Its working tree was clean before this checkpoint and CI changes.
- Dependency advisory fixes (#92) are merged. There are 13 open draft PRs: #93, #95–#99, #101–#106, and #108. Open PRs are not release evidence.
- Trust work through B7 is in PR #108. The latest review identified the ignore-rules readiness gap, an epoch fast-path omission, non-atomic metadata writes, and three watcher/ignore reports needing reproduction. B8 is still pending.
- Discoverability G has commits through tool descriptions. The `stack/g/4-one-guide` worktree contains uncommitted guide, hook, and editor integration changes; preserve and finish those before starting replacement work.
- Eval infrastructure is in #104–#106. The historical five-task smoke is insufficient to support release performance or usefulness claims. Codex client support and harder tasks remain.
- Storage, graph export, CLI usability, remaining UI work, and the benchmark stack are incomplete. Local branch names alone do not establish completion.
- GitHub reports `main` is not protected (branch-protection API returned 404). Configuration of required checks remains an explicit maintainer decision.

## Delivery order

1. **CI foundation.** Validate the workflow changes below, land them early, and propagate them to each stack base. Diagnose existing red checks rather than carrying forward cancelled or stale successes. Preserve existing security failures until their causes are fixed; do not silence them.
2. **Finish G and trust B8.** Complete the existing G worktree; run its focused tests and full checks. Implement confirmed B8 fixes with regressions, reproduce the remaining reports, and record any rejected findings with evidence. Only one heavy Rust build runs at a time. Use Codex models for development and review.
3. **Review and finish the delivery stacks.** Review V1–V3; finish storage C, export D, CLI F, and UI/product H in the dependency order in plan 08. Keep C5's ADR decision and existing cut-line decisions explicit. Do not expand scope with more use cases.
4. **Integration checkpoint.** Merge bottom-up only after checks and review pass. Run rebuild-equivalence and live lifecycle tests after combined store/watcher changes; verify hook/editor interactions after G and F integration. Check docs commands against real fixtures.
5. **Candidate and evidence.** Complete version changes before freezing the candidate. Record source commit, binary checksum, benchmark suite checksum, model/client versions, fixture revisions, budgets, and result paths. Measure retrieval, continuous coding, discovery, storage, query/index performance, and audit overhead against v0.7.6. Keep failures and negative results. Run an inexpensive Codex smoke before requesting a measured budget for the full paid run. No Claude runs under the current model preference.
6. **Release decision.** Require the release evidence gate to report ready, zero false-safe results, install/update/reindex/rollback checks, supported-client checks, pilot feedback triage, and version-matched documentation. A changed binary invalidates candidate evidence and must be re-measured. Presentation-only docs may reference the pinned binary without changing it.
7. **Publish and verify.** Tag the verified merged candidate following RELEASING.md, verify GitHub artifacts plus crates.io/npm/PyPI/Homebrew installs, then publish approved launch material. Do not expose unreleased install commands on Pages early.

## CI improvements in this checkpoint

- Run CI and Audit for all PR targets, including stacked branches, and for merge-queue events. Add manual CI dispatch for candidate checks.
- Give workflows read-only repository permissions and bounded job timeouts.
- Lint Actions workflows with actionlint before expensive builds.
- Use `npm ci` with the committed cross-platform optional dependencies. Run formatting, lint, unit tests, production build, and bundle-size checks inside the main CI dependency graph; the path-filtered Viz CI remains a fast additional signal.
- Use Cargo's locked dependency resolution for build, lint, docs, and tests.
- Make the binary smoke assert the expected nonzero doctor result and recovery message outside a repository. A crash or unexpected success must fail.
- Expose `CI required`: all applicable main-workflow jobs must succeed; only the explicitly inapplicable macOS job may be skipped. Audit remains separately required.

After these checks run successfully on GitHub, configure required checks as `CI required`, `cargo-deny`, `cargo-audit`, and `npm audit (viz)`. Do not require the path-filtered Viz CI check: GitHub can leave skipped workflows pending. Branch rules are not changed by this checkpoint. CodeQL #101 must be reviewed and integrated separately; this patch does not claim it is active.

## Remaining CI/release acceptance work

- Verify these workflows on Linux runners and on a main push for macOS; local syntax validation is not hosted execution.
- Resolve the workspace's declared Rust 1.80 minimum versus the existing CI 1.95 lane. Establish the actual supported minimum before changing metadata or advertising MSRV coverage.
- Add candidate package/install checks to the release workflow after inspecting channel contracts. The current binary smoke is not an installation test.
- Once V is integrated, verify its full offline suite is discovered by python-tools. Paid agent evals must remain explicit candidate runs, never automatic PR jobs with credentials.
- Keep the broad release evidence gate separate from ordinary PR CI: missing launch evidence should block a release, not every implementation PR.

## Definition of ready

Every in-scope layer is merged with current green checks and reviewed; B8 has no unresolved release-blocking trust findings; final evidence is pinned and reproducible; package and client journeys pass; and the remaining budget, ruleset, pilot, and public-post decisions in plan 08 are resolved. Until then, report 0.8.0 as pending rather than scheduled.

## Local verification of this checkpoint

- actionlint 1.7.7: all workflows pass (embedded ShellCheck disabled; hook ShellCheck remains its own job).
- `git diff --check`: passes.
- `npm ci`, frontend format/lint, 30 Vitest tests, production build, and bundle-size gate: pass. The lockfile remains unchanged.
- Scripts: 55 tests pass. Existing agent-bench: 50 tests pass with Python 3.12.9 invoked directly; the local pyenv shim was killed when invoked from the benchmark directory. Hosted CI uses setup-python instead.
- Aggregate gate: 16 cases cover successful, failed, cancelled, and unexpectedly skipped prerequisites across PR, push, merge-group, and manual events.
- No Rust source changed; the full Rust workspace was not rebuilt for these workflow/documentation edits. Hosted execution, audits, and required-check configuration remain pending.
