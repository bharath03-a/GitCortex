# Codex Agent Gate v1

Pinned single-round agent-loop validation of the agent-first response contracts. This uses the cheapest Codex model available to the current ChatGPT account and the explicit `codex-graph-cli` lane—not MCP, because this Codex exec environment did not expose configured ad-hoc MCP tools.

Compared with the earlier exploratory Codex run (1.20× total, 1.31× uncached), the pinned valid-trace gate reaches 1.81× total and 2.65× uncached with non-inferior required source evidence on all 20 tasks. The task manifests and validity rules also changed, so this comparison is directional rather than a controlled phase delta; the model-free base/head gate in [`agent-retrieval-v1.md`](agent-retrieval-v1.md) isolates the response-contract change.

Model: `gpt-5.4-mini`  
Samples: 20 (20 valid)  
Total-token geomean: **1.81×**  
Uncached-token geomean: **2.65×**  
Quality non-inferior: **20/20**  
≤3 verification commands: **15/20**

## By repository

| Repo | Valid | Total | Uncached | Command budget |
|---|---:|---:|---:|---:|
| cobra | 4/4 | 1.62× | 2.10× | 4/4 |
| gson | 4/4 | 1.94× | 2.93× | 4/4 |
| hono | 4/4 | 1.82× | 2.30× | 2/4 |
| requests | 4/4 | 1.64× | 3.06× | 3/4 |
| ripgrep | 4/4 | 2.07× | 2.99× | 2/4 |

## By workflow

| Workflow | Valid | Total | Uncached | Command budget |
|---|---:|---:|---:|---:|
| callers | 5/5 | 1.85× | 2.36× | 3/5 |
| search | 5/5 | 1.35× | 1.75× | 5/5 |
| subgraph | 5/5 | 2.11× | 3.53× | 3/5 |
| tour | 5/5 | 2.04× | 3.35× | 4/5 |

## Methodology and limitations

- Five repositories are pinned by commit in `tools/agent-bench/suite.toml`.
- Each task runs an ordinary-search baseline and a graph-first arm in separate ephemeral Codex sessions; arm order alternates deterministically.
- A graph sample is invalid if the exact GitCortex command is not called once, the command errors, the model falls back after an error, usage is missing, or required answer evidence regresses.
- Total tokens include cached input; uncached tokens subtract reported cached input and add output.
- `15/20` graph samples completed with the graph call plus at most three verification commands. The other five remained valid but exceeded the aspirational follow-up budget.
- This is one round with targeted search reruns replacing the original search samples. Run three complete rounds before treating the aggregate as a release claim.
- Raw JSONL traces and Codex event logs are local-only under `tools/agent-bench/results/`; reports can be regenerated without model calls via `agent_report.py`.
