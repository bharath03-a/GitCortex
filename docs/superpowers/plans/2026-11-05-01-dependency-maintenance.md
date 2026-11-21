# Dependency Maintenance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dependabot PRs merge themselves when they are safe (patch or minor, including security fixes, with all checks green). Majors and unclassifiable updates wait for a human. `main` stays audit-clean.

**Architecture:** Fix the current advisories first. Then make every check run on every PR so it can be required, protect `main` with a ruleset, and add a workflow that enables GitHub auto-merge on qualifying Dependabot PRs. A small script keeps the two advisory ignore lists from drifting.

**Tech Stack:** GitHub Actions, rulesets API, `dependabot/fetch-metadata@v2`, `gh`, cargo-deny, cargo-audit, npm audit, Python 3 unittest.

**Spec:** [Program overview](2026-11-05-00-release-program.md). Maintainer choice 2026-11-05: "Auto-merge behind checks". Existing intent is in `.github/dependabot.yml` header comments.

## Global Constraints

- Auto-merge is an **allowlist**: only `update-type` `version-update:semver-patch` or `version-update:semver-minor`. Major, empty/null, or any unknown value is labelled (`major-update` or `needs-review`) and handled manually.
- Auto-merge is only *enabled* while `main` has required status checks. The guarantee is narrow: the guard runs when auto-merge is enabled and on an hourly sweep. If protection is removed later, the sweep disables auto-merge on every open Dependabot PR; the residual window is ≤ 1 h and `docs/MAINTENANCE.md` says so. Admins disable auto-merge before editing the ruleset.
- No `pull_request_target`. Use `pull_request` (plus `schedule`/`workflow_dispatch` for the sweep) with an explicit `permissions:` block.
- `deny.toml` `[advisories].ignore` and `.cargo/audit.toml` `[advisories].ignore` contain the same IDs. Each ID has a same-line `#` reason.
- Closing PRs, applying rulesets, and deleting branches are outward-facing. Confirm with the maintainer before each.
- Branch for Tasks 1–4: `chore/dependency-maintenance` from `main`.

## Review Focus

1. A required check whose workflow does not trigger on a PR leaves that PR blocked forever. Test: a docs-only draft PR reaches mergeable (Task 2 Step 4).
2. The ruleset is deleted after auto-merge was already enabled, and no PR event re-runs the guard. Test: T4 `test_sweep_disables_auto_merge_when_unprotected`, `test_guard_zero_checks_refuses_merge`.
3. A mixed group containing a major, or metadata with a null/unknown `update-type`, must not auto-merge; a failed metadata step must never reach `gh pr merge`. Test: T4 `test_mixed_major_group_not_merged`, `test_unknown_update_type_needs_review`, `test_missing_metadata_exits_without_merge` (stubbed `gh`).
4. The ignore lists drift (today `RUSTSEC-2026-0202` is only in `.cargo/audit.toml`). Test: Task 1 script.
5. The ruleset blocks release automation that pushes to `main`. Check: Task 3 Step 2.

---

### Task 1: Restore a green audit and guard the ignore lists

**Files:**
- Create: `scripts/check_advisory_ignores.py`, `scripts/test_check_advisory_ignores.py`
- Modify: `Cargo.lock`, `viz/package-lock.json`, `deny.toml`, `.cargo/audit.toml`, `.github/workflows/ci.yml` (python-tools job)

**Interfaces:**
- Produces: `load_ignores(path: Path) -> dict[str, str]` (ID → reason), and `main(deny: Path, audit: Path) -> int` (0 ok, 1 mismatch, missing reason, or an ID that does not match `RUSTSEC-\d{4}-\d{4}`; prints the offending IDs).

