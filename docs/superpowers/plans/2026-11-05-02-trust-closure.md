# Trust Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every "When GCX is weak" item from the 2026-10-17 feedback is reproduced by a test and either fixed or explicitly reported to the caller. That includes answering about a branch that is not checked out.

**Architecture:** Isolate test stores. Build a scenario harness that compares the full persisted incremental graph against a rebuild of a separate repo copy. Run lifecycle scenarios through the hook path and the MCP protocol. Add tree-based indexing so any local branch can be indexed from git objects without a checkout, scheduled in the background by the daemon. Tighten agent-facing wording.

**Tech Stack:** Rust, git2, tree-sitter, KuzuDB, rmcp, tokio (mcp crate only).

**Spec:** [Program overview](2026-11-05-00-release-program.md) § "Feedback → owner plan". `docs/CONTINUOUS-CODING-PLAN.md` §1. Work lands on `release/r1-combined-validation` (PR 5).

## Global Constraints

- An empty, missing, or stale graph is never reported as "no dependencies". Every MCP response carries the readiness envelope under `index` (`index.state`, `index.branch`, `index.working_tree_verified`, `index.next_action`), as inserted at `mcp/tools.rs:2028` and read by `server.rs` (`daemon_readiness`) and `audit.rs`. Do not rename it.
- No `.unwrap()` in library code. `tokio` only in `gitcortex-mcp`. The indexer and store stay sync.
- The hook path (Pass 1) stays under 500 ms on the existing fixtures. Tree indexing never runs inside a git hook.
- Tests never write to the real data root.
- Only one `KuzuGraphStore` per repository per process: `open` takes the repository lock (`kuzu/mod.rs:790`) and `data_root()` is process-global (`branch.rs:98`). Harnesses rebuild into a **separate repo copy** (own repo id, own lock); daemon jobs write through the daemon's existing store handle.
- Tree indexing has one apply path and one commit-readiness function, shared with [Plan 05](2026-11-05-05-pr-impact-action.md) E2 (`gcx index --rev`). A tree key is certified (`.dirty` absent, `{key}.sha` written) only after a complete apply; a failed or interrupted apply stays `unverified`. Plan 02 T4 owns `resolve_commit`, `apply_tree_index`, and `inspect_commit`; E2 consumes them and adds no second resolver, apply sequence, or readiness function.

## Review Focus

1. Branch names with `/`, `-`, `.`, or unicode (`feat/ü-x`) in tree indexing → indexed under the same `sanitize` key as a checkout index. Test: T4 `tree_index_handles_unicode_branch`.
2. The harness misses stale content (bodies, signatures, spans, metadata, edge line/confidence, misresolved endpoints, duplicate qualified names) → false "incremental equals rebuild". Test: T1 `fingerprint_detects_stale_body`, `fingerprint_detects_stale_span`, `fingerprint_detects_misresolved_edge`, `fingerprint_counts_duplicate_qualified_names`.
3. Requested branch does not exist (typo) → `index.state == "missing"` with `next_action` naming the bad ref; no indexing loop. Repeated requests for an indexing branch start one job; a failed job is reported and retryable. Test: T4 `unknown_branch_reports_missing_without_indexing`, `duplicate_requests_spawn_one_job`, `failed_tree_index_reports_error_and_allows_retry`.
4. A tree index runs while the watcher indexes the checked-out branch → both finish; neither branch's tables are corrupted. Test: T4 `tree_index_concurrent_with_watcher`.
5. Detached HEAD → answers use the short-SHA key, never `main`. Test: T3 `detached_head_never_answers_main`.
6. Commit readiness depends on the checkout (HEAD, statuses) or certifies a half-applied tree → a bare or unborn clone holding a fetched commit is wrongly unavailable, or a failed apply reads `ready`. Test: T4 `inspect_commit_ready_in_bare_repo`, `inspect_commit_ready_with_unborn_checkout`, `failed_tree_apply_stays_unverified`, `inspect_checkout_semantics_unchanged`.

---

### Task 1: Test store isolation and scenario harness

