# Agent Retrieval Gate v1

Date: 2026-07-20  
Suite: `tools/agent-bench/suite.toml`  
Repos: 5 pinned commits, one each for Rust, Python, TypeScript, Go, and Java  
Tasks: 20 (search, tour, exact callers, exact neighborhood per repo)

## Purpose

This is a **model-free product contract gate**, not a token-usage claim. It compares `main`'s legacy CLI response surface with the agent-first response branch using the same pinned repos, target symbols, required source evidence, and payload limits.

A task is valid only when the command succeeds, required source evidence is present, forbidden evidence is absent, the payload fits its budget, and agent JSON reports `status=ok` with the configured minimum evidence count.

## Result

| Metric | `main` legacy | Agent-first branch |
|---|---:|---:|
| Valid tasks | 10 / 20 | **20 / 20** |
| Mean required-evidence score | 0.95 | **1.00** |
| Median payload | 3,040 bytes | **1,392 bytes** |
| Median payload improvement | — | **2.89×** |

### Median payload improvement by workflow

| Workflow | Improvement | Notes |
|---|---:|---|
| Search | 1.00× | Lexical CLI search is intentionally unchanged |
| Tour | **3.63×** | Production components; no duplicated community/raw arrays |
| Callers | **3.41×** | Exact identity, ranked evidence, global 600-token budget |
| Neighborhood | **5.64×** | Exact-ID relation digest, global 400-token budget |

No branch task lost required evidence. One task (`hono-callers-compose`) returns more bytes because `main` returned a 41-byte incorrect/empty result; the new response includes the actual caller and raises evidence quality from 0.5 to 1.0. Every neighborhood task is now smaller than `main` while preserving required evidence.

## Reproduce

```bash
# Head
cargo build --bin gcx
python3 tools/agent-bench/bench.py run \
  --gcx target/debug/gcx --label head

# Base binary built in a main worktree
python3 tools/agent-bench/bench.py run \
  --gcx /path/to/main/gcx --adapter legacy-retrieval --label main

python3 tools/agent-bench/bench.py compare \
  tools/agent-bench/results/main-*.jsonl \
  tools/agent-bench/results/head-*.jsonl
```

Raw JSONL traces are intentionally local-only and replayable. Each trace includes the suite hash, binary hash/version, exact repository commits, commands, stdout/stderr, latency, payload size, and evidence checks.

## What this proves—and does not prove

It proves the new CLI/MCP-oriented response contracts are smaller, stricter, and more source-grounded on the pinned retrieval suite. It also proves ambiguity handling and exact-ID traversal work across the supported language set.

It does **not** yet prove end-to-end model token savings or answer quality. Those require the controlled provider and native-client lanes after this free gate passes.
