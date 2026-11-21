# Storage Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Storage stays bounded for big repos with many branches and worktrees: one graph per repository rather than per worktree, no garbage from deleted branches, detached checkouts, removed clones, or leftover backup dirs, and an honest report of what is used.

**Architecture:** Every data dir records its owner (repo metadata and a branch registry). `gcx storage` reports and prunes, dry-run by default. Branch GC drops tables for refs that no longer exist and runs automatically at most once a day from the branch-switch hook. Compaction reclaims Kuzu file space by swapping only the DB payload inside the data dir, so the lock file and sidecars never move. Repository identity moves from the worktree path to the git common dir, so all worktrees share one store and one daemon (ADR-0005).

**Tech Stack:** Rust, KuzuDB, git2, serde_json, blake3, fs2.

**Spec:** [Program overview](2026-11-05-00-release-program.md). Maintainer, 2026-11-05: "storage itself is growing huge … for now worry about how we optimize our product".

**Facts behind the design** (verified 2026-11-05):
- `branch::repo_id` hashes the supplied absolute repo-root path (it does not run git itself), so each worktree gets its own `graph.kuzu`.
- Each branch gets full `{key}_nodes/_edges/_references` tables (`schema.rs`), and nothing ever drops them.
- Detached HEAD is keyed by short SHA, derived **twice**: MCP `mcp/helpers.rs:~109` and CLI `cmd/hook.rs:100–110` (`current_branch`). Every rebase, bisect, or detached checkout adds a full table set.
- Data dirs record no owner path, so orphans can't be detected.
- A data dir holds the DB payload (`graph.kuzu`, `graph.kuzu.wal`) plus sidecars that must survive any maintenance: `serve.lock`, `schema_version`, `{key}.sha`, `{key}.dirty`, `embeddings_{key}.bin`, `editor_prompted`, `daemon.log`, `mcp.sock`, audit `calls.jsonl*`, and (from T1/T3) `repo.json`, `branches.json`, `last_branch_gc`.
- Schema rebuilds do **not** create backups: `wipe_repo_data` (`branch.rs:276–291`) wipes contents but keeps the `serve.lock` inode so the held lock stays valid. `rg 'bak-' crates` finds nothing. `<id>.bak-<ts>` dirs observed on the maintainer machine are historical, of unknown origin; T2 treats them as prunable `Backup`.
- Test stores leaked into the real root (fixed by Plan 02 T1).
- The proxy preamble is one byte whose value selects compact/full mode (`server.rs:19–20`, `spawn_client` ~line 163; CLI `cmd/serve.rs:12–13,38`; `daemon_readiness` sends `COMPACT_MODE` at `server.rs:52`).

## Global Constraints

- Every destructive command defaults to dry-run. Nothing is deleted without `--apply`.
- A data dir whose lock (`serve.lock`) is held is never modified. Maintenance never moves, renames, or recreates `serve.lock` or the data dir itself; it holds the lock for the whole operation.
- The index is a cache: the worst-case recovery from over-pruning is a re-index. Even so, active repos are never pruned automatically.
- `GCX_STORE_PATH`, `XDG_DATA_HOME`, and the legacy macOS `~/.local/share/gitcortex` resolution keep working (`branch::data_root`).
- Automatic work in hooks stays under 50 ms when no GC is due.
- Layering: `gitcortex-store` never depends on `gitcortex-indexer` or git2. Any re-index lives in CLI/MCP orchestration.
- Detached keys are built by one function and resolved by one resolver; no producer or reader derives them on its own. Key kind (`Branch`/`Detached`/`Commit`) is decided by one classifier, `branch::key_kind`.
- Commit keys (full-SHA keys written by Plan 02 `apply_tree_index` via `gcx index --rev`, [Plan 05](2026-11-05-05-pr-impact-action.md) E2) are immutable tree snapshots, not worktree detached keys. They have no ref and no worktree, are never kept or dropped by ref existence or current-HEAD checks, and default GC never removes them.
- **Closed snapshot contract** (consumed by Plan 05 X2 base checkpoints): a copy of a data dir is valid only if it is taken while holding the exclusive `fs2`/`flock` lock on the existing `serve.lock` inode (never renamed, unlinked, or recreated), after every `KuzuGraphStore` and gcx child has exited. It contains exactly the `Payload` and `Durable` entries of T1 `data_dir_inventory` and no `Runtime` entry. It is bound to `branch::IDENTITY_VERSION`, `schema_version`, and the gcx version. A restore whose recorded `worktree_root`/`git_common_dir` differs from the opener's actual paths is a mismatch: the consumer rebuilds. GitCortex never merges two checkpoint DBs and never silently relocates (renames or re-ids) a restored store.
- Depends on Plan 02 T1 (isolated test stores, `support::fingerprint`) and T4 (`IncrementalIndexer::run_tree`, used only by the CLI compaction fallback).