**Files:**
- Modify: `.cargo/config.toml`, `tools/agent-bench/bench.py`, `docs/DEV_SETUP.md`
- Create: `crates/gitcortex-mcp/tests/support/mod.rs`, `crates/gitcortex-mcp/tests/support_selftest.rs`

**Interfaces:**
- Produces (in `support`):
  - `TestRepo::new() -> TestRepo`, with `.root() -> &Path`, `.write(rel, contents)`, `.remove(rel)`, `.rename(from, to)`, `.git(args: &[&str]) -> String`, `.commit(msg) -> String /*sha*/`, and `.snapshot_copy() -> TestRepo` (recursive copy of the worktree **including `.git` and uncommitted files** into a new temp dir; distinct canonical root ⇒ distinct `repo_id` and `RepositoryLock`, same `GCX_STORE_PATH`)
  - `index_hook_path(repo: &TestRepo, store: &mut KuzuGraphStore, branch: &str)` (same calls as `gcx hook`)
  - `NodeKey { file: String, kind: String, qualified_name: String, start_line: u32, end_line: u32 }` — endpoint identity without UUIDs (some parsers mint `NodeId::new()`, e.g. `parser/typescript.rs:160`)
  - `NodeRecord { key: NodeKey, name: String, span: String /*Debug of Span*/, metadata: String /*canonical serde_json of NodeMetadata, incl. definition signature/body/doc/bytes and lld*/ }`
  - `EdgeRecord { src: Endpoint, kind: String, dst: Endpoint, line: Option<u32>, confidence: String }`, `enum Endpoint { Node(NodeKey), Unresolved(String /*raw id*/), Ambiguous(String) }`
  - `Fingerprint { nodes: BTreeMap<NodeRecord, usize>, edges: BTreeMap<EdgeRecord, usize> }` — multisets, so repeated qualified names are counted, not collapsed
  - `fingerprint(store: &dyn GraphStore, branch: &str) -> Fingerprint` (from `list_all_nodes` + `list_all_edges`)
  - `assert_matches_rebuild(repo: &TestRepo, store: &dyn GraphStore, branch: &str)`: `repo.snapshot_copy()`, full `IncrementalIndexer::run(None)` into `KuzuGraphStore::open(copy.root())` in-process, compare fingerprints, panic with a readable per-record diff

- [ ] **Step 1:** Add to `.cargo/config.toml` `[env]`: `GCX_STORE_PATH = { value = "target/gcx-test-store", relative = true }`. This also keeps dev `cargo run` from touching the real store; note that in `docs/DEV_SETUP.md`.
- [ ] **Step 2: Write failing tests** in `support_selftest.rs`:
  - `store_path_is_isolated`: asserts `std::env::var("GCX_STORE_PATH")` ends with `target/gcx-test-store`.
  - `rebuild_copy_has_own_repo_id_and_lock`: original store stays open while the copy's store opens; repo ids differ.
  - Negative self-tests (`#[should_panic]` on `assert_matches_rebuild`): `fingerprint_detects_added_symbol` (add `pub fn extra() {}`, no reindex), `fingerprint_detects_stale_body` (change a body, signature unchanged), `fingerprint_detects_stale_span` (insert a blank line above a fn), `fingerprint_detects_misresolved_edge` (apply a `GraphDiff` that repoints a `Calls` edge to a same-named fn in another module), `fingerprint_counts_duplicate_qualified_names` (two nodes share a qualified name; remove one from the store only).
  - `fingerprint_equal_after_full_reindex` (positive control).
- [ ] **Step 3: Run** `cargo test -p gitcortex-mcp --test support_selftest`. Expected: FAIL (no `support`).
- [ ] **Step 4:** Implement `support/mod.rs` with the signatures above. Set `GCX_STORE_PATH` per child `Command` only; never call `set_var` in-process.
- [ ] **Step 5:** In `bench.py`, set `GCX_STORE_PATH` to `DEFAULT_WORK / "store"` for every spawned `gcx`.
- [ ] **Step 6: Run** the tests. Expected: PASS. Then run `cargo test --workspace`, and `ls ~/.local/share/gitcortex | wc -l` before and after shows no new dirs.
- [ ] **Step 7: Commit.** `test: isolate graph stores and add rebuild-equivalence harness`

