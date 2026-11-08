# Trust scenario ledger

Fingerprints compare persisted node metadata, spans, complete edge records and
per-branch reference facts (canonical source endpoint, source file, target name,
kind and line) against a full rebuild in a separate repository copy. Record
multiplicity is preserved; mismatch output lists individual records.

The CLI hook equivalence test runs the built `gcx hook` against the harness on
identical snapshots in isolated stores.

| scenario | first result | fix commit | final |
|---|---|---|---|
| after_create_file | PASS | — | PASS |
| after_modify_body | FAIL: fixture Kuzu virtual-address reservation exhausted under parallel opens | 8ed3baa | PASS |
| after_modify_signature | PASS | — | PASS |
| after_shift_lines | PASS | — | PASS |
| after_delete_file | PASS | — | PASS |
| after_rename_file | PASS | — | PASS |
| after_rename_symbol | PASS | — | PASS |
| after_commit_amend | PASS | — | PASS |
| after_reset_hard_to_parent | PASS | — | PASS |
| after_fast_forward_merge | PASS | — | PASS |
| after_rebase_onto_main | PASS | — | PASS |
| after_cross_file_caller_removed | PASS | — | PASS |
| after_duplicate_name_added | PASS | — | PASS |
| after_callee_added_later | PASS | — | PASS |

B1: the missing-support compile failure preceded implementation; all twelve
self-tests (including stale/missing reference facts, a byte-identical duplicate
node and an appended symbol) then passed. The after_modify_body failure was a
harness resource fix (fixture lock), not a graph fix; after the stack was
rebased the lock lives in the B1 harness commit listed above.
No B2 scenarios are ignored.

Isolation evidence: B1 full `just ci` real-root count was 9370 → 9370.
The initial session count was 9291; it rose to 9370 before that full run.
The repository's post-commit hook subsequently created a real-root store
(9371); later outer Git commands explicitly pass the isolated store path.
Fixture Git commands already pass that path and disable hooks.

B2 full `just ci`: PASS; real-root count 9371 → 9371.


## B3: live daemon and initialized MCP clients

Each scenario runs in full and compact modes. The daemon runs in a child
process with an explicit isolated store path (a short symlink to the same
Cargo test-store directory), semantic downloads disabled, and a temporary cache.
The client validates advertised output schemas, or MCP's object-only
structured-content contract when no output schema is advertised.

| scenario | first result | fix commit | final |
|---|---|---|---|
| uncommitted_edit_visible_after_debounce (B3-001) | FAIL | — (architectural; deferred) | IGNORED |
| clean_checkout_reports_ready_and_verified (positive control) | PASS | — | PASS |
| deleted_file_symbols_disappear (asserts the symbol is visible first) | FAIL: test unwrapped items during transient degraded response | — (test polling corrected) | PASS |
| branch_omitted_answers_current_checkout | PASS | — | PASS |
| branch_switch_while_running_updates_index_branch | PASS | — | PASS |
| restart_with_dirty_overlay_not_ready_until_verified (disk restored clean, stale overlay + dirty flag; never ready while overlay visible; polls to reconciliation) | FAIL: test startup barrier required a clean commit for a dirty overlay | — (wait for watcher health; preserve readiness assertion) | PASS |
| detached_head_never_answers_main (detached at an older commit; main-only symbol absent once ready) | PASS | — | PASS |
| array_results_validate_against_output_schema | FAIL: test expected callers under items rather than evidence | — (test response-shape assertion corrected) | PASS |

B3-001 root cause: `readiness::inspect` hard-codes `unverified` whenever
`changed_sources` is non-empty for the checkout branch, and the watcher's
`apply_disk_batch` verifies batch snapshots only against HEAD, so an uncommitted
overlay can never be certified. The function is visible within the debounce
window, but the response stays `unverified` with `working_tree_verified=false`
(reproduced after the B3 rewrite with `--ignored`). The fix is a persisted
overlay checkpoint `{head, path -> blob hash}` that readiness revalidates
against the working tree across requests and restarts, rather than clearing the
dirty marker or equating watcher health with verified content. That changes
readiness semantics and shared watcher state, so the scenario stays an explicit
ignored regression under the plan's architectural-stop rule. No production fix
is claimed.

During B3 targeted runs the real-root count rose 9371 → 9372 from another
worktree's hook (the new directory contains
`stack__f_ui__6_type_foundation.sha`). Trust daemon children use explicit
isolated store paths. Full-gate before/after counts are recorded separately.

B3 full `just ci`: PASS (one documented ignored scenario).
`cargo nextest run -p gitcortex-mcp -j 4`: 205 run, 205 passed, 1 skipped
(B3-001). The only library change in B1–B3 is the `#[doc(hidden)]` test-only
accessor `KuzuGraphStore::list_reference_facts`; no behavior changed.