## Review Focus

1. An upgraded user's active repo has no metadata yet → it must not be pruned. Test: T2 `unknown_dirs_not_pruned_without_flag`.
2. A repo on an unmounted volume looks missing → classify it `Unreachable`, never `Orphaned`. Test: T1 `missing_parent_is_unreachable`.
3. Prune, GC, or compact while another process opens the store → skipped or refused; the lock inode never moves; a crash at any compaction phase leaves an openable DB with all sidecars. Test: T2 `locked_dir_skipped`, T3 `gc_skips_when_locked`, T4 `open_refused_during_each_compact_phase`, `compact_failure_restores_original`, `compact_preserves_sidecars`.
4. Two worktrees on different branches share one daemon, in compact and full mode, and old/new proxies fail loudly against each other. Test: T5 `two_worktrees_answer_own_branch`, `handshake_carries_mode`, `legacy_mode_byte_rejected`, `new_handshake_against_old_daemon_reports_restart`.
5. Two worktrees detached at the same SHA with different dirty edits → no shared overlay, and the hook writes the same key the MCP reads. Test: T5 `detached_keys_include_worktree`, `hook_then_mcp_lookup_in_two_detached_worktrees`.
6. A checkpoint copy misses a durable file (WAL, `.sha`, `.dirty`, `schema_version`, metadata) or includes a runtime file; GC drops a commit key an analysis is using; the identity change reuses old checkpoints or relocates a restored store. Test: T1 `inventory_classifies_every_known_entry`, `unknown_entry_is_reported_not_copied`, T3 `commit_keys_survive_default_gc`, `active_comparison_keys_survive_gc_between_subprocesses`, `commit_key_is_not_detached`, T5 `identity_version_bumped_with_common_dir_identity`, `mismatched_recorded_identity_is_not_migrated`.

---

### Task 1: Repo metadata, branch registry, `gcx storage list`

**Files:**
- Create: `crates/gitcortex-store/src/meta.rs`, `crates/gitcortex-store/src/usage.rs`, `crates/gitcortex-cli/src/cmd/storage.rs`
- Modify: `crates/gitcortex-store/src/lib.rs`, `crates/gitcortex-store/src/branch.rs` (identity version and payload inventory helpers), `crates/gitcortex-store/src/kuzu/mod.rs` (`open_with_schema_policy`, `ensure_branch`, `set_last_indexed_sha`), `crates/gitcortex-cli/src/main.rs`
- Test: `crates/gitcortex-store/tests/storage_meta.rs`