### Task 2: Commit-path lifecycle scenarios

**Files:**
- Create: `crates/gitcortex-mcp/tests/trust_commit_path.rs`, `docs/benchmarks/trust-ledger.md`
- Modify: whatever indexer/store code a failing scenario points to

- [ ] **Step 1: Write tests.** Each one indexes, mutates, commits, runs `index_hook_path`, then calls `assert_matches_rebuild` (full-content fingerprint): `after_create_file`, `after_modify_body`, `after_modify_signature`, `after_shift_lines` (span-only change), `after_delete_file`, `after_rename_file`, `after_rename_symbol`, `after_commit_amend`, `after_reset_hard_to_parent`, `after_fast_forward_merge`, `after_rebase_onto_main`, `after_cross_file_caller_removed` (caller edge must disappear), `after_duplicate_name_added` (a second same-qualified-name symbol).
- [ ] **Step 2: Run** `cargo test -p gitcortex-mcp --test trust_commit_path`. Record each result in `trust-ledger.md` (`scenario | first result | fix commit | final`).
- [ ] **Step 3:** For each failure, fix the root cause (superpowers:systematic-debugging) and rerun until it passes. Commit each fix separately: `fix: <scenario> leaves stale graph state`.
- [ ] **Step 4: Commit.** `test: lifecycle scenarios prove incremental equals rebuild`

### Task 3: Live MCP scenarios (watcher, branch, restart)

**Files:**
- Create: `crates/gitcortex-mcp/tests/trust_live.rs` (follow the client setup in `readiness_protocol.rs`)

- [ ] **Step 1: Write tests** (all readiness assertions read `structured_content["index"]`, as `readiness_protocol.rs:76` does):
  - `uncommitted_edit_visible_after_debounce`: write a new fn, then within `MAX_BATCH_MS + 2s` `lookup_symbol` finds it, and `index.working_tree_verified == true`.
  - `deleted_file_symbols_disappear`.
  - `branch_omitted_answers_current_checkout`: index `main`, switch to `feat/x`, call without `branch`. Then `index.branch == "feat/x"`, and the response holds `feat/x` data or `index.state != "ready"`. Never `main` data.
  - `branch_switch_while_running_updates_index_branch`.
  - `restart_with_dirty_overlay_not_ready_until_verified`.
  - `detached_head_never_answers_main`.
  - `array_results_validate_against_output_schema`: `list_definitions` and `find_callers` responses pass the client's `structured_content` schema check.
- [ ] **Step 2: Run** `cargo test -p gitcortex-mcp --test trust_live`. Fix failures as in T2 Step 3 and log them in the ledger.
- [ ] **Step 3: Commit.** `test: live MCP trust scenarios`

### Task 4: Index any local branch from git objects

**Files:**
- Create: `crates/gitcortex-indexer/src/tree_source.rs` (with `#[cfg(test)]` unit tests), `crates/gitcortex-mcp/src/mcp/tree_jobs.rs`, `crates/gitcortex-mcp/tests/tree_index.rs` (store-backed equivalence; beside `tests/support`), `crates/gitcortex-cli/src/cmd/index.rs`
- Modify:
  - `crates/gitcortex-indexer/src/indexer.rs` (`IncrementalIndexer`): abstract the file source, currently `fs::read_to_string` at ~line 497.
  - `crates/gitcortex-mcp/src/mcp/readiness.rs` (new state), `crates/gitcortex-mcp/src/mcp/audit.rs` (add `"indexing"` to the state whitelist at ~line 103).
  - `crates/gitcortex-mcp/src/mcp/server.rs` / `tools.rs`: own a `TreeIndexJobs` and route requests through it.
  - `crates/gitcortex-cli/src/main.rs`, `docs/REFERENCE.md`.
  - `crates/gitcortex-mcp/tests/trust_live.rs` (new MCP tests).

