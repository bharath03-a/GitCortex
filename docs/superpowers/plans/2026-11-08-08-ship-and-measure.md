# 0.8.0 Ship Scope, Merge Order, Test Plan, and Metrics

**Status:** proposed 2026-11-08. Extends [00](2026-11-05-00-release-program.md) and [07](2026-11-05-07-delivery-orchestration.md). No result below is claimed for code that has not run; every "after" is a target until a pinned run records it.

## 1. What ships

| Area | Layers | State 2026-11-08 |
|---|---|---|
| Supply chain | A1 (#92), A2, A3, A4, A′ rmcp 2.x | A1 in PR; rest not started |
| Security scanning | CodeQL (#101) | In PR |
| R1 merged work + PR 5 | #77, #80, #82, #85, #93 | Merged / in PR |
| Trust | B1–B3 (#96–#98), B4, **B6**, B5 | B1–B3 in review fixes; B4 uncommitted |
| Storage | C1–C4, C5 (gated by ADR-0005) | Not started |
| Export | D1–D6 | D1 committed; rest not started |
| CLI usability | F0–F5 | F0 partial, uncommitted |
| UI typography | F6 (#95), F7a (#99), F7b, F8 | Two in PR |
| **Agent discoverability (new)** | G0–G5, section 6 | Not started |
| **Product UX + landing (new)** | H1–H6, section 9 | Not started |
| Final acceptance | F9a, F9b | Last |

**B6** is new: fix B3-001 (any uncommitted edit leaves readiness `unverified`) with a persisted overlay checkpoint.

**One release, no prereleases (maintainer decision, 2026-11-08):** 0.8.0 is the first public release of this work. It goes out only with proof of what improved and why someone should use it. Pilot users test a frozen candidate privately.

**Not in the 0.8.0 tag:**
- PR impact Action (X1–X10, E4, E5). The action pins a released `gcx`, so it follows the tag. E1–E3 do ship in 0.8.0 because the action needs them.
- Backlog items in Plan 00: usage coaching, change-since checkpoint, relevant tests, boundary checks, branch-delta storage.

**Cut line if time runs out**, last to first: D5 static bundle, C5 shared worktree store (ship C1–C4 and move C5 to 0.8.1), E1–E3. Never cut trust (B), discoverability (G), or the evidence run.

## 2. Merge order

Each wave needs the previous wave merged unless noted. Inside a stack, bottom layer first.

| Wave | Merge | Why this position |
|---|---|---|
| 0 | #92 A1 → #101 CodeQL → A2 → A3 (needs maintainer OK) → A4 | Turns CI trustworthy. #92 also has the macOS runner fix every other PR needs. A3 before A4: auto-merge without required checks merges unchecked PRs |
| 1 | #93 PR 5 → #96 B1 → #97 B2 → #98 B3 | PR 5 is the base for readiness. B1 stops tests leaking store dirs |
| 1p | #95 F6 → #99 F7a → F7b → F8 | Parallel; touches only `viz/`. F8 needs A2 |
| 2 | B4 → B6 → B5 | B4 fixes unindexed branches. B6 must land before any live-verification claim. B5 wording describes final behavior |
| 2p | F0 (needs #93 only) | Fixes the QUICKSTART command early |
| 3 | D1 → D2 → D3 → D4 → D6 | D3 owns `CliExit`, which F1 and E2 consume |
| 3p | G0 → G1 → G2 → G3 → G4 | Needs B5 wording and F4 hook contract; G3 merges with or after F4 |
| 4 | F1 → F2 → F3 → F4 → F5 | F1 needs D3 and B4. F5 needs D1 moved code |
| 4p | C1 → C2 → C3 → C4 → C5 | C needs B4. C5 stops at the ADR if blocked. C5 changes daemon handshake, so it goes after G0 |
| 5 | E1 → E2 → E3; D5 if kept | E2 needs B4, D3; E3 needs D2 |
| 6 | F9a → F9b → G5 evidence | Docs and journeys describe final behavior |
| 4u | H1 → H2 → H3 (product UX) | Parallel with wave 4; needs F-ui tokens and F2 onboarding text |
| 7 | Freeze candidate, private pilot, run section 4 evidence, H4–H6 with real numbers, version bump, then merge landing pages together with the tag | Evidence must be on the exact release binary. Pages deploys all of `docs/` on every push to `main`, so landing and doc changes merged earlier go live before the release exists |
| 8 | X1–X9 → E4 → E5 | After tag; separate repo |

**Risky pairs to re-test after rebase:** F0 with E3 (`blast_radius`); D3 with F1 (`exit.rs`); C5 with G0 (handshake, server state); F4 with G3 (hook files).

**Merge rule:** no merge on a red check. A stale cancelled run is not a pass: re-run it.

## 3. Test plan

| Level | What | Command / source | When |
|---|---|---|---|
| L0 | fmt, clippy, unit, viz lint, vitest | `just ci`, `-j 4` locally | Every layer |
| L1 | Incremental equals rebuild, including reference facts | `trust_commit_path.rs` | B layers, then every store/indexer change |
| L2 | Live MCP lifecycle: edit, delete, switch, restart, detached, not-ready | `trust_live.rs`, run under `nextest` as CI does | B, C5, G0 |
| L3 | CLI journey on clean fixture; every documented command executes | F9b journey tests | Final |
| L4 | Viz computed-style gate, contrast, bundle ≤ 2,560,000 bytes | F8 | F-ui |
| L5 | Storage and hook budgets on fixture (section 4C) | `gcx storage list --json`, `GCX_TIMING=1` | C, B4 |
| L6 | Discoverability evals (section 4B) | New unforced lane, G5 | G, release |
| L7 | Agent retrieval + continuous-coding + competitor lanes | `tools/agent-bench`, `r1_release_gate.py` | Candidate only |
| L8 | Install, update, reindex, rollback on clean machine for all 4 channels; `gcx doctor` | `RELEASING.md` | Before and after tag |
| L9 | Real clients: Claude Code, Codex, Cursor, Antigravity connect, list tools, answer one query | Manual checklist with recorded output | Candidate |

**Before/after discipline:** "before" is the published v0.7.6 binary, not an older build. Run both arms on the same pinned repos, same model, same client, same budgets, alternating order. Keep failed and negative runs.

## 4. Metrics

Correctness outranks speed and tokens. A cost win with weaker answers is a loss.

### A. Task outcome (retrieval + continuous-coding lanes)

| Metric | Type | Gate |
|---|---|---|
| Task success / hidden-test pass rate | Primary | Non-inferior to baseline on every lane |
| Answer quality vs required evidence | Primary | Non-inferior |
| False-safe rate: claimed "no dependents" when dependents exist | Primary | 0 |
| Total tokens and **uncached** tokens, per task (geomean ratio) | Primary | Uncached ≥ 1.0× overall; report every workflow losing |
| Cost per successful task | Primary | Report |
| Wall time, turns, tool calls, retries | Secondary | Report |
| Repeat exploration: same file or symbol re-read across a session | Secondary | Lower than baseline on continuous lane |
| Patch quality (diff size, review pass) | Guardrail | Non-inferior |

Report by repo, language, workflow, client, and model. Report variance (rounds ≥ 3) before any claim. Compare `baseline`, `gcx`, `graphify`, `codegraph` on the same tasks.

Known weak spots to beat, from the older Claude gate on an old build (`claude-agent-gate-v1.md`): the `search` workflow at 0.67× uncached, and `hono` at 0.90× uncached. Confirm the ratio direction before citing them; 0.8.0 must re-measure both.

### B. Discoverability (new; no existing lane measures this)

The existing agent lanes **require** exactly one GitCortex call, so they prove value when used but say nothing about whether agents choose it.

| Metric | Definition | Target for first run |
|---|---|---|
| Unprompted adoption rate | Tasks with ≥ 1 gcx call when the prompt never mentions it | Baseline first, then improve |
| Right-tool rate | Graph-suited tasks (callers, impact, path) that use gcx | ≥ 80% |
| Restraint rate | Exact-string or config tasks that correctly skip gcx | ≥ 80% |
| First-useful-call position | Tool-call index of first gcx call | Earlier than first broad grep |
| Discovery cost | Tokens before the first gcx call; `tools/list` bytes in compact vs full | Report, cap in test |
| Wrong-action / schema error rate | Failed calls per gcx call | < 5% |
| Correct not-ready behavior | Non-ready `index.state` → agent falls back and says so | 100% |
| Connection success | Client connects and lists tools, per client | 100% on supported clients |

### C. Product and engineering

| Metric | Target |
|---|---|
| Store bytes: fixture of 1 repo, 3 worktrees, 5 branches, 10 detached checkouts | ≥ 50% lower than v0.7.6 |
| Test runs leaking dirs into the real store | 0 |
| Hook p95, no GC due | No regression; GC check < 50 ms |
| Commands from install to first useful answer | ≤ 3, timed |
| Failure messages with an executable next step | 100% of audited paths |
| Smallest UI text; body contrast | ≥ 12px; WCAG AA |
| Audit-off overhead; audit-on p50/p95/p99 | Measured, never called zero |

## 5. Pain points: before and after

| Pain point | Before (evidence) | After (target) | Proof |
|---|---|---|---|
| Stale or empty graph reads as "safe" | 2026-10-17 feedback | Non-ready never returns items as safe; agents told so | L2, B5 |
| Omitted `branch` answers another branch | 2026-10-17 feedback | Answers current checkout, with scope shown | L2 |
| Branch not checked out returns zero nodes | `codex/roles-cp3` case | Indexed from git objects; `indexing` then `ready` | B4 tests |
| Uncommitted edits stay `unverified` forever | B3-001 | Verified after settling, when disk bytes match | B6 test |
| Documented quickstart command fails | `index missing on branch 'HEAD'` | Works on a clean repo | F0, L3 |
| `gcx clean` outside a repo can wipe a store | Plan 06 review | Refuses; no store touched | F1 test |
| `gcx status` reports success on lost store or bad ref | Plan 06 audit U1 | Explicit state and exit code | F1 test |
| Generated Read hooks call a nonexistent command | U3: `gcx query context` | Valid command, correct client output format | F4, G3 |
| Storage grows without bound | 19 GB, 9,291 dirs on maintainer machine | Shared store, GC, prune, compaction | Section 4C |
| No scoped export for front ends | Only full dumps | `--seed --direction`, versioned JSON, Mermaid, static viewer | D tests |
| Tiny, low-contrast UI | 11px labels, sub-AA gray | ≥ 12px, AA | L4 |
| Red CI, unmanaged dep PRs | Audit red since ≥ 2026-10-19; 11 open PRs | Green, required checks, auto-merge | Wave 0 |
| Agents don't reach for the tool | This delivery session used grep throughout; MCP showed `Connection closed` | Measured adoption, reliable connect | Section 4B |

## 6. Discoverability: Stack G

Plans 01–07 cover tool correctness, not whether agents pick the tool. Agents decide from a short list of signals: server instructions, tool name and description, project guide, hooks, and what the last response taught them.

**Findings in the code today:**
- No MCP server `instructions` string exists in `crates/gitcortex-mcp/src`, so that channel is unused.
- Clients may defer tools behind search (`claude-agent-gate-v1.md`). A tool named only `gcx` must carry the words agents search for.
- Guidance differs per editor: `AGENT_GUIDE` plus separate generators in `crates/gitcortex-cli/src/cmd/init/editors/`.
- The Claude, Codex and Antigravity Read hooks call a command that does not exist (U3), so passive context silently never arrives.
- `gitcortex` showed `CONNECTION_CLOSED` repeatedly in this delivery session. Cause not yet investigated.

| Layer | Deliverable | Pinning test |
|---|---|---|
| **G0** Reliable connect | Root-cause `CONNECTION_CLOSED`; `gcx doctor` reports connect failure with next step | `client_connect_survives_daemon_restart`; `doctor_reports_unreachable_server` |
| **G1** Server instructions | `get_info` returns ≤ 1,000 characters: when to use, when not, how to read `index.state`, verify in source | `initialize_returns_bounded_instructions` |
| **G2** Tool text | Descriptions state *when* in task language ("before renaming", "who calls X"). The compact tool lists its actions with searchable words; byte budget enforced | `tool_descriptions_state_when_to_use`; `compact_tools_list_under_budget`; `deferred_search_terms_match_compact_tool` |
| **G3** One guide, all clients | Single source generates guidance for all supported editors; valid hooks (with F4); idempotent `gcx init` block | `editor_guides_share_core_rules`; `generated_hooks_run_successfully` |
| **G4** Teaching responses | Results carry a bounded `next` hint; empty results say whether empty means absent or not indexed; unused capability never advertised | `empty_result_distinguishes_absent_from_unindexed`; `next_hint_names_real_action` |
| **G5** Adoption lane | `agent_run.py --unforced` removes the one-call requirement and records section 4B; arms: no guidance, guide only, guide + instructions + hooks | `unforced_lane_counts_adoption_without_prompt_mention` |

Run a cheap G5 smoke **before** G1–G4 for a real "before" number, then again after. Keep each signal only if it moves adoption without harming restraint or outcomes. If a signal doesn't help, delete it: guidance costs tokens on every session.

**Human usability, same principle:** `gcx init` ends with three copyable commands; `gcx --help` leads with questions, not internals; every failure names the next command; `gcx doctor` is the single recovery entry.

## 7. Release decision

Ship 0.8.0 when all are true:
1. Waves 0–6 merged with required checks green.
2. `python3 scripts/r1_release_gate.py` prints `"ready": true` on the frozen candidate.
3. Section 4A correctness gates pass on every lane; false-safe rate is 0.
4. Section 4B has before and after numbers, published even if weak.
5. Section 4C targets met or the miss is written in release notes.
6. L8 and L9 pass on the candidate.
7. Private pilot finished, with feedback triaged.
8. Landing, compare, and benchmark pages use only recorded results, and the launch kit in section 10 is ready.

Release notes cite result ids only, keep negative results, and use the section 5 table as the changelog's core.

## 9. Product UX and landing refresh: Stack H

A slight redesign, not a rebuild. Keep the current identity and viz architecture. Implementation: `frontend-design` guidance, screenshot review at each layer.

| Layer | Deliverable | Proof |
|---|---|---|
| **H1** Viz first run | Opens on an answer, not an empty canvas: "Who calls X?", "What changes if I edit X?". Clear empty, loading, stale, and error states with the next command | Playwright journey with screenshots; states covered by tests |
| **H2** Investigation flow | Search → symbol → callers and callees → source location in the fewest clicks; readiness and branch always visible; keyboard path | Click count before vs after; screenshots |
| **H3** Graph legibility | Initial framing fills the canvas (today a small cluster sits in a large empty area), sensible default zoom, clearer legend, readable at small and large graph sizes | Before/after screenshots on fixtures of two sizes |
| **H4** Landing page | `docs/index.html`: lead with the problem and proof; demo showing edit → query → branch switch → recovery; honest "use with search and tests" framing per `RELEASE-POSITIONING-0.8.md`; version-matched install | Links, a11y, Lighthouse; every number cites a result id |
| **H5** Proof pages | `docs/benchmarks.html`, `compare*.html`: 0.8.0 results vs v0.7.6 and competitors, method, negative results, one command to reproduce; older numbers clearly dated | Numbers generated from result files, not hand-typed |
| **H6** "Why use this" | One short page by role: developer, agent user, team lead; concrete before/after from section 5; when *not* to use it | Reviewed against section 7 claims rule |

H4–H6 are drafted as early as useful but merged only with the tag.

## 10. Reaching users

Goal: developers using AI coding agents on real repos, then their teams. Audience first, volume later. Nothing is posted without maintainer approval.

**Why someone would try it:** the message is the measured result, not the feature list: "here's what changed for agents, here's how to reproduce it, and here's where it doesn't help." Final wording waits for section 4 results.

| Phase | Action | Output |
|---|---|---|
| Before launch | Private pilot with 2–5 developers on the frozen candidate (`docs/PILOT-GUIDE.md`) | Setup friction, quotes with permission, recorded before/after |
| Before launch | Launch kit: 60–90 s demo, launch post on what we measured (including what failed), short posts per channel, comparison facts, FAQ | Ready before tag |
| Launch day | Tag, verify all channels install, landing live, then Show HN and the launch post | One coordinated moment |
| Launch week | Developer communities where this problem is live: r/rust, r/ClaudeAI, r/cursor, agent and MCP communities. Answer every question | Tailored post per community, no copy-paste |
| Launch week | Listings: official MCP registry, awesome-mcp-servers, MCP directories, and editor plugin or extension marketplaces where we qualify | Check each listing's rules first |
| After launch | PR impact Action (wave 8): reviewers see impact on every PR without installing anything; this is the team and org channel | Each install reaches a whole team |
| After launch | Outreach to a few maintainers of repos in the benchmark suite, sharing their repo's results and asking for critique; only if useful to them | Credible feedback |
| After launch | Follow-up posts from real data: "does an agent choose a code graph when not told to?" is useful to others even when the numbers are mixed | Ongoing reach |

**Measuring reach without telemetry** (local-first stays true): registry downloads per channel, Pages visits if an analytics choice is approved, stars, issues, Action installs, referral per channel, pilot repeat use after 1–2 weeks. Local audit data is shared only by explicit opt-in.

**Rules:** no claim without a result id. No "better than X" beyond what the compare pages show. Disclose that we built it when comparing. No mass unsolicited messages.

## 8. Maintainer decisions needed

1. Keep C5 (shared worktree store) in 0.8.0 if the ADR passes. Recommended: yes; it was the original storage request.
2. Keep D5 (static viewer) in 0.8.0. Recommended: yes unless it delays; first to cut.
3. Approve A3 ruleset when Wave 0 reaches it.
4. Set the paid benchmark budget. Run the smoke first, then set a cap from measured cost.
5. Approve the launch kit and each public post before it goes out.
6. Name 2–5 pilot developers, or say how to find them.