**Interfaces:**
- Produces:
  - `pub const IDENTITY_VERSION: u32 = 1;` in `branch.rs` (current toplevel-path identity; T5 bumps it). Cache/checkpoint consumers (Plan 05 X2 `indexCacheKey`) include it.
  - `RepoMeta { format: u32 /*=1*/, identity_version: u32, worktree_root: PathBuf, git_common_dir: PathBuf, created_at: u64, last_opened_at: u64 }` (unix seconds). `read_repo_meta(repo_id) -> Result<Option<RepoMeta>>` and `write_repo_meta(repo_id, &RepoMeta) -> Result<()>`, written atomically (tmp + rename) to `repo.json`. `last_opened_at` is rewritten at most once per 3600 s.
  - `BranchKind { Branch, Detached, Commit }`. `branch::key_kind(name: &str) -> BranchKind` (pure, the only classifier): a full 40- or 64-char lowercase hex object id → `Commit`; a key built by `detached_key` (T5; short SHA before T5) → `Detached`; else `Branch`. A branch literally named as a full SHA classifies as `Commit`, which is safe: it is never ref-dropped. `BranchEntry { name: String, key: String, kind: BranchKind, worktree_id: Option<String> /*None for Commit*/, last_indexed_at: u64 }`. `read_branch_registry(repo_id) -> Result<Vec<BranchEntry>>`, `upsert_branch(repo_id, BranchEntry) -> Result<()>` (`branches.json`).
  - `branch::payload_files(repo_id) -> [PathBuf; 2]` (`graph.kuzu`, `graph.kuzu.wal`), moved here from T4 so the inventory and T4 share one definition.
  - `enum EntryClass { Payload, Durable, Runtime, Unknown }`, `DataDirInventory { entries: Vec<(PathBuf, EntryClass)> }`, `usage::data_dir_inventory(repo_id) -> Result<DataDirInventory>`. Payload = `payload_files` that exist. Durable = `schema_version`, `{key}.sha`, `{key}.dirty`, `embeddings_{key}.bin`, `editor_prompted`, `repo.json`, `branches.json`, `last_branch_gc`. Runtime (never copied, never moved) = `serve.lock`, `mcp.sock`, `daemon.log`, audit `calls.jsonl*`, and `worktrees/*/` runtime files (T5). Anything else, including an interrupted compaction's `compact-*/` or `graph.kuzu.pre-compact-*`, is `Unknown`: reported, never silently copied, and a snapshot is refused while any `Unknown` entry exists. This is the authoritative list for the closed snapshot contract.
  - `RepoStatus { Active, Orphaned, Unreachable, Unknown, Backup }`. `RepoUsage { repo_id: String, dir: PathBuf, bytes: u64, meta: Option<RepoMeta>, status: RepoStatus, branches: Vec<BranchEntry> }`. `scan_store_root(root: &Path) -> Result<Vec<RepoUsage>>`.
  - CLI `gcx storage list [--all] [--json]`. The default shows the current repo only.

- [ ] **Step 1: Write failing tests:** `meta_written_on_open` (records `identity_version == IDENTITY_VERSION`), `meta_last_opened_throttled`, `registry_records_branch_and_detached`, `registry_records_commit_key` (`set_last_indexed_sha` on a full-SHA key → `kind == Commit`, `worktree_id == None`), `key_kind_distinguishes_commit_detached_branch`, `inventory_classifies_every_known_entry` (every sidecar in "Facts" plus `graph.kuzu.wal` lands in its class), `unknown_entry_is_reported_not_copied`, `scan_classifies_active`, `scan_classifies_orphaned` (meta path deleted, parent exists), `missing_parent_is_unreachable`, `scan_unknown_without_meta`, `scan_backup_suffix` (historical `x.bak-1787394447`).
- [ ] **Step 2: Run** `cargo test -p gitcortex-store --test storage_meta`. Expected: FAIL.
- [ ] **Step 3: Implement** the modules and hooks into open, ensure_branch, and set_last_indexed_sha, plus the CLI. Bytes are a recursive file size sum.
- [ ] **Step 4: Run** the tests. Expected: PASS. Then run `gcx storage list --all --json | jq length` on a dev machine.
- [ ] **Step 5: Commit.** `feat: record store ownership and add gcx storage list`

