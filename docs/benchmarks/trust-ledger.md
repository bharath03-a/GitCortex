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
| uncommitted_edit_visible_after_debounce (B3-001) | FAIL | cea0c7e (B6 overlay checkpoint) | PASS |
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
(reproduced after the B3 rewrite with `--ignored`). Resolved in B6 (`cea0c7e` /
`ddbce46`) via a persisted atomic overlay checkpoint `{head, branch_key, files: path -> blob hash | null}`
that readiness revalidates against the working tree across requests and restarts
while keeping `.dirty` set and respecting pending watcher events.

During B3 targeted runs the real-root count rose 9371 → 9372 from another
worktree's hook (the new directory contains
`stack__f_ui__6_type_foundation.sha`). Trust daemon children use explicit
isolated store paths. Full-gate before/after counts are recorded separately.

B3 full `just ci`: PASS (one documented ignored scenario).
`cargo nextest run -p gitcortex-mcp -j 4`: 205 run, 205 passed, 1 skipped
(B3-001). The only library change in B1–B3 is the `#[doc(hidden)]` test-only
accessor `KuzuGraphStore::list_reference_facts`; no behavior changed.

## B4: tree indexing from git objects without checkout

| scenario | first result | fix commit | final |
|---|---|---|---|
| tree_source_lists_same_paths_as_disk | PASS | — | PASS |
| tree_source_honors_ignore_in_tree | PASS | — | PASS |
| tree_index_skips_binary_and_oversized | PASS | — | PASS |
| tree_index_matches_checkout_index | PASS | — | PASS |
| tree_index_incremental_from_sha | PASS | — | PASS |
| tree_index_handles_unicode_branch | PASS | — | PASS |
| inspect_commit_ready_in_bare_repo | PASS | — | PASS |
| inspect_commit_ready_with_unborn_checkout | PASS | — | PASS |
| inspect_commit_wrong_sha_is_stale | PASS | — | PASS |
| inspect_commit_dirty_marker_is_unverified | PASS | — | PASS |
| inspect_commit_missing_key_is_missing | PASS | — | PASS |
| inspect_checkout_semantics_unchanged | PASS | — | PASS |
| failed_tree_apply_stays_unverified | PASS | — | PASS |
| tree_apply_publishes_sha_only_after_success | PASS | — | PASS |
| failed_first_apply_is_unverified_without_sha | PASS | — | PASS |
| dirty_retry_rebuilds_complete_tree | PASS | — | PASS |
| requested_unindexed_branch_starts_tree_index | PASS | — | PASS |
| unknown_branch_reports_missing_without_indexing | PASS | — | PASS |
| duplicate_requests_spawn_one_job | PASS | — | PASS |
| failed_tree_index_reports_error_and_allows_retry | PASS | — | PASS |
| tree_index_concurrent_with_watcher | PASS | — | PASS |
| index_branch_preserves_checkout_and_ignores_uncommitted_bytes | PASS | — | PASS |

B4 full `just ci`: PASS (real-root count 9470 → 9470).
`cargo nextest run -p gitcortex-mcp -j 4`: 236 run, 236 passed, 1 skipped (B3-001).
Hook timing (`GCX_TIMING=1 gcx hook`): 322ms first run, 259ms incremental (under 500ms budget).

### B4 review fixes

| Test | Result |
| --- | --- |
| finish_aborts_when_key_became_checked_out, finish_aborts_when_baseline_moved_even_after_leaving_checkout, begin_aborts_for_checked_out_key_without_touching_markers, queued_job_cannot_overwrite_a_key_that_became_checked_out | PASS |
| non_branch_input_reports_missing_without_work, resolve_local_branch_rejects_revision_syntax, revision_syntax_and_unknown_branches_are_refused_clearly | PASS |
| legacy_blob_modes_index_as_regular_files, unusual_tree_entries_survive_the_full_apply_path | PASS |
| dirty_retry_drops_folders_of_vanished_directories, tree_index_incremental_drops_folders_of_vanished_directories | PASS |
| indexing_branch_gates_queries, indexing_gate_blocks_queries_while_a_job_is_held (mutation-checked) | PASS |
| duplicate_requests_spawn_one_job (now 5 real MCP calls in both tool modes) | PASS |
| idle_daemon_finishes_an_active_tree_job_before_exiting, idle_drain_is_capped_when_a_job_never_finishes | PASS |
| dotted_directory_sibling_survives_incremental (B4-001) | PASS |
| after_modify_in_dotted_directory (hook path) | PASS |

B4-001 (fixed): an incremental update under a directory whose name has an
extension (`v1.2/`) deleted that folder node, because `apply_diff` inferred
directories from extensions, and the rebuilt folder lost its edges to unchanged
siblings. `removed_files` now carries files only, and removal never matches a
`folder` node. Reused folders also no longer duplicate their edges to each other.

Source symlinks are a documented split, not parity: checkout indexing refuses them
(`indexing_rejects_source_symlink_outside_repository`), tree indexing skips them
(`source_symlink_is_skipped_in_tree_but_refused_on_disk`).

`cargo nextest run -p gitcortex-mcp -j 4`: 259 run, 259 passed, 1 skipped (B3-001).

## B6: working-tree overlay checkpoint and readiness certification

| scenario | first result | fix commit | final |
|---|---|---|---|
| uncommitted_edit_visible_after_debounce (B3-001 un-ignored) | FAIL (pre-B6) | cea0c7e | PASS |
| edit_then_settle_reports_ready_and_verified | PASS | cea0c7e | PASS |
| further_edit_after_checkpoint_returns_unverified_until_settled | PASS | cea0c7e | PASS |
| checkpoint_for_other_head_is_not_trusted | PASS | cea0c7e | PASS |
| changed_file_set_mismatch_is_not_certified | PASS | cea0c7e | PASS |
| deleted_file_overlay_certifies_after_settle | PASS | cea0c7e | PASS |
| restart_with_valid_checkpoint_recertifies_without_reindex | PASS | cea0c7e | PASS |
| restart_with_tampered_or_corrupt_checkpoint_stays_unverified | PASS | cea0c7e | PASS |
| oversized_or_many_changed_files_stay_unverified | PASS | cea0c7e | PASS |

B6 full `just ci`: PASS (real-root count 9470 → 9470; 0 skipped).
`cargo nextest run -p gitcortex-mcp -j 4`: 253 run, 253 passed, 0 skipped.


