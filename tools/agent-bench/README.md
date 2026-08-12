# GitCortex Agent Bench

A pinned, replayable benchmark around the real `gcx` binary. The first lane is model-free so retrieval correctness and response contracts can be fixed before spending provider credits.

## Run

```bash
cargo build --release --bin gcx
python3 tools/agent-bench/bench.py run \
  --gcx target/release/gcx \
  --label head
```

Run a cheap subset or reuse an already-built index:

```bash
python3 tools/agent-bench/bench.py run --gcx target/release/gcx --label smoke \
  --repo cobra --task cobra-callers-add-command
python3 tools/agent-bench/bench.py run --gcx target/release/gcx --label smoke-2 \
  --repo cobra --reuse-index
```

Replay and compare without running tools:

```bash
python3 tools/agent-bench/bench.py replay tools/agent-bench/results/head.jsonl
python3 tools/agent-bench/bench.py compare base.jsonl head.jsonl
```

## Search relevance

Payload bytes and evidence presence cannot tell a correct top hit from a correct
tenth hit, so ranking changes are invisible to the byte-oriented gate. Search
tasks additionally pin ranked ground truth:

```toml
relevant_files = ["src/requests/sessions.py"]
relevant_symbols = ["Session", "SessionRedirectMixin"]
```

Ground truth is read from the pinned repository source, never from `gcx` output.
Each search task then reports:

- **MRR** — reciprocal rank of the first relevant hit;
- **precision@5** — relevant share of the top five, divided by the fixed cut-off
  so a short result cannot buy precision by withholding candidates;
- **file recall** — pinned files found anywhere in the ranked list.

These are reported, not gated: a task stays valid on contract and evidence
grounds. `compare` reports `relevance_non_inferior` so a ranking regression fails
a base/head comparison even when the payload shrank.

Run the tests for the scorer and harness wiring:

```bash
cd tools/agent-bench && python3 -m unittest discover -p "test_*.py"
```

## Validity rules

A task fails when:

- the command exits non-zero;
- agent JSON is malformed or has a non-`ok` status;
- required source evidence is missing;
- forbidden evidence appears;
- the payload exceeds its task budget.

Every JSONL trace records the suite hash, binary hash/version, exact repo commits, command, stdout/stderr, latency, payload size, contract status, and evidence checks.

## Codex agent-loop lane

The Codex lane alternates baseline/graph arm order, enforces exactly one successful
GitCortex command with no fallback, captures cached and uncached usage, and scores
required source evidence in the final answer:

```bash
python3 tools/agent-bench/agent_run.py \
  --gcx target/release/gcx \
  --model gpt-5.4-mini --reasoning low \
  --label codex-smoke --repo cobra --rounds 1
```

This lane is explicitly reported as `codex-graph-cli`, not MCP. Current
ChatGPT-account `codex exec` sessions list configured ad-hoc MCP servers but do
not expose their tools to the model. A missing MCP capability is never silently
reported as an MCP result.

## Claude Code MCP lane

Claude Code runs the same pinned tasks through a strict per-session MCP config.
The graph arm exposes `Read` and the compact `mcp__gcx` single-dispatch tool;
the baseline exposes normal read/search tools and an empty MCP config:

```bash
python3 tools/agent-bench/agent_run.py --client claude \
  --gcx target/release/gcx --model haiku --reasoning low \
  --label claude-smoke --repo cobra --rounds 1
```

Streamed native-client events are used to enforce exactly one successful MCP
call, count follow-up tools, capture client-reported cache usage, and score the
same required final-answer evidence. Claude total tokens include cache reads;
uncached tokens exclude cache reads.

## Agy MCP lane

Agy has no per-invocation MCP config flag — it only reads its global
`~/.gemini/config/mcp_config.json`. The harness swaps that file around each
arm's run and restores whatever was there before, so a benchmark run never
leaves the user's own `agy` setup altered:

```bash
python3 tools/agent-bench/agent_run.py --client agy \
  --gcx target/release/gcx --model "Gemini 3.6 Flash (Low)" --reasoning low \
  --label agy-smoke --repo cobra --rounds 1
```

This lane is reported as `agy-mcp` and scores the same required
final-answer evidence as the Codex and Claude Code lanes.

## Lanes

1. **Retrieval** (implemented): deterministic, free contract/evidence gate.
2. **Codex graph CLI** (implemented): native autonomous baseline vs graph-first loop.
3. **Controlled answer**: fixed baseline or GitCortex context, one provider call.
4. **Provider tool loop**: equivalent single dispatch schema through OpenAI/Anthropic APIs.
5. **Native MCP clients**: Claude Code MCP, Codex MCP, and Agy MCP when local MCP is actually exposed.

Provider lanes must consume the same pinned manifest and emit provenance-rich JSONL. A missing client capability is `unsupported`, never a silent CLI fallback.
