# Token-Savings Benchmark — GitCortex v0.3

**Date:** 2026-05-30
**Binary:** `target/release/gcx` on branch `fix/multi-lang-query-bugs`
**Harness:** [`bench-harness.sh`](bench-harness.sh) (5 mechanical queries) + [`dev-harness.sh`](dev-harness.sh) (7 developer-style questions)

---

## What "geomean" means

Geometric mean. For a list of N positive values, geomean = `(v₁ × v₂ × … × vₙ)^(1/N)` = `exp(mean(ln(vᵢ)))`.

For ratios, geomean is the right average because:
- Arithmetic mean of `[1×, 1×, 10000×]` = **3334×**, dominated by the outlier.
- Geometric mean of `[1×, 1×, 10000×]` = **22×**, the "typical" ratio.

Whenever you see `geomean = 200×` in this report, read it as "**on a typical question, GitCortex returns 200× fewer tokens than reading raw files**" — half the questions do better, half worse.

When you see absolute token counts (`baseline_tokens`, `gcx_tokens`, `saved_tokens`), they're **summed across all questions for that repo**. That's the budget an LLM would actually burn in a one-session sweep of those questions.

Token proxy = `chars / 4` (rough tiktoken approximation; under-states for English doc comments, over-states for high-symbol-density code; qualitative order unchanged).

---

## Developer-style benchmark (7 realistic questions, 5 repos)

These are the 7 questions a developer actually asks an AI editor when they open an unfamiliar repo. Each baseline is the file-set an LLM would have to read manually (`grep -l` + `cat`).

| # | Developer question                        | GitCortex command                |
|---|-------------------------------------------|----------------------------------|
| 1 | "Give me a tour of this codebase"          | `gcx query tour --limit 10`      |
| 2 | "Find code related to `<concept>`"         | `gcx query search <term>`        |
| 3 | "Explain symbol `X`"                       | `gcx query wiki X`               |
| 4 | "If I change `Y`, what breaks? (3 hops)"   | `gcx query find-callers Y --depth 3` |
| 5 | "How does `Y` reach `Z`?"                  | `gcx query trace-path Y Z`       |
| 6 | "Show 2-hop neighborhood around `X`"       | `gcx query get-subgraph X --depth 2` |
| 7 | "What dead code exists?"                   | `gcx query find-unused --limit 30` |

Symbols are picked from the **centrality-ranked tour** (most-connected real graph nodes — what a developer would actually look at first), not random PascalCase matches.

### Headline — absolute tokens per session

| Repo (lang)     | Total baseline tokens | Total gcx tokens | Saved tokens | Saved % | Geomean |
|-----------------|----------------------:|-----------------:|-------------:|--------:|--------:|
| ripgrep (Rust)  |             1,248,662 |           20,713 |    1,227,949 | 98.34 % |    284× |
| hono (TS)       |               641,744 |            5,917 |      635,827 | 99.08 % |    224× |
| cobra (Go)      |               567,203 |           12,652 |      554,551 | 97.77 % |    208× |
| requests (Py)   |               543,662 |           10,416 |      533,246 | 98.08 % |    182× |
| gson (Java)     |               768,472 |           64,460 |      704,012 | 91.61 % |    119× |
| **TOTAL**       |         **3,769,743** |      **114,158** | **3,655,585** | **96.97 %** | **199× (geomean of geomeans)** |

**Plain reading:** across these 5 repos and 7 questions per repo, a developer who asked an LLM all 35 questions while reading raw files would burn **~3.77 M tokens**. Using GitCortex they'd burn **~114 K tokens** — **3.66 M tokens saved, 96.97 % less context spent**.

### Per-question detail

#### ripgrep (Rust) — symbols: `as_bytes` (type), `line_number` (fn), `search_reader` (other), concept `parse`

| # | Question                                                  | Baseline tokens | gcx tokens | Ratio    |
|---|-----------------------------------------------------------|----------------:|-----------:|---------:|
| 1 | Tour                                                      |         203,300 |        234 |     869× |
| 2 | Find code related to "parse"                              |          98,215 |        319 |     308× |
| 3 | Explain `as_bytes`                                        |          76,713 |      2,987 |      26× |
| 4 | If I change `line_number`, what breaks? (3 hops)          |         175,142 |      4,251 |      41× |
| 5 | How does `line_number` reach `search_reader`?             |         177,387 |         29 |   6,117× |
| 6 | 2-hop neighborhood around `as_bytes`                      |          76,713 |     12,862 |       6× |
| 7 | Find dead code                                            |         441,192 |         31 |  14,232× |

#### hono (TypeScript) — symbols: `toString`, `match`, `header`, concept `auth`

| # | Question                                                  | Baseline tokens | gcx tokens | Ratio    |
|---|-----------------------------------------------------------|----------------:|-----------:|---------:|
| 1 | Tour                                                      |         155,303 |        227 |     684× |
| 2 | Find code related to "auth"                               |          33,645 |        308 |     109× |
| 3 | Explain `toString`                                        |          26,418 |        619 |      43× |
| 4 | If I change `match`, what breaks? (3 hops)                |          46,742 |      1,153 |      40× |
| 5 | How does `match` reach `header`?                          |          44,808 |         15 |   2,987× |
| 6 | 2-hop neighborhood around `toString`                      |          26,418 |      3,564 |       7× |
| 7 | Find dead code                                            |         308,410 |         31 |   9,949× |

#### cobra (Go) — symbols: `executeCommand`, `AddCommand`, `String`, concept `parse`

