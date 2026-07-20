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

## Validity rules

A task fails when:

- the command exits non-zero;
- agent JSON is malformed or has a non-`ok` status;
- required source evidence is missing;
- forbidden evidence appears;
- the payload exceeds its task budget.

Every JSONL trace records the suite hash, binary hash/version, exact repo commits, command, stdout/stderr, latency, payload size, contract status, and evidence checks.

## Lanes

1. **Retrieval** (implemented): deterministic, free contract/evidence gate.
2. **Controlled answer**: fixed baseline or GitCortex context, one provider call.
3. **Provider tool loop**: equivalent single dispatch schema through OpenAI/Anthropic APIs.
4. **Native clients**: Claude Code MCP and Codex when local MCP is actually exposed.

Provider lanes must consume the same pinned manifest and emit the same JSONL task schema. A missing client capability is `unsupported`, never a silent CLI fallback.
