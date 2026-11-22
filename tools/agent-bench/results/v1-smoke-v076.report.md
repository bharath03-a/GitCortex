# Eval report: v1-smoke-v076

**Not release evidence:** fewer than 3 rounds. Single-round ratios are noise-dominated and must not be cited.
**Harness provenance is dirty** (uncommitted changes when the run started); not release-grade.

## Provenance

| Trace | Lane | Client | Model | gcx | Suite | Rounds | Runs | Spent | Harness |
|---|---|---|---|---|---|---|---|---|---|
| v1-smoke-v076 | claude-code-unforced | claude 2.1.322 (Claude Code) | claude-sonnet-5-5 | gcx 0.7.6 `2987096fa6ec` | `c243a3c7df9f` | 1 | 10/10 | $0.25 | `f24f8736` dirty |

Non-inferiority margin (success rate): 0.05. Intervals are 95% bootstrap over paired tasks and rounds (seed 0).

## 4A. Task outcome

### Overall

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| all | baseline | 5 | 5/5 (100%) | 0 | 4310 | $0.0264 | 10.1 | 0.00 | n/a |
| all | gcx-bare | 5 | 5/5 (100%) | 0 | 3432 | $0.0236 | 11.7 | 0.00 | n/a |

### By repo

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| cobra | baseline | 3 | 3/3 (100%) | 0 | 4235 | $0.0269 | 10.8 | 0.00 | n/a |
| cobra | gcx-bare | 3 | 3/3 (100%) | 0 | 3595 | $0.0251 | 12.6 | 0.00 | n/a |
| requests | baseline | 2 | 2/2 (100%) | 0 | 4423 | $0.0257 | 9.1 | 0.00 | n/a |
| requests | gcx-bare | 2 | 2/2 (100%) | 0 | 3188 | $0.0211 | 10.4 | 0.00 | n/a |

### By language

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| go | baseline | 3 | 3/3 (100%) | 0 | 4235 | $0.0269 | 10.8 | 0.00 | n/a |
| go | gcx-bare | 3 | 3/3 (100%) | 0 | 3595 | $0.0251 | 12.6 | 0.00 | n/a |
| python | baseline | 2 | 2/2 (100%) | 0 | 4423 | $0.0257 | 9.1 | 0.00 | n/a |
| python | gcx-bare | 2 | 2/2 (100%) | 0 | 3188 | $0.0211 | 10.4 | 0.00 | n/a |

### By workflow

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| callers | baseline | 1 | 1/1 (100%) | 0 | 6332 | $0.0383 | 14.4 | 0.00 | n/a |
| callers | gcx-bare | 1 | 1/1 (100%) | 0 | 2836 | $0.0218 | 10.9 | 0.00 | n/a |
| impact | baseline | 2 | 2/2 (100%) | 0 | 4472 | $0.0297 | 12.9 | 0.00 | n/a |
| impact | gcx-bare | 2 | 2/2 (100%) | 0 | 5156 | $0.0340 | 17.4 | 0.00 | n/a |
| restraint | baseline | 2 | 2/2 (100%) | 0 | 3136 | $0.0173 | 5.2 | 0.00 | n/a |
| restraint | gcx-bare | 2 | 2/2 (100%) | 0 | 2006 | $0.0139 | 6.5 | 0.00 | n/a |

### By client

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| claude | baseline | 5 | 5/5 (100%) | 0 | 4310 | $0.0264 | 10.1 | 0.00 | n/a |
| claude | gcx-bare | 5 | 5/5 (100%) | 0 | 3432 | $0.0236 | 11.7 | 0.00 | n/a |

### By model

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| claude-sonnet-5-5 | baseline | 5 | 5/5 (100%) | 0 | 4310 | $0.0264 | 10.1 | 0.00 | n/a |
| claude-sonnet-5-5 | gcx-bare | 5 | 5/5 (100%) | 0 | 3432 | $0.0236 | 11.7 | 0.00 | n/a |

### By lane

| Group | Arm | Runs | Success | False-safe | Uncached tok | Cost/success | Wall s | Repeat reads | Patch lines |
|---|---|---|---|---|---|---|---|---|---|
| claude-code-unforced | baseline | 5 | 5/5 (100%) | 0 | 4310 | $0.0264 | 10.1 | 0.00 | n/a |
| claude-code-unforced | gcx-bare | 5 | 5/5 (100%) | 0 | 3432 | $0.0236 | 11.7 | 0.00 | n/a |

## 4A. Paired against baseline (baseline/arm; above 1.0x means the arm used less)

### Overall

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| all | gcx-bare | 5 | 0.95x [0.75-1.21] | 1.26x [0.73-2.34] | 1.12x [0.75-1.74] | +0.00 | yes | 1 (n/a) |

