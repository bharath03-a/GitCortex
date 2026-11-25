# GitCortex v0.8.0 Benchmark Evidence Archive

This directory preserves the machine-readable benchmark artifacts and SHA-256 provenance backing the v0.8.0 release documentation, `README.md`, and the six self-hosted charts in `docs/assets/charts/`.

See [`provenance.json`](provenance.json) for file checksums and suite metadata.

## Summary of Measured Lanes

### 1. Deterministic Code Retrieval & Industry IR Ranking (45 tasks, 10 repos)
- **Artifacts:** [`det-35.jsonl`](det-35.jsonl) (5 core repos: `ripgrep`, `requests`, `hono`, `cobra`, `gson`) and [`det-big-10.jsonl`](det-big-10.jsonl) (5 large repos: `tokio`, `django`, `next.js`, `moby`, `spring-boot`).
- **Results:** **45/45 PASS (100%)**, **Hit@1 = 100%**, **MRR = 1.000**, **NDCG@5 = 0.925**, **NDCG@10 = 0.971**, **MAP@5 = 0.897**, **Precision@5 = 0.640** (naturally capped because tasks average ~3.2 ground-truth files across 5 slots), **File Recall = 1.000**.

### 2. Paired Coding-Agent Benchmark — Codex CLI (35 tasks, 5 core repos)
- **Artifact:** [`agent-codex-35.jsonl`](agent-codex-35.jsonl) (`gpt-6.1-sol`, low reasoning; baseline shell/grep vs. shell/grep + GitCortex MCP).
- **Results:** **35/35 PASS**, 0 invalid, 0 quality regressions.
  - Evidence quality mean: **0.943 → 0.986**
  - Total tokens: **3,877,403 → 3,124,734 (-19.4%, 1.22× geomean)**
  - Uncached input tokens: **662,427 → 478,846 (-27.7%, 1.38× geomean)**

### 3. Operational Scale: Cold Indexing, Incremental Hooks & Storage GC (10 repos)
- **Artifact:** [`ops.json`](ops.json)
- **Cold full-index speedup:**
  - `ripgrep` (123 files, 3.8k nodes, 19.7k edges): **22.07s → 1.68s (13.1× faster)**
  - `django` (3,049 files, 48.5k nodes, 267.5k edges): **>600s timeout → 5.06s (>118× faster)**
  - `hono` **0.44s**, `tokio` **2.85s**, `spring-boot` **9.16s**, `next.js` **10.72s**, `moby` **18.19s** (4,606 files including ~1,500 vendored Go packages under `vendor/`).
- **Day-2 incremental update (`gcx hook` on single-file edit):** median **821 ms** across 10 repos (sub-second on 6/10 repos; max `2.57s` on `moby`).
- **Multi-branch storage reclaim (`gcx storage gc --apply` + `compact --apply` after 5 deleted feature branches):** **83.4–84.6%** disk reclaimed across every repo (e.g., `ripgrep 85.5 → 14.2 MiB`, `django 676.2 → 104.1 MiB`, `moby 852.6 → 139.2 MiB`).

### 4. Head-to-Head Competitor Benchmarks (Codex CLI `gpt-6.1-sol`, low)
- **vs. Graphify `0.9.44` (15 tasks, 5 core repos — [`vs-graphify-15.json`](vs-graphify-15.json)):**
  - Baseline (grep): **0.967** quality (14/15), 367k uncached tokens
  - GitCortex v0.8.0: **1.000** quality (15/15), 246k uncached tokens (**-32.9%**)
  - Graphify `0.9.44`: **0.533** quality (6/15), 5 symbol-resolution errors (`Searcher`, `dispatch`, `peek`, `beginArray`) and 0.00 on 3 impact tasks. *Fairness note:* on the 6 tasks where Graphify scored 1.00, Graphify used fewer uncached tokens than GitCortex (`77k` vs. `91k`, vs. `154k` baseline).
- **vs. CodeGraph `1.5.0` (10 tasks, 5 large repos — [`vs-codegraph-10.json`](vs-codegraph-10.json)):**
  - Baseline (grep): **0.900** quality (8/10), 178k uncached tokens
  - GitCortex v0.8.0: **0.950** quality (9/10), 129k uncached tokens (**-27.7%**)
  - CodeGraph `1.5.0`: **0.750** quality (6/10), 197k uncached tokens (**+10.5%** vs. baseline). *Fairness note:* CodeGraph indexed `moby` faster than GitCortex (`8.32s` vs. `18.19s`).

### 5. Unforced Adoption & Restraint (48 runs) + Continuous Episodes
- **Artifact:** [`master.json`](master.json) (`unforced_48`, `continuous_episodes`).
- **48-run unforced study (16 tasks = 10 graph + 6 restraint × 3 arms):**
  - Overall pass rate: Baseline **14/16 (88%)** → `gcx-bare` **15/16 (94%)** → `gcx-guide` **16/16 (100%)**
  - Voluntary graph adoption (10 graph tasks): `gcx-bare` **4/10 (40%)** → `gcx-guide` **10/10 (100%)**
  - Restraint accuracy (6 non-graph tasks): **6/6 (0 false calls)** across all arms.
  - *Honest token caveat:* without explicit per-call `--budget-tokens` caps in the unforced harness, the guided agent made 55 thorough CLI calls and spent `321k` vs. `196k` uncached tokens. Pair the shared Agent Guide with budgeted queries in cost-sensitive workflows.