- [ ] **Step 1: Write failing tests** in `scripts/test_check_advisory_ignores.py` (fixtures use well-formed IDs):
  - `test_identical_lists_pass`: both files list `RUSTSEC-2024-0001` with reasons → `main(...) == 0`
  - `test_id_only_in_audit_fails`: audit has an extra `RUSTSEC-2024-0002` → returns 1, stdout contains `RUSTSEC-2024-0002`
  - `test_missing_reason_fails`: `"RUSTSEC-2024-0001",` with no comment → returns 1, stdout contains `missing reason`
  - `test_malformed_id_fails`: `"RUSTSEC-1"` (in the TOML list, unmatched by the regex) → returns 1, stdout contains `malformed`
  - `test_real_repo_files`: run against the repo's `deny.toml` and `.cargo/audit.toml` → 0
- [ ] **Step 2: Run** `python3 -m unittest scripts/test_check_advisory_ignores.py -v`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `scripts/check_advisory_ignores.py` with `tomllib` for the IDs and a line regex `"(RUSTSEC-\d{4}-\d{4})",\s*#\s*(\S.*)` for the reasons.
- [ ] **Step 4: Run tests.** Expected: the first 4 pass and `test_real_repo_files` FAILS on `RUSTSEC-2026-0202`. Add that ID to `deny.toml` with the reason copied from `.cargo/audit.toml`. Rerun → all pass.
- [ ] **Step 5: Update the locked deps.** Run `cargo update -p tokio -p rustls-webpki -p libz-sys -p pulldown-cmark -p webpki-roots@0.26.11 -p webpki-roots@1.0.7`. Then run `cargo deny check advisories` and `cargo audit`. Expected: no errors. Any advisory with no fixed version goes into **both** ignore files with the reason and the upstream issue URL.
- [ ] **Step 6: Fix npm.** In `viz/`, run `npm audit fix`, then `npm audit --omit=dev --audit-level=high`. Expected: exit 0 (fixes dompurify ≤ 3.4.16).
- [ ] **Step 7: Wire into CI.** Add a step to `python-tools` in `ci.yml`: `python3 scripts/check_advisory_ignores.py deny.toml .cargo/audit.toml`.
- [ ] **Step 8: Run** `just ci`. Expected: PASS.
- [ ] **Step 9: Commit.** `fix: clear open advisories and guard advisory ignore drift`

### Task 2: Make audit and viz checks run on every PR

**Files:**
- Modify: `.github/workflows/audit.yml`, `.github/workflows/viz-ci.yml`

- [ ] **Step 1:** In `audit.yml`, delete `paths:` under `pull_request`. Add `"viz/package.json"` and `"viz/package-lock.json"` to `push.paths`. (npm audit currently never runs on viz-only PRs.)
- [ ] **Step 2:** In `viz-ci.yml`, delete `paths:` under `pull_request`. Add a `changes` job using `dorny/paths-filter@v3` with filter `viz: ['viz/**', '.github/workflows/viz-ci.yml']`. Give the existing job `needs: changes` and `if: needs.changes.outputs.viz == 'true'`. A skipped job counts as passing for required checks.
- [ ] **Step 3: Run** `actionlint .github/workflows/*.yml`. Expected: no errors.
- [ ] **Step 4:** Push the branch and open a **draft** PR. On that PR, `gh pr checks` lists `cargo-deny`, `cargo-audit`, `npm audit (viz)` as pass, and `viz · lint + test + build` as pass or skipping.
- [ ] **Step 5: Commit.** `ci: run audit and viz checks on every pull request`

### Task 3: Ruleset for `main`

**Files:**
- Create: `.github/rulesets/main.json`, `scripts/apply_ruleset.sh` (`gh api -X POST repos/{owner}/{repo}/rulesets --input .github/rulesets/main.json`, or PUT if one named `main` exists)