### Task 2: `gcx storage prune`

**Files:**
- Modify: `crates/gitcortex-store/src/usage.rs`, `crates/gitcortex-cli/src/cmd/storage.rs`
- Test: `crates/gitcortex-store/tests/storage_prune.rs`

**Interfaces:**
- Produces: `PrunePolicy { orphaned: bool, backups: bool, unknown_older_than_days: Option<u32> }`, `PrunePlan { items: Vec<(RepoUsage, &'static str /*reason*/)>, bytes: u64 }`, `plan_prune(root, &PrunePolicy) -> Result<PrunePlan>`, and `apply_prune(&PrunePlan) -> Result<PruneReport { removed: u64, skipped_locked: Vec<PathBuf> }>`. CLI: `gcx storage prune [--orphaned] [--backups] [--unknown-older-than <days>] [--apply]`. With no selector flags it means `--orphaned --backups`. `--backups` covers historical `*.bak-*` dirs and leftover in-dir `graph.kuzu.pre-compact-*` payloads (T4) whose repo lock is free.

- [ ] **Step 1: Write failing tests:** `dry_run_deletes_nothing`, `apply_removes_orphaned_only`, `backups_removed_with_flag`, `pre_compact_leftover_removed_with_backups`, `unknown_dirs_not_pruned_without_flag`, `unknown_older_than_uses_graph_mtime`, `locked_dir_skipped` (hold `serve.lock` via `fs2`), `unreachable_never_pruned`.
- [ ] **Step 2: Run.** Expected: FAIL. **Step 3: Implement.** **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat: add dry-run-first gcx storage prune`

### Task 3: Branch GC (deleted branches, stale detached snapshots)

**Files:**
- Modify: `crates/gitcortex-store/src/schema.rs`, `crates/gitcortex-store/src/kuzu/mod.rs`, `crates/gitcortex-cli/src/cmd/hook.rs`, `crates/gitcortex-cli/src/cmd/storage.rs`
- Test: `crates/gitcortex-store/tests/branch_gc.rs`

**Interfaces:**
- Produces:
  - `schema::drop_branch(conn: &mut Connection, key: &str) -> Result<()>`. It drops `_edges` and `_references` before `_nodes`, and is idempotent.
  - `KuzuGraphStore::prune_branches(&mut self, keep: &dyn Fn(&BranchEntry) -> bool) -> Result<BranchGcReport { dropped: Vec<String>, kept: usize }>`. It also removes `{key}.sha`, `{key}.dirty`, `embeddings_{key}.bin`, and the registry entry. This is an inherent method, not on `GraphStore` (backend maintenance).
  - Default keep policy (computed in the CLI, which has git access; the store only receives the predicate): a `Branch` is kept iff `refs/heads/<name>` exists. A `Detached` entry is kept iff it is the current HEAD of its worktree. A `Commit` entry is always kept: it may be an active comparison key (Plan 05 base or head), and ref/HEAD checks say nothing about it. No age-based commit-key deletion ships in this plan: a free store lock between analysis subprocesses does not prove a comparison is inactive. Both automatic and explicit GC retain all `Commit` entries until a future comparison-lifetime lease contract can prove safe deletion.
  - CLI `gcx storage gc [--apply]`. The hook (`gcx hook --branch-switch`) runs it with `--apply` (no commit selector) when `{data_dir}/last_branch_gc` is older than 86 400 s.

- [ ] **Step 1: Write failing tests:** `deleted_branch_tables_dropped` (index main, a, b; delete b; gc; b's tables are gone and main/a queries are unchanged), `stale_detached_dropped_current_kept`, `commit_keys_survive_default_gc` (a full-SHA key with no ref and no worktree survives `gc --apply` and the hook GC), `commit_key_is_not_detached` (a commit key at the current detached HEAD's SHA is not matched by, or dropped with, the detached entry), `active_comparison_keys_survive_gc_between_subprocesses` (release the store lock between base indexing and head analysis, run explicit GC, then verify both keys survive), `gc_idempotent`, `gc_skips_when_locked`, `hook_gc_runs_at_most_daily`, `unregistered_tables_left_alone` (pre-registry keys are reported, not dropped).
- [ ] **Step 2: Run.** Expected: FAIL. **Step 3: Implement.** **Step 4: Run.** Expected: PASS. Hook timing with no GC due adds < 50 ms (`GCX_TIMING=1`).
- [ ] **Step 5: Commit.** `feat: garbage-collect graphs of deleted branches and detached snapshots`

### Task 4: Compaction (payload swap under a stable lock)

**Files:**
- Modify: `crates/gitcortex-store/src/kuzu/mod.rs` (compaction + open-time recovery), `crates/gitcortex-store/src/branch.rs` (payload path helpers), `crates/gitcortex-cli/src/cmd/storage.rs` (orchestration and fallback)
- Test: `crates/gitcortex-mcp/tests/compact.rs` (lives next to Plan 02's `tests/support` so it can reuse `fingerprint`; store-crate tests cannot import it)

**Interfaces:**
- Produces:
  - Consumes T1 `branch::payload_files` and `data_dir_inventory`. Produces `branch::staging_dir(repo_id, ts) -> PathBuf` (`{data_dir}/compact-<ts>/`, same filesystem so renames are atomic).
  - `enum CompactPhase { Exported, Imported, OldMovedAside, NewInPlace, Verified }`.
  - `KuzuGraphStore::compact(repo_root: &Path) -> Result<CompactReport { before: u64, after: u64 }>` = `compact_with(repo_root, None)`; `#[doc(hidden)] pub fn compact_with(repo_root, fail_after: Option<CompactPhase>)` for failure-injection tests. It acquires one `RepositoryLock` (never released until return), `EXPORT DATABASE` into the staging dir, `IMPORT DATABASE` into `staging/graph.kuzu`, drops both DB handles, renames the live payload to `graph.kuzu.pre-compact-<ts>{,.wal}`, renames the staged payload into place, reopens, and verifies (schema version, `fingerprint` per registered branch, registry keys present). Then it deletes the pre-compact payload and staging dir. Only payload files move; every sidecar listed in "Facts" stays untouched.
  - Open-time recovery in `open_with_schema_policy`: if `graph.kuzu` is missing and a `graph.kuzu.pre-compact-*` exists, restore it before opening (crash between renames).
  - Fallback if the spike fails (store stays indexer-free): `KuzuGraphStore::begin_rebuild(repo_root, &RepositoryLock) -> Result<StagedStore>` and `StagedStore::commit(self) -> Result<CompactReport>` (same payload swap + verify). `cmd/storage.rs` drives it, re-indexing retained branches with `IncrementalIndexer::run` (checked-out branch) and `IncrementalIndexer::run_tree` (others).
  - CLI: `gcx storage compact [--apply]`.

