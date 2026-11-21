# Initial 0.8.0 benchmark baseline — 2026-11-20

The published v0.7.6 macOS ARM64 archive was verified against its release checksum. The existing model-free harness ran the 20 tasks supported by its legacy adapter on five pinned repositories. Raw output, failures, binary hash, suite hash, repository commits, and timings are retained in `v076-compatible20.jsonl`; `provenance.json` records limitations.

18 tasks passed and two caller queries failed their required-source checks. These failures are retained. The initial Cobra smoke also passed. The attempted full 35-task run stopped at the legacy adapter's unsupported `impact` action and produced no complete trace; the compatible run is explicitly labeled as a 20-task subset.

This is an initial baseline, not release proof. Ranking metrics from the legacy text adapter are unavailable despite the old harness printing zeros. No performance or usefulness improvement can be claimed until matching candidate runs, repeated measurements, and the remaining release lanes finish. Do not publish these timings as final benchmarks.

Reproduce using the published binary and `bench.py run --adapter legacy-retrieval`, selecting every task whose action is `search`, `tour`, `callers`, or `subgraph`; use a fresh `--work` path and `GCX_STORE_PATH`. The original suite is unchanged.

## Development run

`development35.jsonl` records a fresh release-mode build of source commit `77093a342153de0863ac181f3e7038ab7899f145` (the original release branch, before later feature stacks are integrated). It ran all 35 tasks on separate repository/index paths. 33 passed. Failures are retained: `ripgrep-impact-line-number` exceeded its payload budget, and `requests-architecture-session` missed required evidence. The matching first 20 tasks all passed.

This is a development diagnostic, not the frozen candidate. The raw timing and payload results do not demonstrate a speed or token win; there is only one round, the adapters differ, and OS caching/order was not controlled. Final benchmarks must use the completed candidate, alternating repeated runs, and the remaining performance, storage, competitor, and agent lanes.