- [ ] **Step 1: Write `main.json`.** Target `refs/heads/main`. Rules: `deletion`, `non_fast_forward`, and `required_status_checks` with `strict_required_status_checks_policy: false`. Contexts, verbatim: `typos · spell check`, `shellcheck · hook scripts`, `python · tooling tests`, `viz · build bundle`, `rust · fmt + clippy`, `rust · doc`, `rust · unused deps (machete)`, `rust · ubuntu-latest · stable`, `rust · ubuntu-latest · 1.95`, `cargo-deny`, `cargo-audit`, `npm audit (viz)`, `viz · lint + test + build`. Bypass: `{"actor_type":"RepositoryRole","actor_id":5,"bypass_mode":"always"}` (admin).
- [ ] **Step 2: Check release automation.** Run `grep -n "git push\|contents: write" .github/workflows/release.yml .github/workflows/publish*.yml`. Any push to `main` must come from the admin token or be removed. Record the result in the PR description.
- [ ] **Step 3: Apply.** After maintainer confirmation, run `scripts/apply_ruleset.sh`. Verify: `gh api repos/bharath03-a/GitCortex/rules/branches/main` lists `required_status_checks`.
- [ ] **Step 4: Commit.** `ci: protect main with required checks`

### Task 4: Dependabot auto-merge workflow and grouped security updates

**Files:**
- Create: `.github/workflows/dependabot-auto-merge.yml`, `scripts/dependabot_auto_merge.py`, `scripts/test_dependabot_auto_merge.py`, `scripts/fixtures/dependabot/*.json` (recorded `fetch-metadata` outputs), `docs/MAINTENANCE.md`
- Modify: `.github/dependabot.yml`, `.github/workflows/ci.yml` (python-tools runs the new tests), `CONTRIBUTING.md` (link to MAINTENANCE)

**Interfaces:**
- Produces (`scripts/dependabot_auto_merge.py`; every `gh` call goes through `--gh <path>` so tests can stub it):
  - `decide(update_type: str | None) -> Literal["merge", "major", "review"]`: `merge` only for `version-update:semver-patch` / `version-update:semver-minor`; `major` for `version-update:semver-major`; `review` for empty, null, or anything else.
  - `guard(gh: str, repo: str, ruleset: Path) -> bool`: true iff `gh api repos/<repo>/rules/branches/main` has a `required_status_checks` rule containing every context in `.github/rulesets/main.json`.
  - `enable(gh, repo, pr_url, update_type) -> int`: guard first (exit 1, `::error::main has no required checks; refusing auto-merge`), then `gh pr merge --auto --squash` for `merge`, or label `major-update` / `needs-review` plus a comment otherwise. Exit 1 if `update_type` is absent (metadata missing).
  - `sweep(gh, repo, ruleset) -> int`: if `guard` fails, `gh pr merge --disable-auto` and comment on every open Dependabot PR whose `autoMergeRequest` is non-null; exit 1 so the run is red and notifies.