**Interfaces:**
- Produces:
  - `IncrementalIndexer::run_tree(&self, rev: &str, from_sha: Option<&str>) -> Result<(GraphDiff, String /*commit sha*/)>` (the real type exported at `indexer/src/lib.rs:5`; there is no `Indexer`). It reads blobs with git2, applies `.gitcortex/ignore` **as stored in that tree**, and uses the same size and binary limits as the disk path.
  - Crate-private `trait SourceProvider` with `DiskSource` and `TreeSource`.
  - `TreeIndexJobs` (mcp, tokio): `fn request(&self, branch: &str, rev_sha: &str) -> JobStatus` where `enum JobStatus { Started, AlreadyRunning, Failed { error: String, retry_after: Instant } }`. Holds `Mutex<HashMap<String /*sanitized key*/, JobEntry>>` plus a `tokio::sync::Semaphore` of 1 permit (new; none exists in `mcp/src` today). Jobs run in `spawn_blocking`, write through the daemon's shared store handle (never a second `KuzuGraphStore`), and a drop guard removes the `Running` entry on error or panic. Failures are kept for 60 s, then the next request retries.
  - Shared resolver in `tree_jobs.rs`: `pub fn resolve_commit(root: &Path, rev: &str) -> anyhow::Result<String>` resolves `rev^{commit}` to a full SHA with git2, without checkout/HEAD/status probes. Both branch jobs and Plan 05 E2 use it; E2 passes the resolved SHA as both key and revision to apply.
  - Shared sync apply path in `tree_jobs.rs`: `pub fn apply_tree_index(root: &Path, store: &mut dyn GraphStore, key: &str, rev: &str) -> anyhow::Result<TreeIndexOutcome>`, `TreeIndexOutcome { sha: String /*full commit*/, nodes: usize, edges: usize }`. Order: call `resolve_commit(root, rev)` (no checkout, HEAD, or status) → `set_working_tree_dirty(key, true)` → `IncrementalIndexer::run_tree(&sha, last_indexed_sha(key))` → `apply_diff` → `set_last_indexed_sha(key, &sha)` → `set_working_tree_dirty(key, false)`. Abort before mutation if resolution or setting the dirty marker fails. After mutation starts, any error leaves `.dirty` set, so the key reads `unverified`, never `ready`. A retry after a dirty/failed apply must rebuild the key completely rather than trusting its old SHA as an incremental baseline. `TreeIndexJobs` calls it inside `spawn_blocking`; `gcx index` calls it directly. Plan 05 E2 `index_revision` calls it with `key = rev = <full SHA>`.
  - `readiness::inspect_commit(store: &dyn GraphStore, key: &str, sha: &str) -> IndexReadiness` (the caller passes a resolved full SHA). Compares `last_indexed_sha(key)` with `sha` and reads the persisted `.dirty` marker. It never calls `detect_current_branch`, `rev-parse HEAD`, or `changed_sources`, so it works in bare repos and unborn checkouts. State precedence: either store read error → `unavailable`, `.dirty` present → `unverified` (even with missing or mismatched SHA), no SHA → `missing`, different SHA → `stale`, otherwise `ready`. Fields: `branch = key`, `checkout_branch`/`checkout_revision = None`, `target_revision = Some(sha)`, `indexed_revision` = actual, `working_tree_verified = false` (no checkout is certified). The existing checkout `inspect(root, store, branch)` keeps its signature and behavior.
  - New readiness state `"indexing"`, with `next_action: "Branch <b> is being indexed from commit <sha>; retry shortly."`. A failed job reports `"missing"` with `next_action` containing the error.
  - CLI `gcx index --branch <name>` (sync; prints node and edge counts), implemented as a clap `ArgGroup` so Plan 05 E2 adds the mutually exclusive `--rev <commit>` to the same command (no `--key` flag). `--branch` keys by the branch name (`sanitize`d by the store, as a checkout index); `--rev` keys by the full SHA.

