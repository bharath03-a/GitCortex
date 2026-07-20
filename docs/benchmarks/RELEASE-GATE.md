# Historical Claude Token Benchmark

> **Status:** retained for historical Claude API comparisons. The pinned, validity-gated agent-first harness is now documented in [`tools/agent-bench/README.md`](../../tools/agent-bench/README.md). Current deterministic results are in [`agent-retrieval-v1.md`](agent-retrieval-v1.md), and the first native Codex result is in [`codex-agent-gate-v1.md`](codex-agent-gate-v1.md). Do not compare its dynamic tasks directly with the pinned v1 suite.

A measured-usage benchmark originally run for releases to catch MCP-tool
regressions. Unlike the chars/4 proxy in [`token-savings-v0.3.md`](token-savings-v0.3.md),
every number comes from the token `usage` the Claude API actually reports.

## Why it exists

The proxy benchmark over-states savings by 100–1000× because it assumes the
baseline reads *whole files*. Real Claude greps and reads only snippets, so its
baseline is far cheaper than the proxy imagines. Worse, the proxy's biggest
"wins" (whole-repo questions like dead-code) are actually **losses** in reality —
real Claude never reads the whole repo, it answers approximately and cheaply,
while a graph tool returns a large exact result.

So the only number we trust for shipping decisions is **measured usage**.

## What it measures

For each language (one canonical repo each) and each of 7 developer questions,
Claude answers the question **twice**:

| Arm | Tools allowed | Represents |
|-----|---------------|------------|
| baseline | `Read Grep Glob Bash(grep/find/cat/…)` | how Claude works today |
| gcx | `Read` + the GitCortex MCP tools (no grep) | the graph-first path |

Both arms get the identical question and the identical Claude system prompt, so
the ~14k-token fixed overhead cancels in the ratio. We record real
`usage.{input,cache_creation,output}_tokens`, `total_cost_usd`, and `num_turns`.

`tokens = input + cache_creation + output`. `cache_read` is **excluded** — it is
cheap re-reads of context already counted, and summing it across turns
double-counts. (It still shows up in `total_cost_usd`, where gcx often wins by
running fewer turns.)

Each question maps 1:1 to one MCP tool, so the output is a **tool × language
scorecard** — the thing we act on.

| Question | MCP tool |
|----------|----------|
| tour | `start_tour` |
| search concept | `search_code` |
| explain symbol | `wiki_symbol` |
| refactor impact | `find_callers` |
| trace flow | `trace_path` |
| 2-hop neighborhood | `get_subgraph` |
| dead code | `find_unused_symbols` |

## Running it

```bash
cargo build --release --bin gcx

# Full sweep, one repo per language, renders the scorecard at the end.
bash docs/benchmarks/real-sweep.sh                       # haiku, 7 questions
bash docs/benchmarks/real-sweep.sh claude-sonnet-4-6 7   # production fidelity
PARALLEL=4 bash docs/benchmarks/real-sweep.sh            # faster, more API contention

# Single repo
bash docs/benchmarks/real-harness.sh \
  https://github.com/django/django docs/benchmarks/real-django.json \
  claude-haiku-4-5-20251001 7

# Re-render the HTML scorecard from existing real-*.json
python3 docs/benchmarks/real-report.py
```

Cost: ~$1–1.5 per repo on haiku (×5 ≈ $5–8). Each session is capped by
`BUDGET` (default `$0.75`). Token *volume* is roughly model-independent, so
haiku is a fair cheap proxy for the ratio; use sonnet/opus for a production run.

Outputs: `real-<repo>.json` per repo, `real-report.html` scorecard.

## The release loop

```
1. Build gcx for the release candidate.
2. Run the sweep → real-report.html.            verify: scorecard renders, 5 langs
3. Diff against the previous release's scorecard.
   - A tool green→red in most languages = REGRESSION. Block release.
4. Fix the offending tool (usually: cap output size, lower default depth,
   return a summary instead of a full dump).
5. Re-run the single repo for that tool's worst language.   verify: ratio recovered
6. Re-run the full sweep before tagging.        verify: no new red cells
```

## Reading the verdict column

- **keep — net win** — geomean ≥ 1.15× across languages. Ship as-is.
- **marginal** — between 0.9× and 1.15×. Watch; small wins, fixed-overhead bound.
- **REDESIGN — loses to grep** — loses in most languages. The tool returns more
  tokens than grep would cost. Fix before leaning on it in agent guidance.

## Known structural costs

- **Fixed MCP tax.** 15 tool schemas ride in every turn of the gcx arm. On small
  repos and high-fan-out tools this overhead can exceed the savings. Shrinking
  tool-schema size (terser descriptions, or a single dispatch tool) lifts every
  cell at once.
- **High-fan-out dumps lose.** `get_subgraph --depth 2` and `find_callers` on hub
  symbols serialise huge result sets. Cap count, default to depth 1, summarise.
- **Whole-repo questions reverse.** `find_unused_symbols` looks like a giant win
  in the proxy and a loss in reality. Return a short ranked head, not the full list.
