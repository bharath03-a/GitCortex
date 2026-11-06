# Delivery Orchestration: Stacked PRs and Model Routing

> **For agentic workers:** This is the delivery runbook for the 0.8.0 program. The chief-of-staff session executes it. Each layer is one task group from a numbered plan, implemented by a delegated child in its own worktree and then reviewed by a different model family.

**Goal:** Deliver plans 01–06 as small, reviewable, stacked PRs on GitHub. Each layer gets independent implementation and review. The chief-of-staff session only routes work, reviews diffs, and makes the calls.

**Spec:** [Program overview](2026-11-05-00-release-program.md). Maintainer, 2026-11-05: use GitHub native stacked PRs (`gh-stack`); use Claude/Codex/Antigravity/Muse models by availability; use subagents; this session orchestrates.

## Global Constraints

- **Verify `gh-stack` first.** Run `gh extension install github/gh-stack && gh stack --help`. If it is unavailable, fall back to manual stacks: each PR's base is the previous layer's branch, and the chief of staff retargets after merges. Record which path was used in this file.
- One layer = one plan task group = one PR, about 400 changed lines or fewer, excluding goldens and lockfiles. Split any layer whose estimate exceeds that **before dispatch**, and record the resulting layers and their SHA edges in the ledger.
- Every GitCortex layer passes `just ci` locally before push. Every `gitcortex-action` layer (stack X) passes its own local gates before push: `node --test`, `actionlint`, and, from X2 on, the real released-gcx/Kuzu integration tests. Layers never merge out of order.
- **Separate repositories.** Stack X lives in the separate `bharath03-a/gitcortex-action` repository. No PR's base is a branch in the other repository, no commit touches both repositories, and a cross-repo dependency is always a **merged SHA or released tag**, never an open branch. Creating that repository and its first push each need their own maintainer approval, separate from GitCortex stack approvals.
- **Maintainer rule (2026-11-05): everything ships as stacked PRs (`gh stack`), pushed without per-push approval; the maintainer reviews stacks later.** Repo creation (`gitcortex-action`), rulesets, closing others' PRs, merges to `main`, and paid benchmark runs still need explicit approval.
- Implementer and reviewer for the same layer are always different model families.
- Children never spawn subagents and never watch PRs. Only the chief of staff watches.

## Stack map

PR 5 (`release/r1-combined-validation`) merges to `main` as an ordinary PR first, after Stack A layer A1 turns the audit green. Every stack below branches from `main` after that.

