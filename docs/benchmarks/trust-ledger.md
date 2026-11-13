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
| after_modify_body | FAIL: fixture Kuzu virtual-address reservation exhausted under parallel opens | 3fa4fb5 | PASS |
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
| uncommitted_edit_visible_after_debounce (B3-001) | FAIL | 7fffe9c (B6 overlay checkpoint) | PASS |
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
(reproduced after the B3 rewrite with `--ignored`). Resolved in B6 (`7fffe9c` /
`fc70117`) via a persisted atomic overlay checkpoint `{head, branch_key, files: path -> blob hash | null}`
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

Source symlinks were a documented split at B4 (checkout refused, tree skipped);
B7 makes both paths skip them (`indexing_skips_source_symlink_outside_repository`,
`source_symlink_is_skipped_on_disk_and_in_tree`).

`cargo nextest run -p gitcortex-mcp -j 4`: 259 run, 259 passed, 1 skipped (B3-001).

## B6: working-tree overlay checkpoint and readiness certification

| scenario | first result | fix commit | final |
|---|---|---|---|
| uncommitted_edit_visible_after_debounce (B3-001 un-ignored) | FAIL (pre-B6) | 7fffe9c | PASS |
| edit_then_settle_reports_ready_and_verified | PASS | 7fffe9c | PASS |
| further_edit_after_checkpoint_returns_unverified_until_settled | PASS | 7fffe9c | PASS |
| checkpoint_for_other_head_is_not_trusted | PASS | 7fffe9c | PASS |
| changed_file_set_mismatch_is_not_certified | PASS | 7fffe9c | PASS |
| deleted_file_overlay_certifies_after_settle | PASS | 7fffe9c | PASS |
| restart_with_valid_checkpoint_recertifies_without_reindex | PASS | 7fffe9c | PASS |
| restart_with_tampered_or_corrupt_checkpoint_stays_unverified | PASS | 7fffe9c | PASS |
| oversized_or_many_changed_files_stay_unverified | PASS | 7fffe9c | PASS |
| begin_leaves_overlay_of_checked_out_key_certified (mutation-checked) | FAIL (pre-rebase order) | 7fffe9c | PASS |
| finish_leaves_overlay_written_during_the_job_certified | PASS | 7fffe9c | PASS |
| successful_job_content_is_not_recertified_from_an_old_overlay | PASS | 7fffe9c | PASS |
| failed_job_cannot_be_recertified_from_an_old_overlay (mutation-checked) | PASS | 7fffe9c | PASS |
| overlay_written_during_the_job_does_not_outlive_its_apply (mutation-checked) | PASS | 7fffe9c | PASS |
| failed_apply_leaves_no_overlay_to_recertify | PASS | 7fffe9c | PASS |

A tree job removes a key's overlay only after confirming the checkout does not own
it, and again before writing, so a partial apply can never sit beside a checkpoint.

B6 full `just ci`: PASS (0 skipped).
`cargo nextest run -p gitcortex-mcp -j 4`: 282 run, 282 passed, 0 skipped.

## B7: trust hardening (mutation epochs, store-level folder pruning, non-blocking diagnostics)

| scenario | first result | fix commit | final |
|---|---|---|---|
| finish_aborts_after_transient_checkout_with_overlay | FAIL: `finish` passed (`outcome.is_some()`) and certified `X` at `S1` with stray `overlay_only` nodes | 9fb4a00 | PASS |
| finish_aborts_when_sync_commit_mutated_graph_without_sha | FAIL: `finish` passed (`outcome.is_some()`) after `sync_commit` mutated `X`'s graph without advancing `last_indexed_sha` | 9fb4a00 | PASS |
| after_delete_last_file_in_directory (`trust_commit_path` + `assert_matches_rebuild`) | FAIL: rebuild mismatch — 4 vanished `Folder` nodes (`parent/vanish`, `parent/vanish/deep`, `parent/vanish/deep/inner`, `standalone_dir`) and 3 `Contains` edges remained in incremental store | bb25c66 | PASS |
| watcher_prunes_vanished_nested_folders_and_keeps_surviving_sibling | FAIL: `left: ["parent", "parent/sibling", "parent/vanish", "parent/vanish/deep", "parent/vanish/deep/inner"]` vs `right: ["parent", "parent/sibling"]` | bb25c66 | PASS |
| graph_stats_returns_indexing_promptly_when_finish_holds_store (`graph_stats` + `health_report`, full + compact) | FAIL: `Elapsed(())` (500ms timeout exceeded; blocked on `store.lock()`) | 4a12449 | PASS |

1. **Per-key monotonic mutation epoch (`<branch>.epoch`)**: stored alongside `<branch>.sha` and `<branch>.dirty` in `gitcortex-store::branch` and exposed via `GraphStore::{mutation_epoch, bump_mutation_epoch}`. Bumped on every branch graph/revision write (`KuzuGraphStore::apply_diff`, `watcher::sync_commit`, `watcher::apply_disk_batch`, `tree_jobs::finish_tree_apply`, `cli::cmd::hook::run`, and `cli::cmd::init::universal::initial_index`). `TreeApply` snapshots `epoch` at `begin_tree_apply` and `apply_tree_index_shared` aborts at `finish` if `store.mutation_epoch(key)? != plan.epoch`, closing both transient-checkout overlay and interrupted `sync_commit` windows without coupling `gitcortex-store` to `gitcortex-mcp`.
2. **Store-level folder pruning in `KuzuGraphStore::apply_diff`**: after deleting replaced/deleted file nodes and explicit `removed_node_ids`, ancestor `Folder` nodes of `diff.removed_files` are checked bottom-up (deepest path first) and deleted when no child `file` or `folder` remains via outgoing `Contains` edges. The duplicate `stale_folders` pass in `tree_jobs.rs` is removed so commit-hook, watcher, and tree-index paths share a single rule while preserving B4-001 dotted-directory survival and folder-edge deduplication.
3. **Non-blocking diagnostics (`graph_stats`, `health_report`)**: `readiness_scopes_inner` (when `diagnostic` is true) and `try_lock_for_diagnostic` use `self.store.try_lock()`, returning `status: "index_not_ready"` with `index.state: "indexing"` immediately on `WouldBlock` instead of parking an async worker on `self.store.lock()` while `finish_tree_apply` holds the store mutex.