- [ ] **Step 1: Group security PRs.** In `dependabot.yml`, add a second group per ecosystem with `applies-to: security-updates` and `patterns: ["*"]`, so security fixes arrive batched instead of one PR per package (today: #87, #90, #91).
- [ ] **Step 2: Write failing tests** in `scripts/test_dependabot_auto_merge.py`, with a fake `gh` script that logs its argv to a file and replays canned API JSON. Inputs are recorded `fetch-metadata@v2` outputs (`update-type` is the highest change in the PR; it is empty for non-semver versions such as SHA-pinned actions):
  - `test_patch_and_minor_merge`
  - `test_mixed_major_group_not_merged` (group fixture whose output is `version-update:semver-major`): label `major-update`, no `merge` in the log
  - `test_unknown_update_type_needs_review` (empty string and `version-update:semver-other`)
  - `test_missing_metadata_exits_without_merge`: no `--update-type` → exit 1, log has no `merge`
  - `test_guard_zero_checks_refuses_merge`, `test_guard_missing_context_refuses_merge`
  - `test_sweep_disables_auto_merge_when_unprotected`, `test_sweep_noop_when_protected`
- [ ] **Step 3: Run** `python3 -m unittest scripts/test_dependabot_auto_merge.py -v`. Expected: FAIL (module missing).
- [ ] **Step 4: Implement** the script with the signatures above, and the workflow:
  - Job `enable`: `on: pull_request` (`opened, synchronize, reopened`), `if: github.event.pull_request.user.login == 'dependabot[bot]'`, `permissions: { contents: write, pull-requests: write }`. Steps: `dependabot/fetch-metadata@v2` (id `meta`, no `continue-on-error`), then `python3 scripts/dependabot_auto_merge.py enable --update-type "$UPDATE_TYPE" ...` with `env: { UPDATE_TYPE: ${{ steps.meta.outputs.update-type }} }` (never inline `${{ }}` in `run:`) and `if: steps.meta.outcome == 'success'`.
  - Job `sweep`: `on: schedule` (`cron: '17 * * * *'`) and `workflow_dispatch`, same permissions, runs `... sweep`. This rechecks PRs whose auto-merge was enabled before protection changed.
- [ ] **Step 5: Run** the tests. Expected: PASS. Then `actionlint .github/workflows/dependabot-auto-merge.yml`. Expected: no errors. Add the test file to `python-tools` in `ci.yml`.
- [ ] **Step 6: Write `docs/MAINTENANCE.md` "Dependency updates":** what auto-merges (patch/minor allowlist), the narrow guard guarantee and its ≤ 1 h sweep window, "disable auto-merge before editing the ruleset", what to do when a Dependabot PR is red (it is never bypassed), the monthly majors and `needs-review` triage, and the ignore-list policy (Task 1).
- [ ] **Step 7: Commit, then mark the PR ready.** `ci: auto-merge safe dependabot updates`. After merge, verify on the next Dependabot PR: `gh pr view <n> --json autoMergeRequest` is non-null, and save that run's real `steps.meta.outputs` as a new fixture. Trigger `sweep` once via `workflow_dispatch` and confirm it is a green no-op.

### Task 5: One-time backlog triage (after Tasks 1–4 merge; confirm each close with the maintainer)

- [ ] **Step 1:** Comment `@dependabot recreate` on group PRs #89 (cargo) and #88 (viz). Expected: both regenerate against the fixed `main`, go green, and auto-merge.
- [ ] **Step 2:** For each single PR #91, #90, #87, #86, #83, #75, #73: if `main` or the regenerated group already has a version ≥ the PR's target, close it with "Superseded by #<n> / main". Otherwise leave it for auto-merge.
- [ ] **Step 3:** Close #74 (stale manual combine) and delete branch `deps/combined-update` (local and remote).
- [ ] **Step 4: Verify.** `gh pr list --author app/dependabot --state open` shows only regenerated groups and #78.

### Task 6: rmcp 2.x migration (closes #78)

**Files:**
- Modify: `crates/gitcortex-mcp/Cargo.toml`, the `crates/gitcortex-mcp/src/mcp/*.rs` files the compiler flags, and `crates/gitcortex-cli/src/cmd/serve.rs` if the handshake types moved

- [ ] **Step 1:** Branch `chore/rmcp-2` from `main`. Read rmcp's CHANGELOG from 1.8 to 2.1 and list the breaking changes that touch `ServerHandler`, tool routing, `structured_content`, and transports in the PR description.
- [ ] **Step 2:** Bump `rmcp` and run `cargo check -p gitcortex-mcp -p gitcortex-cli`. Fix errors without changing tool names or response shapes.
- [ ] **Step 3: Run** `cargo test -p gitcortex-mcp --test readiness_protocol --test full_pipeline` and `cargo test -p gitcortex-cli --test daemon_multiplex`, plus `cargo test -p gitcortex-cli rewrites_antigravity_server_discover_to_initialize`. Expected: PASS.
- [ ] **Step 4: Manual check.** `gcx serve` lists tools in Claude Code and in Codex (`tools/list` shows `gcx`).
- [ ] **Step 5: Commit.** `chore(deps): migrate to rmcp 2.x`. Close #78 as superseded.