| Stack | Layers (bottom → top) | Plan | Depends on |
|---|---|---|---|
| **A supply-chain** | A1 advisories + ignore guard → A2 checks run on every PR → A3 ruleset → A4 auto-merge + grouped security | 01 T1–T4 | — |
| A′ rmcp-2 | single PR | 01 T6 | A3 |
| **B trust** | B1 test isolation + harness → B2 commit-path scenarios → B3 live scenarios → B4 tree indexing → B5 agent wording | 02 | PR 5 merged |
| **C storage** | C1 metadata + list → C2 prune → C3 branch GC → C4 compact → C5 ADR + shared worktree store | 03 | B4 merged |
| **D export** | D1 scope refactor → D2 graph JSON → D3 CLI flags → D4 Mermaid → D5 static bundle → D6 docs | 04 | PR 5 merged |
| **E PR impact (GitCortex)** | E1 semantic impact collector → E2 commit indexing + readiness → E3 impact JSON | 05 T1, T2a, T2b | E1: PR 5 merged. E2: B4 and D3 merged (shared `resolve_commit`/`apply_tree_index`/`inspect_commit`; consumes D3's `exit.rs`). E3: E1, E2, and D2 (explicit-union graph-v1 schema) merged |
| E′ impact rollout (GitCortex) | E4 managed workflow pair migration → E5 CLI rollout docs | 05 T9b, T9c | E4: X9 merged and released. E5: E4 merged |
| **X gitcortex-action** (separate repo) | X1 comparisons → X2 checkpoint cache → X3 installer → X4 analyze → X5 composite → X6 validate → X7 render → X8 publish → X9 workflows + docs; X10 policy gates (optional, later) | 05 T4, T5, T6a, T6b, T6c, T7, T3, T8, T9a; T10 | X2: a GitCortex release containing E1–E3. X2 real-Kuzu CI records its E1–E3 merged SHAs; X3 and X9 pin that same exact release |
| **F usability** | per Plan 06 | 06 | per layer, see Plan 06 |

B, D, and F run in parallel. C waits for B4. E2 waits for B4 and D3; E3 waits for D2. X1 can start any time (pure Node, no gcx); X2 onward waits for the E1–E3 release. E4 waits for X9. D4 (Mermaid) and D5/D6 are optional export functionality, not E or X prerequisites: impact comments are rendered only by X7.

**Stack letters.** `A` is the supply-chain stack (Plan 01). The action repository's stack is `X`; Plan 05 uses the same `X1`–`X10` names.

**Shared exit helper owner.** D3 (Plan 04 T3) creates `crates/gitcortex-cli/src/exit.rs` (`CliExit`, `exit_code`). E2 and E3 consume it; neither creates a second exit policy. D3 normally lands first (D starts right after PR 5; E2 also waits for B4). If E2 is otherwise ready earlier, it waits for the merged D3 SHA.

## Model routing

All of these are available in this T3 session (`orchestrator_capabilities`, 2026-11-05). Cursor, Grok, OpenCode, Pi, and fx are disabled.

| Work type | Implementer | Reviewer | Why |
|---|---|---|---|
| Rust store/indexer/daemon (B, C) | Codex `gpt-6.1-sol` high | Claude `claude-opus-5-5` high | Hardest correctness work. Codex wrote PR 5, Claude reviews across families |
| Rust export + CLI (D1–D4, E1–E4) | Claude `claude-sonnet-5-5` high | Codex `gpt-6.1-sol` medium | Well-specified, interface-heavy |
| Frontend static bundle (D5) | Antigravity `gemini-4-argon` high | Claude `claude-sonnet-5-5` | Frontend + vitest |
| CI / Actions / YAML / action Node (A, X) | Codex `gpt-6.1-sol` medium | Claude `claude-sonnet-5-5` | Mechanical, needs `actionlint` |
| Docs and usability copy (D6, E5, F docs) | Muse `muse-wonder-1.0` | Claude `claude-opus-5-5` | Writing-heavy; Opus checks facts against code |
| Plan cross-check before each stack starts | Antigravity `gemini-4-argon` | — | Independent read of plan vs code |
| Final whole-stack review | Claude `claude-opus-5-5` max | — | Most capable model, once per stack |

If a model is unavailable at dispatch time, use the next one in its family; if the whole family is down, use the reviewer's family and pick another family for review. Never fall back silently: log the substitution here.


**Maintainer rule (2026-11-05, overrides the table):** every Codex implementation or review runs on `gpt-6.1-sol`. `gpt-6-astra` is used only for planning work (plan writing and plan revision), because it costs too much for implementation.

**Substitutions:**
- 2026-11-05: Antigravity hit its usage quota (resets ≈ 2026-11-07). Plan 06 authoring moved to Claude `claude-sonnet-5-5` (so its review must use Codex). Until the reset: the D5 implementer is Claude `claude-sonnet-5-5` with reviewer Codex `gpt-6-astra`, and plan cross-checks use Codex `gpt-6-astra`.

- 2026-11-05: Claude API rate limit — Plan 06 re-run on Codex gpt-6.1-sol; cross-plan delta application re-run on Codex gpt-6-astra; reviews of those use Claude after the limit resets.

## Per-layer loop (run by the chief of staff)

1. **Dispatch** the implementer with `delegate_task`, `isolation: "worktree"` (in the layer's own repository). The brief contains the plan file and task numbers, the layer branch name `stack/<stack>/<n>-<slug>`, its parent branch (same repository only), the merged SHAs or release it depends on, the Global Constraints of both this file and the plan, and the required final line: `branch <name>, head <sha>, just ci <pass|fail>` (stack X: `branch <name>, head <sha>, node <pass|fail>, actionlint <pass|fail>, real-gcx <pass|fail|n/a>`).
2. **Gate:** for X2 onward, verify real-Kuzu CI uses the exact GitCortex release recorded below and records its E1–E3 merged SHAs; X3 installer and X9 workflows must pin that same release. For E4 require X9's merged SHA and published action release before dispatch. Read the child's diff (`git diff <parent>...<branch>`). Reject if it is out of scope, has no tests, or has failing tests.
3. **Dispatch the reviewer** (a new `delegate_task`, different family). The brief contains the diff range, the plan task, and the Review Focus lines that task owns. The reviewer returns findings only.
4. **Fix:** small findings the chief of staff fixes directly. Larger ones go back as a new implementer task with the findings attached.
5. **Push** the layer and add it to the stack (`gh stack push`, or `gh pr create --base <parent>`, with `<parent>` always in the same repository). Link the PR. Watch only the bottom unmerged layer.
6. **Merge** when the bottom layer is green and reviewed. Native stacks retarget and rebase the upper layers; on the manual path, the chief of staff rebases and force-pushes with lease.
7. **Record** in the ledger below, including the merged SHA and every SHA/release edge the layer depends on. Before dispatching a layer with a cross-stack or cross-repo dependency, confirm that edge's merged SHA is in the ledger.

## Ledger

| Layer | Repo | Branch | PR | Depends on (merged SHA / release) | Merged SHA | Implementer | Reviewer | Status |
|---|---|---|---|---|---|---|---|---|
| — | — | — | — | — | — | — | — | — |

**Impact release pins** (fill on merge; X2, X3, X9, and E4 read these, never an open branch):

| Pin | Value |
|---|---|
| B4 merged SHA (shared revision resolver/apply/readiness; E2 prerequisite) | — |
| E1 merged SHA | — |
| E2 merged SHA | — |
| E3 merged SHA | — |
| D2 merged SHA (graph-v1 explicit union) | — |
| D3 merged SHA (shared `exit.rs`) | — |
| GitCortex release containing E1–E3 | — |
| X9 merged SHA / action release tag (E4 templates pin this) | — |
