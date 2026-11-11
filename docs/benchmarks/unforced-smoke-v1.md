# Unforced lane smoke, v0.7.6 baseline (2026-11-11)

Purpose: prove the harness end to end and measure what a run costs. This is **not** a release claim: 5 tasks, 1 round, one model, one client, easy tasks.

## Provenance

| Field | Value |
|---|---|
| Lane | `claude-code-unforced` (`agent_run.py --unforced`) |
| Client | Claude Code 2.1.322, `--model sonnet` resolved to `claude-sonnet-5-5` (Haiku appears only as an auxiliary call inside the client), `--effort low` |
| Binary | `gcx 0.7.6`, sha256 `2987096fa6ec692185533b72b2a473d55784b791e1a770288a38cba8fc609a38` |
| Suite | `unforced-suite.toml` sha256 `c243a3c7df9f56bceb767e0853e89279213a2f8a06757cfdd6e017bf3710cbbc`; core `suite.toml` sha256 `e5754f41f3c4a8b9a0ce20d97894fd7430289174782f4281fcf5ddd94b1a2fb1` |
| Repos | cobra `adbc8813901bba65827259daa8e22ff94ec1f30e`, requests `f361ead047be5cb873174218582f7d8b9fcd9f49` |
| Seed / order | 20261111, per-task seeded arm order |
| Raw trace | `results/v1-smoke-v076-20261111T093242Z.unforced.jsonl` sha256 `b4617c7b85ae3c19f867115dd526bc6f8ebac0e668ec39239644442265dc09de` (+ `-logs/` raw client streams; `results/` is git-ignored, kept locally) |
| Harness state | **dirty** (the harness files were uncommitted when this ran); not release-grade provenance |
| Pilot | one extra run before the smoke to check wiring: `v1-pilot-20261111T093132Z`, $0.0492, sha256 `60e9b98d…929bf1`, gcx-bare on `cobra-impact-add-command`, no gcx call |

Arms: `baseline` (no gcx) and `gcx-bare` (gcx MCP server connected, no guidance). The `gcx-guide` arm exists but was not run (smoke scope was 2 arms).

## Spend

| | |
|---|---|
| Smoke (10 runs) | $0.2500 |
| Pilot (1 run) | $0.0492 |
| **Total** | **$0.2992** of a $15 cap |
| Per run | $0.010 to $0.038, mean $0.025 |
| Per run wall time | 4.6 s to 19.9 s, mean 10.9 s |

## Per-run results

| Task | Kind | Arm | Success | Total tok | Uncached tok | Cost | Wall | Turns | Tools | gcx calls |
|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| cobra-impact-add-command | graph | baseline | yes | 36,267 | 4,886 | $0.0324 | 13.3s | 5 | 4 | 0 |
| cobra-impact-add-command | graph | gcx-bare | yes | 48,880 | 5,361 | $0.0370 | 19.9s | 5 | 4 | 0 |
| cobra-callers-add-command | graph | baseline | yes | 65,257 | 6,332 | $0.0383 | 14.4s | 5 | 4 | 0 |
| cobra-callers-add-command | graph | gcx-bare | yes | 43,920 | 2,836 | $0.0218 | 10.9s | 4 | 3 | 0 |
| requests-impact-send | graph | baseline | yes | 23,995 | 4,059 | $0.0269 | 12.4s | 2 | 1 | 0 |
| requests-impact-send | graph | gcx-bare | yes | 24,183 | 4,952 | $0.0311 | 15.0s | 2 | 1 | 0 |
| cobra-restraint-unknown-command | restraint | baseline | yes | 21,329 | 1,486 | $0.0101 | 4.6s | 2 | 1 | 0 |
| cobra-restraint-unknown-command | restraint | gcx-bare | yes | 32,152 | 2,589 | $0.0166 | 7.0s | 3 | 2 | 0 |
| requests-restraint-redirect-limit | restraint | baseline | yes | 34,258 | 4,787 | $0.0245 | 5.8s | 3 | 2 | 0 |
| requests-restraint-redirect-limit | restraint | gcx-bare | yes | 31,630 | 1,423 | $0.0112 | 5.9s | 3 | 2 | 0 |

## Section 4B metrics (gcx-bare arm, n = 5)

| Metric | Result |
|---|---|
| Unprompted adoption rate | **0 / 5** (server connected and `mcp__gcx__gcx` listed in every gcx run) |
| Right-tool rate (graph tasks using gcx) | 0 / 3 |
| Restraint rate (restraint tasks skipping gcx) | 2 / 2, but trivially: the agent never used gcx on any task |
| First gcx call position, tokens before it | none |
| gcx error rate | not defined (0 calls) |
| Not-ready behavior | not measurable: v0.7.6 emits no `index.state`; the metric stays empty rather than guessed |

Agents went to `Grep` first in all 10 runs.

## Section 4A metrics, paired

Success was 5/5 in both arms (all required evidence found, no false "no dependents" claim). Geomean baseline/gcx-bare ratios: total tokens 0.95, uncached tokens 1.26, cost 1.12. Per-task ratios ranged 0.57 to 3.36 **while gcx was never called**, so they measure run-to-run noise plus the cost of carrying an unused tool definition. Single-round ratios must not be cited; the release run needs at least 3 rounds and variance.

## Caveats

- Tasks are small and easy: both arms solve them in 1 to 5 tool calls at low effort, so success cannot discriminate.
- One model, one client, effort `low`; adoption may differ at higher effort or other clients.
- Index was built by `gcx hook` at the pinned commit before the runs; the MCP server resolved the branch itself (agents passed no `branch` because they never called it).
- The v0.7.6 server has no `index.state`, `instructions`, or hooks; this is the "before" number G5 needs.
- Heuristic false-safe and disclosure detectors are regex-based and surface matches for human review.