- [ ] **Step 1: Spike.** Confirm that the pinned `kuzu` crate supports `EXPORT/IMPORT DATABASE` with the per-branch rel tables. If it doesn't, implement the fallback above instead (CLI orchestration only) and note the choice in the PR.
- [ ] **Step 2: Write failing tests:**
  - `compact_reduces_size_after_branch_drop` (3 indexed branches, drop 2, compact → `after < before * 7 / 10`).
  - `compact_preserves_fingerprints` (uses Plan 02 `fingerprint`).
  - `compact_preserves_sidecars`: every T1 `Durable` inventory entry is byte-identical, `serve.lock` keeps its inode, and the inventory has no `Unknown` entry after success; after reopen, readiness for each branch (including a `Commit` key), registry, and schema version are unchanged.
  - `closed_snapshot_reopens_ready`: hold `RepositoryLock` with no open store, copy only `Payload` + `Durable` entries to a fresh data root, open there; every registered key's `inspect_commit`/fingerprint matches. Pins the closed snapshot contract that Plan 05 X2 consumes; GitCortex ships no snapshot command.
  - `compact_failure_restores_original`: one case per `CompactPhase` via `compact_with`; afterwards the store opens, fingerprints and sidecars match the original, and no staging dir or pre-compact payload remains.
  - `crash_between_renames_recovers_on_open`: simulate `OldMovedAside` then process exit; next `open` restores and passes fingerprints.
  - `open_refused_during_each_compact_phase`: a child process calling `gcx query lookup-symbol` while compaction is paused at each phase gets the lock-held error, never a partial DB.
