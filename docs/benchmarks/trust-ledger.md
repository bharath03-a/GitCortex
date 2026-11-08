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
No scenarios are ignored.

Isolation evidence: B1 full `just ci` real-root count was 9370 → 9370.
The initial session count was 9291; it rose to 9370 before that full run.
The repository's post-commit hook subsequently created a real-root store
(9371); later outer Git commands explicitly pass the isolated store path.
Fixture Git commands already pass that path and disable hooks.

B2 full `just ci`: PASS; real-root count 9371 → 9371.