- [ ] **Step 1: Write failing tests:**
  - Indexer unit tests in `tree_source.rs` (git2 + `tempfile`, no store): `tree_source_lists_same_paths_as_disk`, `tree_source_honors_ignore_in_tree`, `tree_index_skips_binary_and_oversized` (`run_tree` diff equals `run` diff on the same commit).
  - `crates/gitcortex-mcp/tests/tree_index.rs` (uses `support::fingerprint`): `tree_index_matches_checkout_index` (index `feat/x` from the tree while `main` is checked out; fingerprint equals checkout + hook path), `tree_index_incremental_from_sha`, `tree_index_handles_unicode_branch`.
  - In `trust_live.rs`: `requested_unindexed_branch_starts_tree_index` (first call `index.state == "indexing"`; a retry within 10 s gives `"ready"` with nodes > 0), `unknown_branch_reports_missing_without_indexing`, `duplicate_requests_spawn_one_job` (5 concurrent calls → one job), `failed_tree_index_reports_error_and_allows_retry` (corrupt blob → `"missing"` with the error; later request re-runs), `tree_index_concurrent_with_watcher`.
  - `readiness.rs` unit tests (in-memory or temp store, git2 fixtures): `inspect_commit_ready_in_bare_repo` (`git init --bare` + fetched commit, indexed via `apply_tree_index`), `inspect_commit_ready_with_unborn_checkout` (unborn `HEAD`, fetched valid commit), `inspect_commit_wrong_sha_is_stale`, `inspect_commit_dirty_marker_is_unverified`, `inspect_commit_missing_key_is_missing`. Baseline pin `inspect_checkout_semantics_unchanged` (existing `inspect` states on a checkout; may already PASS).
  - In `tree_index.rs`: `failed_tree_apply_stays_unverified` (inject a failing `apply_diff`; `.dirty` remains, `{key}.sha` unchanged, `inspect_commit` ≠ `ready`), `tree_apply_publishes_sha_only_after_success`, `failed_first_apply_is_unverified_without_sha`, `dirty_retry_rebuilds_complete_tree`.
- [ ] **Step 2: Run** them. Expected: FAIL (`run_tree`, `apply_tree_index`, `inspect_commit` undefined); the baseline pin PASSes.
- [ ] **Step 3: Implement** `SourceProvider`, `TreeSource`, and `IncrementalIndexer::run_tree`. Keep the trait private to the crate; it is not a public abstraction. `run_tree` must not depend on a working directory or `HEAD` (the disk ignorer from `IncrementalIndexer::new` is unused on this path). Then `resolve_commit`, `apply_tree_index`, and `inspect_commit` with the signatures above. Land these `readiness.rs` edits here, before Plan 05 E2 starts, so E2 only adds CLI wiring.
- [ ] **Step 4:** MCP: when the requested branch is a local ref, is not checked out here, and its state is missing or stale, call `TreeIndexJobs::request` and return `"indexing"` (or the failure). A non-existent ref returns `"missing"` immediately and never enqueues.
- [ ] **Step 5: Run** all trust tests plus `cargo test --workspace`. Expected: PASS. Then check hook timing on a fixture with `GCX_TIMING=1 gcx hook` and confirm it is unchanged.
- [ ] **Step 6:** Document `gcx index` and the `indexing` state in REFERENCE.
- [ ] **Step 7: Commit.** `feat: index any local branch from git objects without checkout`

### Task 5: Agent-facing wording

**Files:**
- Modify: `crates/gitcortex-cli/src/cmd/init/universal.rs` (`AGENT_GUIDE`), and the response builders in `crates/gitcortex-mcp/src/mcp/tools.rs` for unused symbols, risk/blast radius, and god nodes

- [ ] **Step 1: Write failing tests:**
  - `agent_guide_forbids_empty_graph_inference` (unit, `universal.rs`). The guide contains, verbatim: "An empty or stale graph is never evidence that dependencies do not exist. Check `index.state`; if it is not `ready`, fall back to source search and say so."
  - `lead_results_are_labelled` (in `trust_live.rs`). Responses from `find_unused_symbols`, `pre_edit_impact` risk, and god nodes include `"evidence": "lead"`.
- [ ] **Step 2: Run** them. Expected: FAIL.
- [ ] **Step 3: Implement.** Add the sentence and the `evidence` field. Do not change the other fields.
- [ ] **Step 4: Run** the tests. Expected: PASS. Then run `just ci`.
- [ ] **Step 5: Commit.** `feat: make readiness and lead semantics explicit to agents`