### By repo

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| cobra | gcx-bare | 3 | 0.90x [0.66-1.49] | 1.05x [0.57-2.23] | 0.98x [0.61-1.76] | +0.00 | yes | 1 (n/a) |
| requests | gcx-bare | 2 | 1.04x [0.99-1.08] | 1.66x [0.82-3.36] | 1.37x [0.87-2.18] | +0.00 | yes | 1 (n/a) |

### By language

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| go | gcx-bare | 3 | 0.90x [0.66-1.49] | 1.05x [0.57-2.23] | 0.98x [0.61-1.76] | +0.00 | yes | 1 (n/a) |
| python | gcx-bare | 2 | 1.04x [0.99-1.08] | 1.66x [0.82-3.36] | 1.37x [0.87-2.18] | +0.00 | yes | 1 (n/a) |

### By workflow

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| callers | gcx-bare | 1 | 1.49x | 2.23x | 1.76x | +0.00 | yes | 1 (n/a) |
| impact | gcx-bare | 2 | 0.86x [0.74-0.99] | 0.86x [0.82-0.91] | 0.87x [0.87-0.88] | +0.00 | yes | 1 (n/a) |
| restraint | gcx-bare | 2 | 0.85x [0.66-1.08] | 1.39x [0.57-3.36] | 1.15x [0.61-2.18] | +0.00 | yes | 1 (n/a) |

### By client

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| claude | gcx-bare | 5 | 0.95x [0.75-1.21] | 1.26x [0.73-2.34] | 1.12x [0.75-1.74] | +0.00 | yes | 1 (n/a) |

### By model

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| claude-sonnet-5-5 | gcx-bare | 5 | 0.95x [0.75-1.21] | 1.26x [0.73-2.34] | 1.12x [0.75-1.74] | +0.00 | yes | 1 (n/a) |

### By lane

| Group | Arm | Pairs | Total tok | Uncached tok | Cost | Success delta | Non-inferior | Rounds (stdev) |
|---|---|---|---|---|---|---|---|---|
| claude-code-unforced | gcx-bare | 5 | 0.95x [0.75-1.21] | 1.26x [0.73-2.34] | 1.12x [0.75-1.74] | +0.00 | yes | 1 (n/a) |

## 4B. Discoverability

### Overall

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| all | gcx-bare | 0/5 (0%) | 0/3 (0%) | 2/2 (100%) | n/a | n/a | n/a | n/a | 5/5 (100%) |

### By repo

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| cobra | gcx-bare | 0/3 (0%) | 0/2 (0%) | 1/1 (100%) | n/a | n/a | n/a | n/a | 3/3 (100%) |
| requests | gcx-bare | 0/2 (0%) | 0/1 (0%) | 1/1 (100%) | n/a | n/a | n/a | n/a | 2/2 (100%) |

### By language

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| go | gcx-bare | 0/3 (0%) | 0/2 (0%) | 1/1 (100%) | n/a | n/a | n/a | n/a | 3/3 (100%) |
| python | gcx-bare | 0/2 (0%) | 0/1 (0%) | 1/1 (100%) | n/a | n/a | n/a | n/a | 2/2 (100%) |

### By workflow

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| callers | gcx-bare | 0/1 (0%) | 0/1 (0%) | n/a | n/a | n/a | n/a | n/a | 1/1 (100%) |
| impact | gcx-bare | 0/2 (0%) | 0/2 (0%) | n/a | n/a | n/a | n/a | n/a | 2/2 (100%) |
| restraint | gcx-bare | 0/2 (0%) | n/a | 2/2 (100%) | n/a | n/a | n/a | n/a | 2/2 (100%) |

### By client

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| claude | gcx-bare | 0/5 (0%) | 0/3 (0%) | 2/2 (100%) | n/a | n/a | n/a | n/a | 5/5 (100%) |

### By model

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| claude-sonnet-5-5 | gcx-bare | 0/5 (0%) | 0/3 (0%) | 2/2 (100%) | n/a | n/a | n/a | n/a | 5/5 (100%) |

### By lane

| Group | Arm | Adoption | Right-tool | Restraint | 1st call pos | Uncached tok before 1st | Error rate | Not-ready ok | Connected |
|---|---|---|---|---|---|---|---|---|---|
| claude-code-unforced | gcx-bare | 0/5 (0%) | 0/3 (0%) | 2/2 (100%) | n/a | n/a | n/a | n/a | 5/5 (100%) |

## Negative and inconclusive results (never filtered)

