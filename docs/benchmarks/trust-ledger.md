# Trust scenario ledger

Fingerprints compare persisted node metadata, spans and complete edge records
against a full rebuild in a separate repository copy.

| scenario | first result | fix commit | final |
|---|---|---|---|
| after_create_file | PASS | — | PASS |
| after_modify_body | FAIL: fixture Kuzu virtual-address reservation exhausted under parallel opens | a4dc932 | PASS |
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

B1: the missing-support compile failure preceded implementation; all eight
self-tests then passed. B2 required a harness resource fix, not a graph fix.
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
| deleted_file_symbols_disappear | FAIL: test unwrapped items during transient degraded response | — (test polling corrected) | PASS |
| branch_omitted_answers_current_checkout | PASS | — | PASS |
| branch_switch_while_running_updates_index_branch | PASS | — | PASS |
| restart_with_dirty_overlay_not_ready_until_verified | FAIL: test startup barrier required a clean commit for a dirty overlay | — (wait for watcher health; preserve readiness assertion) | PASS |
| detached_head_never_answers_main | PASS | — | PASS |
| array_results_validate_against_output_schema | FAIL: test expected callers under items rather than evidence | — (test response-shape assertion corrected) | PASS |

B3-001: after correcting transient-response polling, the function is visible
within MAX_BATCH_MS + 2s, but the response is `unverified` with
`working_tree_verified=false`. Both readiness inspection and watcher persistence
certify committed content only; the watcher intentionally keeps a dirty marker
for uncommitted overlays. Safely certifying an overlay requires a distinct live
checkpoint with source-hash revalidation across requests/restarts, rather than
clearing that persisted marker or equating watcher health with verified content.
That changes readiness semantics and shared watcher state, so this scenario is
left as an explicit failing regression under the plan's architectural-stop rule.
No production fix is claimed.

During B3 targeted runs the real-root count rose 9371 → 9372 from another
worktree's hook (the new directory contains
`stack__f_ui__6_type_foundation.sha`). Trust daemon children use explicit
isolated store paths. Full-gate before/after counts are recorded separately.

B3 full `just ci`: PASS (one documented ignored scenario);
real-root count 9372 → 9372. The initial Clippy diagnostic in test code was
corrected before this final gate. No library behavior changed in B1–B3.