| # | Question                                                  | Baseline tokens | gcx tokens | Ratio    |
|---|-----------------------------------------------------------|----------------:|-----------:|---------:|
| 1 | Tour                                                      |          98,404 |        207 |     475× |
| 2 | Find code related to "parse"                              |          76,713 |        180 |     426× |
| 3 | Explain `executeCommand`                                  |          56,850 |      3,537 |      16× |
| 4 | If I change `AddCommand`, what breaks? (3 hops)           |          90,200 |      2,451 |      37× |
| 5 | How does `AddCommand` reach `String`?                     |          60,049 |         16 |   3,753× |
| 6 | 2-hop neighborhood around `executeCommand`                |          56,850 |      6,230 |       9× |
| 7 | Find dead code                                            |         128,137 |         31 |   4,134× |

#### requests (Python) — symbols: `httpbin`, `get`, `Session`, concept `auth`

| # | Question                                                  | Baseline tokens | gcx tokens | Ratio    |
|---|-----------------------------------------------------------|----------------:|-----------:|---------:|
| 1 | Tour                                                      |          85,191 |        214 |     398× |
| 2 | Find code related to "auth"                               |          67,734 |        299 |     227× |
| 3 | Explain `httpbin`                                         |          61,364 |      2,628 |      23× |
| 4 | If I change `get`, what breaks? (3 hops)                  |          83,084 |      4,013 |      21× |
| 5 | How does `get` reach `Session`?                           |          83,084 |         35 |   2,374× |
| 6 | 2-hop neighborhood around `httpbin`                       |          61,364 |      3,196 |      19× |
| 7 | Find dead code                                            |         101,841 |         31 |   3,285× |

#### gson (Java) — symbols: `JsonReader`, `beginArray`, `getType`, concept `auth`

| # | Question                                                  | Baseline tokens | gcx tokens | Ratio    |
|---|-----------------------------------------------------------|----------------:|-----------:|---------:|
| 1 | Tour                                                      |         114,754 |        316 |     363× |
| 2 | Find code related to "auth"                               |          17,784 |          9 |   1,976× |
| 3 | Explain `JsonReader`                                      |          44,288 |      7,559 |       6× |
| 4 | If I change `beginArray`, what breaks? (3 hops)           |          80,215 |      6,651 |      12× |
| 5 | How does `beginArray` reach `getType`?                    |          62,182 |        110 |     565× |
| 6 | 2-hop neighborhood around `JsonReader`                    |          44,288 |     49,784 |       0.9× |
| 7 | Find dead code                                            |         404,961 |         31 |  13,063× |

---

## Reading the per-question table

- **Tour, trace-path, find-dead-code** are the big wins (300×–14000×). The answer fits in a paragraph; the baseline is whole-codebase or whole-file reads.
- **Wiki and refactor-impact** are smaller wins (15×–45×). GitCortex returns a structured 1-3 KB response; the baseline is "read the defining file" which is only one-or-two files of ~50 KB each.
- **2-hop subgraph** is the worst case — sometimes near-zero savings (gson 0.9×). When a node has hundreds of neighbors, the subgraph dump rivals a small file in size. **This is where `--depth 1` or `find-callers`/`find-callees` is a better tool than `get-subgraph --depth 2`.** Worth documenting in the agent guide.

---

## Original 5-question mechanical benchmark (full 15-repo sweep)

For completeness, the earlier mechanical benchmark over **15 repos × 5 mechanical queries** is preserved below. It auto-picks symbols by PascalCase frequency (less realistic) but covers more repos.

| Language   | Avg of geomeans (3 repos) | Repos                       |
|------------|---------------------------|-----------------------------|
| Rust       | **1,966 ×**               | ripgrep, tokio, serde       |
| Go         | **1,928 ×**               | gin, cobra, zap             |
| TypeScript | **1,147 ×**               | zod, hono, io-ts            |
| Python     |   **661 ×**               | requests, flask, fastapi    |
| Java       |   **748 ×**               | gson, picocli, jjwt         |

Numbers there are higher because the auto-picked symbols (e.g., `Error`, `Result`) appear in many files, inflating the baseline term. The developer-style numbers above (~200× geomean) are a more honest representation of what an actual session saves.

Raw per-repo JSON files live in this folder: `dev-<repo>.json` for the developer-style runs, `<lang>-<repo>.json` for the mechanical sweep.

---

## Caveats

1. **Token proxy.** `chars / 4` instead of a real tokenizer. ±30 % error on absolute counts, no effect on ratio ordering.
2. **One run per repo.** No statistical noise estimate — but ratios this large dwarf any per-run variance.
3. **No LLM in the loop.** This measures retrieval-token savings only. End-to-end task quality (correct answers, fewer reasoning hops) is not measured.
4. **Symbols picked by centrality.** Real-world questions might target specific business symbols, but the tour-ranked pick is the closest objective stand-in.
5. **Indexer cost not included.** First index takes 0.3–4 s per repo; incremental hooks are sub-500 ms on changed files. One-time cost.

---

## Reproducing

```bash
cargo build --release --bin gcx

# Developer-style benchmark on a single repo
bash docs/benchmarks/dev-harness.sh \
    https://github.com/BurntSushi/ripgrep \
    /tmp/out.json

# Full 5-repo dev sweep (one per language, ~5 min total)
for url in \
  https://github.com/BurntSushi/ripgrep \
  https://github.com/psf/requests \
  https://github.com/honojs/hono \
  https://github.com/spf13/cobra \
  https://github.com/google/gson
do
  name=$(basename "$url")
  bash docs/benchmarks/dev-harness.sh "$url" "docs/benchmarks/dev-$name.json"
done
```