B7 full gate (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`, `cargo nextest run -p gitcortex-mcp -j 4`): PASS (0 skipped).
`cargo nextest run -p gitcortex-mcp -j 4`: 287 run, 287 passed, 0 skipped.
Hook timing (`GCX_TIMING=1 gcx hook` with isolated `GCX_STORE_PATH`): 497ms first run, 435ms incremental (under 500ms budget).

### B7 review findings

| finding | scenario (test) | first result | fix commit | final |
|---|---|---|---|---|
| M1 newly ignored file keeps stale nodes | `newly_ignored_file_{edited,deleted}_on_clean_path_*`, `newly_ignored_file_on_dirty_path_*` | FAIL: `alpha` nodes remained and the checkout read `ready` | 842ff0d | PASS |
| M2 git-ignored sources block certification | `gitignored_file_does_not_block_a_{dirty,clean}_checkout`, `reconcile_drops_gitignored_nodes_left_by_an_older_daemon`, `gitignored_nodes_from_before_a_restart_*`; control `tracked_file_under_a_gitignored_directory_is_still_indexed` | FAIL: `.venv` symbols indexed, checkout stayed `unverified`; control PASS | 842ff0d | PASS |
| m6 rules change not tied to evidence | `modified_tracked_ignore_blocks_a_clean_checkout`, `ignore_edited_after_an_overlay_checkpoint_is_not_certified`, `committed_rule_change_drops_newly_ignored_files_on_sync`, `watcher_reconciles_when_ignore_rules_change`, `checkout_incremental_reevaluates_paths_when_rules_change` | FAIL: `ready` after rule edits; committed rules left `alpha`/`gamma` nodes | 842ff0d | PASS |
| m3 ignored checkpoint removal | `checkpoint_removal_failure_aborts_before_the_graph_is_mutated` | FAIL: batch succeeded beside an unremovable checkpoint | d6355c5 | PASS |
| m4 startup clear erases raced rescan | `startup_rescan_clear_keeps_a_rescan_raised_after_the_snapshot` | FAIL (helper absent; unconditional `store(false)`) | d6355c5 | PASS |
| m5 overlay bounds | `overlay_over_the_total_byte_budget_writes_no_checkpoint` | FAIL: checkpoint written that verify always rejects | d6355c5 | PASS |
| m7 flaky immediate re-edit | `further_edit_after_checkpoint_returns_unverified_until_settled` (3 consecutive runs) | intermittent | 20d7526 | PASS |
| m8 checkpoint content gates | `hand_written_checkpoint_{with_a_correct_hash_certifies_valid_source,for_an_oversized_file_is_not_certified,for_a_nul_file_is_not_certified}` | PASS (mutation: disabling both gates fails both) | 20d7526 | PASS |
| S source symlinks fail checkout index | `indexing_skips_source_symlink_outside_repository` (#77), `live_update_never_reads_through_a_symlinked_directory`, `source_symlink_is_skipped_on_disk_and_in_tree`, `source_path_skips_symlinked_parent` | FAIL: `run(None)` errored on a committed symlink | 05e020b | PASS |
| busy diagnostic loses target revision (found by the gate) | `busy_diagnostic_still_reports_the_requested_target_revision`, `readiness_protocol` | FAIL: `target_revision` null while the watcher held the store | c24fa31 | PASS |

- **Ignore rules.** Checkpoints record the working-tree `.gitcortex/ignore` hash (`ignore`) and verify compares it. A committed rules file with uncommitted edits keeps the checkout `unverified` on both clean and dirty paths. Untracked rules, as `gcx init` writes them, stay certifiable through the hash and the watcher's reconcile on any rules change. Checkout indexing re-evaluates every path when committed rules change, and removes excluded files without reading them.
- **Git-ignored files.** Untracked files matched by `.gitignore` are never indexed by the watcher; their nodes are removed. Tracked matches stay indexed.
- **Symlinks.** Disk reads skip any path with a symlink component (one warning per skipped path), matching tree indexing. Tests make the outside target unreadable, so any read through a link fails the index.
- **Deferred.** An untracked rules file edited while no daemon runs is not detected on the clean path until `gcx serve` starts (startup always reconciles unless a checkpoint with a matching rules hash recertifies).
- The B7 WIP left by an interrupted agent (a partial revert of 4a12449) was discarded: it restored a blocking store lock on the diagnostic path and failed `graph_stats_returns_indexing_promptly_when_finish_holds_store`. The race it targeted is fixed in c24fa31 instead.

B7 review full gate: `just ci` PASS (`cargo test --workspace`: 514 passed, 0 failed, 0 ignored).
`cargo nextest run -p gitcortex-mcp -j 4`: 309 run, 309 passed, 0 skipped.
