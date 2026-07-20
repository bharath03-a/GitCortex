# Claude Code MCP Gate v1

Pinned single-round validation through Claude Code's real MCP path (`claude-code-mcp`), using a strict per-session config that exposes only GitCortex's compact `gcx` dispatch tool plus focused reads. Claude Code 2.1.215 resolved the `haiku` alias to `claude-haiku-4-5-20251001` in the captured native events.

The first smoke run exposed a genuine interoperability defect: the dispatch `params` schema was unconstrained, causing Claude to serialize the object as a JSON string. The release candidate now declares it as an object and has a schema regression test. No CLI fallback is included in these results.

Model: `haiku`  
Samples: 20 (20 valid)  
Total-token geomean: **2.28×**  
Uncached-token geomean: **1.26×**  
Quality non-inferior: **20/20**  
≤3 verification commands: **18/20**

## By repository

| Repo | Valid | Total | Uncached | Command budget |
|---|---:|---:|---:|---:|
| cobra | 4/4 | 2.14× | 1.16× | 2/4 |
| gson | 4/4 | 1.95× | 1.48× | 4/4 |
| hono | 4/4 | 1.64× | 0.90× | 4/4 |
| requests | 4/4 | 2.46× | 1.37× | 4/4 |
| ripgrep | 4/4 | 3.65× | 1.53× | 4/4 |

## By workflow

| Workflow | Valid | Total | Uncached | Command budget |
|---|---:|---:|---:|---:|
| callers | 5/5 | 2.31× | 1.41× | 5/5 |
| search | 5/5 | 1.12× | 0.67× | 5/5 |
| subgraph | 5/5 | 3.36× | 1.90× | 4/5 |
| tour | 5/5 | 3.10× | 1.42× | 4/5 |

## Methodology and limitations

- Repositories and tasks are pinned in `tools/agent-bench/suite.toml`.
- Baseline and graph arms run in separate ephemeral native-client sessions with deterministic alternating order.
- Graph validity requires exactly one successful GitCortex command and non-inferior required source evidence.
- Total and uncached usage are both reported because client cache behavior materially affects totals.
- The command budget means one graph call plus at most three verification commands.
- Claude's deferred `ToolSearch` schema-discovery event is fixed client overhead and is not counted as a source verification command; its tokens remain included.
- Total tokens include cache reads; uncached tokens exclude cache reads. Alternating arm order matters because Claude's cross-session prompt cache can make the first arm pay cache creation and the second arm mostly read it.
- Search is still the weakest workflow: 1.12× total but 0.67× uncached in this round. The overall gate passes, but search should remain a tracked optimization target.
- This combines one complete round with one targeted Hono search rerun replacing an invalid sample where the model violated the exactly-once MCP rule.
- Run three complete rounds before treating this single-round aggregate as a stable release claim.
- Raw streamed events and JSONL traces are local-only under `tools/agent-bench/results/`; `agent_report.py` replays them without model calls.