- [ ] **Step 3: Run** `cargo test -p gitcortex-mcp --test compact`. Expected: FAIL. **Step 4: Implement.** **Step 5: Run.** Expected: PASS.
- [ ] **Step 6: Commit.** `feat: add gcx storage compact`

### Task 5: One store per repository across worktrees (ADR-0005)

**Files:**
- Create: `docs/adr/0005-shared-store-across-worktrees.md`, `crates/gitcortex-mcp/src/mcp/proxy_protocol.rs`
- Modify: `crates/gitcortex-store/src/branch.rs` (`storage_repo_id`, new `repository_id`, `worktree_id`, `detached_key`), `crates/gitcortex-mcp/src/mcp/server.rs` (`spawn_client`, `daemon_readiness`), `crates/gitcortex-mcp/src/mcp/watcher.rs`, `crates/gitcortex-mcp/src/mcp/helpers.rs` (branch resolver), `crates/gitcortex-cli/src/cmd/serve.rs` (proxy handshake; drop its private mode constants), `crates/gitcortex-cli/src/cmd/hook.rs` (replace `current_branch` with the shared resolver)
- Test: `crates/gitcortex-store/tests/worktree_identity.rs`, `crates/gitcortex-cli/tests/daemon_multiplex.rs`, `crates/gitcortex-cli/tests/detached_worktrees.rs`

**Interfaces:**
- Produces:
  - `branch::repository_id(repo_root: &Path) -> Result<String>` = blake3(canonicalized `git rev-parse --git-common-dir`) truncated to 16 hex characters. This bumps `IDENTITY_VERSION` to 2; migrated and new `repo.json` record it, so Plan 05 X2 checkpoints keyed on version 1 miss and rebuild instead of restoring under the new id.
  - Migration guard: a legacy dir is renamed to the new id only when it has no `repo.json` (legacy) or its recorded `worktree_root`/`git_common_dir` equal the opener's canonical paths. A dir whose recorded paths differ (for example a restored checkpoint from another checkout path) is never renamed, re-id'd, or merged; open reports the mismatch and the caller rebuilds.
  - `branch::worktree_id(repo_root: &Path) -> String` = blake3(canonical worktree root) truncated to 8 hex characters.
  - `branch::detached_key(worktree_id: &str, short_sha: &str) -> String` = `detached_<worktree_id>_<short_sha>` (pure; the only constructor).
  - `gitcortex_mcp::mcp::helpers::resolve_branch_key(repo_root: &Path) -> Result<String>`: symbolic ref, else `detached_key(...)`. Used by the MCP server, the watcher, the CLI hook (all of `gcx hook` and `--branch-switch`), and T3 GC keep policy.
  - `proxy_protocol` (single source for daemon, CLI proxy, and `daemon_readiness`): `enum ClientMode { Compact, Full }`, `struct Hello { gcx_proxy: u32 /*=2*/, mode: ClientMode, worktree_root: PathBuf }`, `write_hello(stream, &Hello)` (one JSON line `{"gcx_proxy":2,"mode":"compact","worktree_root":"<abs>"}\n`, then MCP bytes), `read_hello(stream) -> Result<Hello>`. A first byte of `0x00`/`0x01` (legacy mode byte) or an unknown version is rejected with a logged error naming the client version. A new proxy that gets EOF before the MCP `initialize` reply reports "the running daemon is an older gcx; stop it (see `daemon.log` for its pid) and reconnect" (an old daemon bails on mode byte `{`). No `--restart` flag exists today; do not add one here.
  - The daemon keeps `HashMap<PathBuf, WorktreeContext { watcher, branch }>` and resolves branch and readiness per connection; `daemon_readiness(repo_root)` sends `Hello { mode: Compact, worktree_root: repo_root }`.
  - Per-worktree runtime files live under `{data_dir}/worktrees/<worktree_id>/` (`daemon.log` stays repo-level).