- `overall=all` / `gcx-bare` / total_tokens_ratio = 0.953: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `overall=all` / `gcx-bare` / uncached_tokens_ratio = 1.2635: inconclusive: 95% interval [0.726, 2.3367] contains 1.0
- `overall=all` / `gcx-bare` / cost_usd_ratio = 1.12: inconclusive: 95% interval [0.7526, 1.7363] contains 1.0
- `overall=all` / `gcx-bare` / adoption = 0: gcx was available and never used
- `repo=cobra` / `gcx-bare` / total_tokens_ratio = 0.901: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `repo=cobra` / `gcx-bare` / uncached_tokens_ratio = 1.0531: inconclusive: 95% interval [0.574, 2.2327] contains 1.0
- `repo=cobra` / `gcx-bare` / cost_usd_ratio = 0.9773: gcx-bare used more cost_usd than baseline (baseline/arm < 1)
- `repo=cobra` / `gcx-bare` / adoption = 0: gcx was available and never used
- `repo=requests` / `gcx-bare` / total_tokens_ratio = 1.0367: inconclusive: 95% interval [0.9922, 1.0831] contains 1.0
- `repo=requests` / `gcx-bare` / uncached_tokens_ratio = 1.6605: inconclusive: 95% interval [0.8197, 3.364] contains 1.0
- `repo=requests` / `gcx-bare` / cost_usd_ratio = 1.3741: inconclusive: 95% interval [0.866, 2.1802] contains 1.0
- `repo=requests` / `gcx-bare` / adoption = 0: gcx was available and never used
- `language=go` / `gcx-bare` / total_tokens_ratio = 0.901: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `language=go` / `gcx-bare` / uncached_tokens_ratio = 1.0531: inconclusive: 95% interval [0.574, 2.2327] contains 1.0
- `language=go` / `gcx-bare` / cost_usd_ratio = 0.9773: gcx-bare used more cost_usd than baseline (baseline/arm < 1)
- `language=go` / `gcx-bare` / adoption = 0: gcx was available and never used
- `language=python` / `gcx-bare` / total_tokens_ratio = 1.0367: inconclusive: 95% interval [0.9922, 1.0831] contains 1.0
- `language=python` / `gcx-bare` / uncached_tokens_ratio = 1.6605: inconclusive: 95% interval [0.8197, 3.364] contains 1.0
- `language=python` / `gcx-bare` / cost_usd_ratio = 1.3741: inconclusive: 95% interval [0.866, 2.1802] contains 1.0
- `language=python` / `gcx-bare` / adoption = 0: gcx was available and never used
- `workflow=callers` / `gcx-bare` / adoption = 0: gcx was available and never used
- `workflow=impact` / `gcx-bare` / total_tokens_ratio = 0.858: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `workflow=impact` / `gcx-bare` / uncached_tokens_ratio = 0.8643: gcx-bare used more uncached_tokens than baseline (baseline/arm < 1)
- `workflow=impact` / `gcx-bare` / cost_usd_ratio = 0.8707: gcx-bare used more cost_usd than baseline (baseline/arm < 1)
- `workflow=impact` / `gcx-bare` / adoption = 0: gcx was available and never used
- `workflow=restraint` / `gcx-bare` / total_tokens_ratio = 0.8476: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `workflow=restraint` / `gcx-bare` / uncached_tokens_ratio = 1.3895: inconclusive: 95% interval [0.574, 3.364] contains 1.0
- `workflow=restraint` / `gcx-bare` / cost_usd_ratio = 1.1497: inconclusive: 95% interval [0.6063, 2.1802] contains 1.0
- `workflow=restraint` / `gcx-bare` / adoption = 0: gcx was available and never used
- `client=claude` / `gcx-bare` / total_tokens_ratio = 0.953: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `client=claude` / `gcx-bare` / uncached_tokens_ratio = 1.2635: inconclusive: 95% interval [0.726, 2.3367] contains 1.0
- `client=claude` / `gcx-bare` / cost_usd_ratio = 1.12: inconclusive: 95% interval [0.7526, 1.7363] contains 1.0
- `client=claude` / `gcx-bare` / adoption = 0: gcx was available and never used
- `model=claude-sonnet-5-5` / `gcx-bare` / total_tokens_ratio = 0.953: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `model=claude-sonnet-5-5` / `gcx-bare` / uncached_tokens_ratio = 1.2635: inconclusive: 95% interval [0.726, 2.3367] contains 1.0
- `model=claude-sonnet-5-5` / `gcx-bare` / cost_usd_ratio = 1.12: inconclusive: 95% interval [0.7526, 1.7363] contains 1.0
- `model=claude-sonnet-5-5` / `gcx-bare` / adoption = 0: gcx was available and never used
- `lane=claude-code-unforced` / `gcx-bare` / total_tokens_ratio = 0.953: gcx-bare used more total_tokens than baseline (baseline/arm < 1)
- `lane=claude-code-unforced` / `gcx-bare` / uncached_tokens_ratio = 1.2635: inconclusive: 95% interval [0.726, 2.3367] contains 1.0
- `lane=claude-code-unforced` / `gcx-bare` / cost_usd_ratio = 1.12: inconclusive: 95% interval [0.7526, 1.7363] contains 1.0
- `lane=claude-code-unforced` / `gcx-bare` / adoption = 0: gcx was available and never used
