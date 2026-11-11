# Continuous-coding episodes v1 (2026-11-11)

Implements the paired-episode lane from `docs/CONTINUOUS-CODING-PLAN.md` section 5 (that plan lives on `release/r1-combined-validation`, not yet on `main`). **No paid episode has been run yet.** What exists and is verified: two episodes whose hidden tests are proven to fail on the seeded start state and pass on the reference solution.

## Episodes

| Episode | Repo @ pinned commit | Language | Family | Steps |
|---|---|---|---|---|
| `requests-auth-redirect` | psf/requests `f361ead0…` | Python | cross-module debug | step1 (utils constant), step2 (sessions logic), probe (who references `DEFAULT_PORTS`) |
| `cobra-find-flags` | spf13/cobra `adbc8813…` | Go | cross-module debug | step1 (long flags), step2 (short flags), probe (callers of `stripFlags`) |

Each starts from the pinned commit plus `episodes/<id>/start.patch`, which seeds two independent bugs. Steps are delivered one by one inside **one persistent Claude Code session** (`--session-id`, then `--resume`; confirmed live to carry context). `edit` steps are scored by hidden behavioral tests; the `probe` step is graded against required evidence and scanned for a false "no dependents" claim.

Hidden tests (`episodes/<id>/hidden/`) are never placed in the agent's checkout: they are copied in only to score a step, then removed. The runner flags `hidden_access_suspected` when a transcript touches paths such as `hidden_step` or `episodes/`.

## Verification (proof the tests discriminate)

`python3 tools/agent-bench/continuous_verify.py` applies the reference patches in order and runs every edit step's hidden tests after each. Expected: steps up to the applied patch pass, later steps fail. Recorded run (2026-11-11, python 3.12.9, pytest 9.1.1, go1.26.5):

| State | requests step1 | requests step2 | cobra step1 | cobra step2 |
|---|---|---|---|---|
| start (seeded bugs) | fail | fail | fail | fail |
| + reference step1 | **pass** | fail | **pass** | fail |
| + reference step2 | pass | **pass** | pass | **pass** |

No stray files were left in either checkout. Record: `results/continuous-verify-20261111T174006Z.json`, sha256 `9f81be1d5a57d2fd9e8434da42204fbf28bbfff2c953a8133eaacf77f651f035` (local; `results/` is git-ignored). Episode digests: requests `e21bf733…1834`, cobra `87adbabb…8865`. `continuous_run.py` refuses to run unless the record is passing and each episode digest still matches, so an edited hidden test cannot silently reuse an old verification.

Caveats: at the requests start state only one step-2 test fails (the cross-host leak); the other step-2 tests are regression guards that pass on both the seeded and fixed code. The reference solutions are the exact reversals of the seeded bugs, so they prove the tests discriminate, not that a different valid fix passes (behavioral tests were written against behavior, not the diff).

## Run it

```bash
python3 tools/agent-bench/continuous_verify.py --work /tmp/gcx-agent-bench
python3 tools/agent-bench/continuous_run.py --gcx /path/to/gcx --label ep-smoke \
  --verification tools/agent-bench/results/continuous-verify-<ts>.json \
  --arms baseline,gcx-guide --rounds 1 --seed 1 \
  --max-total-usd 15 --max-run-usd 2     # per client call; an episode is 3 calls
python3 tools/agent-bench/continuous_run.py --gcx ... --label x --max-total-usd 15 --dry-run
```

## Metrics recorded per episode

Task success (all hidden edit-step tests pass on the final state, `steps_passed/steps_total`), `probe_correct`, `false_no_dependents`, session totals (cost, total/uncached tokens, turns, tool calls, wall time), gcx adoption and session-global first-call position, `repeat_reads` and `repeat_commands` across the whole session, and patch quality (`files`, `insertions`, `deletions`, `test_files`, `unrelated_files`, patch saved to the `-logs/` directory). `review` is `null`: the human or blinded review pass is not automated and is never filled in by the harness. Budget exhaustion mid-episode is recorded as `aborted`, counted as a failure, and never dropped.

## Not done in this layer

- No paid episode run, so no episode numbers exist.
- Fault episodes (stale or missing index, stopped watcher) and the branch/review/resume family from the plan are not implemented.
- Only the Claude Code client is supported.