- [ ] **Step 1: Write the ADR.** Cover the decision, alternatives rejected (per-worktree DBs; copy-on-first-index; branch-delta overlays deferred to backlog #6), the migration, the protocol bump, and risks. **Gate:** if the ADR finds Kuzu or daemon constraints that block a single owner, stop and report to the maintainer before Step 2.
- [ ] **Step 2: Write failing tests:**
  - Store: `main_and_linked_worktree_share_id`, `legacy_main_worktree_dir_migrates_by_rename`, `legacy_linked_worktree_dir_marked_superseded` (status `Unknown` → prunable via T2 with reason `superseded`), `gcx_store_path_still_honored`, `detached_keys_include_worktree`, `identity_version_bumped_with_common_dir_identity` (`IDENTITY_VERSION == 2`; migrated `repo.json` records 2), `mismatched_recorded_identity_is_not_migrated` (`repo.json` naming another path: dir untouched, open reports mismatch), `commit_keys_shared_across_worktrees` (a `Commit` key indexed from worktree A is `ready` via `inspect_commit` from worktree B; no `worktree_id` in its key).
  - `daemon_multiplex.rs`: `two_worktrees_answer_own_branch` (worktree A on `main`, B on `feat/x`, each `gcx serve` call without `branch` gets its own branch in `index.branch`), `handshake_carries_mode` (compact and full clients on one daemon see their own tool surfaces), `daemon_readiness_uses_handshake`, `legacy_mode_byte_rejected`, `new_handshake_against_old_daemon_reports_restart` (fake old daemon that bails on an unknown byte).
  - `detached_worktrees.rs`: `hook_then_mcp_lookup_in_two_detached_worktrees` (two worktrees detached at the same SHA with different dirty files; run `gcx hook` in each, then MCP `lookup_symbol` in each sees only its own dirty symbol and `index.branch` equals that worktree's `detached_key`).
- [ ] **Step 3: Run.** Expected: FAIL.
- [ ] **Step 4: Implement** identity, `detached_key`, `resolve_branch_key`, and migration first; switch `hook.rs` and `helpers.rs` to the resolver; run the store and detached tests; commit `feat: identify stores by repository, not worktree`. Then implement `proxy_protocol`, per-worktree contexts, and update every client (CLI proxy, `daemon_readiness`); run the daemon tests; commit `feat: one daemon serves all worktrees of a repository`.
- [ ] **Step 5: Measure.** Fixture: 1 repo, 3 worktrees, 5 branches, 10 detached checkouts. Record `gcx storage list --json` total bytes on `main` (before) and on this branch after `gcx storage gc --apply && gcx storage compact --apply` in `docs/benchmarks/storage.md`. This feeds release step R3.
- [ ] **Step 6: Docs.** Add a "Storage" section to REFERENCE (layout with the full T1 inventory and classes, the closed snapshot contract and `IDENTITY_VERSION`, commit keys vs detached keys, `gcx storage` commands, automatic GC, worktree sharing, proxy protocol v2 and stopping an old daemon after upgrade) and an upgrade note in CHANGELOG (identity version 2 invalidates external checkpoints).
- [ ] **Step 7: Commit.** `docs: document storage layout, GC, and worktree sharing`
