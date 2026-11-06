# PR Impact Action Implementation Plan

> Design only. Delivery follows Plan 07. Implement each task as a separate layer using the failing-test → run FAIL → implement signature → run PASS → commit checkboxes below. Do not create repositories, publish, or push while revising this plan.

**Goal:** Ship `bharath03-a/gitcortex-action`: one updated impact comment, an informational `GitCortex impact` check, identical job summary, and full impact graph JSON. Unknown evidence remains unknown. Stacked PRs show layer and cumulative impact.

**Architecture:** Rust owns commit indexing and typed impact JSON. A separate action repository owns installation, comparisons, checkpoints, validation, and the **only new comment renderer**, dependency-free Node modules. Read-only `pull_request` analysis uploads data; a privileged `workflow_run` publisher loads pinned trusted action code without a PR checkout. No competing `action/` directory in GitCortex.

**Predecessors:** [Plan 02](2026-11-05-02-trust-closure.md) T4 tree indexing, [Plan 04](2026-11-05-04-graph-export.md) T1/T2 graph schema, [Plan 03](2026-11-05-03-storage-optimization.md) storage identity contract, [Plan 07](2026-11-05-07-delivery-orchestration.md) delivery. Post-0.8.0 backlog; no release-gate expansion.

## Global Constraints

- `GitCortex:` paths are in this repo; `gitcortex-action:` paths are in the future separate repo. One task below = one layer, ≤~400 implementation/test lines excluding fixtures. Split again before implementation if the estimate exceeds that limit; no cross-repo commit. GitCortex E1=T1, E2=T2a, E3=T2b; action layers have their own X-series (specified at end). Every consumer pins merged/released dependency SHAs, never an open cross-repo branch.
- Core stays pure; indexer/store synchronous; use `GraphStore`, no new library `unwrap`/`expect`. No init/hooks/watcher/build/install of PR dependencies during analysis. Only the released gcx binary parses objects. Child CLI tests isolate `GCX_STORE_PATH`.
- Analyze exact PR head, never a synthetic merge commit. Resolve/fetch base tip, trunk tip and head once; compute `merge-base(baseTip,head)` separately for layer and cumulative. Pin both tips and merge bases. No common ancestor or ambiguous merge bases means unavailable comparison, not tip-to-head fallback. Unshallow/fetch bounded history first; report fetch exhaustion.
- Inputs use validated full SHAs and argv arrays, never shell interpolation. Fetch forks through base-repo PR refs; disable hooks, filters and persisted credentials. Bare/unborn checkout is supported by commit indexing/readiness.
- A ready zero says “No changed indexed symbols detected; static graph coverage is limited.” Missing/stale/indexing/unverified/failed/partial evidence has null counts/risk/graph. CLI queries never auto-index. Coverage/time/size failures never claim completeness.
- Full artifact retains uncapped reachable depth-2 base∪head graph (depth input 0–8). Presentation caps **each comparison** to 100 nodes, 500 edges, deterministic changed-first/hop/file/id order, no dangling edges; whole comment ≤50,000 UTF-8 bytes including metadata, links and omitted counts. Always reserve room for unavailable/truncation diagnostics.
- Analysis permissions: `contents: read`, `pull-requests: read`, cache/artifact runtime access; no secrets. Publisher only `actions: read`, `contents: read`, `pull-requests: write`, `checks: write`. No `pull_request_target` checkout. Publisher never opens an artifact database or imports downloaded code.
- Pin dependencies to verified full commit SHAs with version comments. All examples/tests use `runs-on: ubuntu-24.04`; initial installer supports only `x86_64-unknown-linux-gnu`. Exact release SHA256 detects corruption, **not authenticity independent of that release**. Unsupported platform/glibc errors name the tested runner and required release.
- Check is informational and never required by default. Complete same-repo evidence may be success; incomplete evidence neutral. For `head.repo.id != base.repo.id`, conclusion is **at most neutral** and comment/check/summary say **unverified fork analysis**, even if manifest claims success. A fork can edit its analysis workflow at the same path. Hashes and provenance cannot authenticate its claims.
- No tests are claimed executed by this design edit. During implementation record RED per new regression; already-green pinning tests (notably body-only edits) are baseline PASS, never fictitious RED. `just ci` gates Rust layers; Node suite/actionlint gate action layers.

## Review Focus

1. False seeds from shifted lines or missed edge/deleted evidence: T1 `insert_line_above_changes_only_edited_function`, `confidence_change_seeds_source`, `mixed_node_and_edge_changes_keep_both`, `removed_callee_keeps_base_callers`; pin `body_only_edit_is_changed` separately.
2. Checkout-dependent readiness or cache poisoning: T2a `bare_and_unborn_checkout_commit_ready`, T2b `unready_report_has_null_counts_and_exit_3`, T5 real-Kuzu `exact_base_hit_skips_run_tree`, `unchanged_lower_sha_is_reused`, `head_mutation_cannot_poison_base_checkpoint`, `open_database_is_not_saved`.
3. Forged fork artifacts/executable content: T7 `manifest_run_pull_head_must_match`, `forged_target_is_rejected`, `symlink_or_extra_file_rejected`; T8 `fork_success_is_neutral`; T9a `publish_import_graph_is_data_only`.
4. Retries/races overwrite current output: T8 `concurrent_first_create_converges_to_one`, `older_attempt_cannot_overwrite`, `pagination_finds_existing_marker`, `force_push_discards_old_run`.
5. Wrong stack range or missed migration: T4 `base_advance_keeps_branch_point`, `manual_stack_without_payload`, T9a `edited_without_base_change_skips`, T9b `deinit_removes_both_managed_files`, `edited_publisher_blocks_upgrade`.

**Pinning tests:** T1 body-only edit, existing text/JSON/github-comment goldens; T2b legacy JSON shape; T3 hostile labels and full graph unaffected by display caps; T9a parsed-YAML permission/runner/SHA pins; T9b user-owned workflow preservation. Run before changing behavior, retain through all layers.

### Verified evidence and external contracts

Source checked for this revision: `readiness.rs:28–104` requires statuses/HEAD; `graph.rs:57–159` derives equality including spans, definition bytes/body and edge line; `blast_radius.rs:141–170` gates edge seeds behind empty node seeds; `kuzu/mod.rs:1446–1520` uses those equalities and reads both keys in one DB. `branch.rs:56–59,205–216` hashes absolute repo paths and stores `.sha`/schema sidecars beside one DB. `main.rs:364–367` exits 1 for every error. `init/universal.rs:16–21` accepts a loose marker; `deinit.rs:366–369` removes only the old workflow. `dist-workspace.toml` maps Linux x64 to ubuntu-24.04. No compact gcx tool is available in this session; source inspection is authoritative here.

GitHub contracts: [workflow_run security](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_run), [native stack fields](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/optimizing-ci-for-stacked-pull-requests), [download-artifact inputs/extraction](https://github.com/actions/download-artifact), [check output limit](https://docs.github.com/en/rest/checks/runs#create-a-check-run), [comment API](https://docs.github.com/en/rest/issues/comments#create-an-issue-comment). Checked 2026-11-05: check summary/text document 65,535 characters; use the stricter 50,000-byte budget for all surfaces. The comment REST page does not specify a numeric body limit: pin an API sandbox acceptance at 50,000 bytes rather than asserting an unverified limit. No browser preview required for this design.

Native stack metadata is optional: explicit `trunk-ref` wins, then validated `stack.base.sha`, then bounded unique same-repo parent PR traversal (32, cycle detection). Never silently substitute repository default branch. Ordinary fork PRs use the same analysis path. Plain PRs with identical effective ranges compute once and label “this layer / cumulative”. Stack fields identify tips, not merge bases.

### Task 1 (E1): Semantic impact seeds and complete union

**Files:** Modify `GitCortex:crates/gitcortex-cli/src/cmd/blast_radius.rs`; create `GitCortex:crates/gitcortex-cli/src/cmd/blast_radius/report.rs`.

**Interfaces:** `collect_impact(store: &dyn GraphStore, base: &str, head: &str, depth: u8) -> anyhow::Result<ImpactReport>`; ready commit keys only. `ImpactReport { changed: Vec<Node>, affected: Vec<AffectedCaller>, nodes: Vec<Node>, edges: Vec<Edge>, evidence: BTreeMap<NodeId, ImpactNodeEvidence>, risk: Option<String> }`; `AffectedCaller { node: Node, hop: u8, risk: String }`; `ImpactNodeEvidence { changed: bool, hop: u8, presence: Base|Head|Both }`.

- [ ] Write regressions `insert_line_above_changes_only_edited_function` (edit f, shifting g and g's call lines: changed IDs exactly f), `pure_span_and_byte_shift_is_unchanged`, `confidence_change_seeds_source`, `mixed_node_and_edge_changes_keep_both`, `removed_callee_keeps_base_callers`, `cycles_keep_shortest_hop`, `depth_two_excludes_hop_three`. Baseline-pin `body_only_edit_is_changed` against real indexed trees; it may already PASS because body participates in equality.
- [ ] Run `cargo test -p gitcortex cmd::blast_radius`; record each new regression FAIL; baseline pins PASS.
- [ ] Implement `collect_impact` and private `node_changed(a: &Node,b: &Node) -> bool`, `edge_signature(e: &Edge) -> (NodeId,NodeId,EdgeKind,EdgeConfidence)`. Compare union snapshots locally; do not change global `branch_diff` equality. Ignore only span and definition start/end byte offsets; preserve body/signature/doc/semantic metadata differences. Edge comparison ignores line, aggregates confidence deterministically per (src,dst,kind); confidence changes seed source. Always include independently changed edge sources after semantic normalization. Preserve base-only nodes/edges and shortest-hop incoming Calls BFS; choose head node display when present. Risk thresholds unchanged, output no source text.
- [ ] Run same suite PASS and legacy output pins; `just ci`.
- [ ] Commit `fix: collect semantic impact seeds and base head evidence`.

### Task 2a (E2): Commit indexing and readiness

**Files:** Modify `GitCortex:crates/gitcortex-cli/src/cmd/index.rs`, `GitCortex:crates/gitcortex-cli/src/main.rs`; consume Plan 02 T4 `GitCortex:crates/gitcortex-mcp/src/mcp/readiness.rs`; create `GitCortex:crates/gitcortex-cli/tests/revision_index_cli.rs`.

**Interfaces:** Extend Plan 02's **same** `gcx index` with mutually exclusive `--branch NAME | --rev COMMIT`; no `--key` flag. `index_revision(root: &Path, rev: &str) -> anyhow::Result<()>` consumes Plan 02 T4 `resolve_commit(root, rev)` and `apply_tree_index(root, store, &sha, &sha)`, whose concrete indexer is `IncrementalIndexer::run_tree`. Consume Plan 02 T4 `inspect_commit(store: &dyn GraphStore,key: &str,sha: &str) -> IndexReadiness`: caller resolves full SHA; compare `last_indexed_sha(key)` to sha without root, HEAD, statuses or dirty checkout calls. Return missing/stale/unavailable/ready; dirty persisted overlay is unverified, never ready. Checkout fields None, `target_revision=sha`, `indexed_revision` actual; `working_tree_verified=false` means no checkout certification.

- [ ] Write `bare_and_unborn_checkout_commit_ready` (unborn checkout with fetched valid commit), `revision_index_does_not_checkout_or_install_hooks`, `wrong_sha_is_stale`, `dirty_commit_key_is_unverified`, `failed_index_never_certifies_sha`, `branch_and_rev_conflict`, `invalid_index_rev_exits_2`.
- [ ] Run `cargo test -p gitcortex --test revision_index_cli` and readiness unit tests; expected new cases FAIL.
- [ ] Implement signatures, sharing Plan 02's resolution/persistence path. Tree keys have `working_tree_dirty=false` only after successful complete apply and SHA publication; mark uncertain before mutation, leave unverified on failure. Immutable SHA keys never receive watcher overlays. Commit readiness checks persisted dirty marker, not worktree status. Consume Plan 04 D3's `CliExit`/`exit_code` for invalid revision exit 2; D3 is the sole owner and merges before E2. E3 also consumes it.
- [ ] Run same tests PASS plus Plan 02 tree/readiness regression suites; `just ci`.
- [ ] Commit `feat: index and inspect immutable commit keys`.

### Task 2b (E3): Impact JSON and shared exit codes

**Files:** Modify `GitCortex:crates/gitcortex-cli/src/main.rs`, `GitCortex:crates/gitcortex-cli/src/cmd/blast_radius.rs`, `GitCortex:crates/gitcortex-cli/src/cmd/blast_radius/report.rs`; consume Plan 04 D3 `GitCortex:crates/gitcortex-cli/src/exit.rs`; create `GitCortex:crates/gitcortex-cli/tests/blast_radius_cli.rs`, `GitCortex:docs/schemas/gitcortex-impact-v1.schema.json`.

**Interfaces:** Existing `gcx blast-radius --base REV --head REV --depth N --format text|github-comment|json` gains only `impact-json` and `--graph-out PATH`. No new Rust comment/Mermaid renderer. `index:{base:IndexReadiness,head:IndexReadiness}` uses `indexed_revision`/`target_revision`, not `scopes` or `indexed_sha`. Envelope `{format:"gitcortex.impact",schema_version:1,base_sha,head_sha,index,readiness,depth,changed_count,affected_count,risk,evidence:"lead",diagnostics,changed_ids,affected:[{id,hop,risk}],node_evidence,graph:<graph-v1>|null}`. `node_evidence` carries T1 presence/classes/hops; graph meta.scope remains export scope. `--graph-out` atomically writes same uncapped graph or unavailable envelope.

- [ ] Write `impact_json_validates_and_contains_removed_nodes`, `unready_report_has_null_counts_and_exit_3`, `wrong_indexed_sha_is_not_ready`, `invalid_blast_ref_exits_2`, `legacy_json_shape_unchanged`, `graph_out_is_uncapped`, `store_open_failure_emits_failed_json`.
- [ ] Run `cargo test -p gitcortex --test blast_radius_cli`; new contract tests FAIL; legacy pin PASS.
- [ ] Implement contract using Plan 02 T4 `inspect_commit` (also consumed by T2a) and Plan 04 T1 `ScopedGraph::from_explicit(head_sha, nodes, edges)` / T2 `to_graph_json`, with `seed=None` and untruncated union totals. Consume D3-owned shared `CliExit { InvalidInput, NotReady }` error + `exit_code(error: &anyhow::Error) -> i32` (downcast through context chain): invalid blast-radius/index ref and export seed/ref = 2; not ready = 3; other errors = 1. Main dispatch honors helper. Impact-json emits valid diagnostic before nonzero exit, including open failure (failed/1, matching D3's unrelated-error policy); clap argument errors remain 2. Coordinate with Plan 04 T3, no second main exit policy.
- [ ] Run same tests PASS, format/schema pins and `just ci`.
- [ ] Commit `feat: expose commit scoped impact JSON and exit codes`.

### Task 3 (action X7): One bounded trusted renderer

**Files:** Create `gitcortex-action:src/render.mjs`, `gitcortex-action:test/render.test.mjs`.

**Interfaces:** `renderImpact(analysis,{runUrl,artifactUrl,maxNodes=100,maxEdges=500,maxBytes=50000}) -> string`. Sole new comment renderer; analysis summary and trusted publisher call this module. Rust retains its legacy github-comment output only. JS consumes graph-v1 plus node_evidence directly: multiple changed seeds, changed/caller/deleted classes, hop, base∪head; no dependence on Plan 04's single-seed `to_mermaid`.

- [ ] Write `comment_caps_nodes_edges_and_utf8_bytes`, `many_seeds_count_omissions`, `multiple_seeds_and_deleted_nodes_have_classes`, `mermaid_has_only_present_endpoints`, `hostile_labels_cannot_close_fence_or_ping`, `unknown_never_says_no_impact`, `summary_matches_comment`. Include dense 100-node graph and multibyte labels; both comparisons share total text budget.
- [ ] Run `node --test test/render.test.mjs`; expected FAIL.
- [ ] Implement signature: caller→callee arrows, incoming traversal label, bounded labels/rows, escaped Markdown/HTML/fences/mentions, deterministic caps, visible omitted node/edge/text counts outside fence. Include readiness, tips/merge-base/head, risk lead, coverage and trusted URLs. Artifacts remain full graph. Fork label cannot be overridden by artifact.
- [ ] Run same tests PASS; sandbox post/check accepts 50,000-byte output, document returned API errors without claiming a universal undocumented comment limit.
- [ ] Commit `feat: render bounded trusted impact comments`.

### Task 4 (action X1): Resolve stack tips and merge bases

**Files:** Create `gitcortex-action:package.json`, `gitcortex-action:src/comparisons.mjs`, `gitcortex-action:test/comparisons.test.mjs`.

**Interfaces:** `resolveComparisons({event,github,trunkRef,resolveMergeBase}) -> Promise<ComparisonPlan>`; plan `{repositoryId,prNumber,headSha,parentTipSha,trunkTipSha,stack,comparisons:[{kind,baseTipSha,mergeBaseSha,headSha}],diagnostics}`. `comparisonKey({repositoryId,baseTipSha,mergeBaseSha,headSha,gcxVersion,indexSchema,depth}) -> string`. The injected git-object resolver returns a single full merge base; ambiguous/missing ancestor unavailable.

- [ ] Write `base_advance_keeps_branch_point` (base B→B2 after divergence: compare B→head, record B2 tip), `stack_parent_differs_from_trunk`, `manual_stack_without_payload`, `native_trunk_not_default_main`, `plain_pr_deduplicates_effective_range`, `ambiguous_parent_is_unknown`, `cycle_and_history_fetch_bounded`, `retarget_changes_comparison_key`.
- [ ] Run `node --test test/comparisons.test.mjs`; expected FAIL.
- [ ] Implement signatures and bounded fallback described above. Preserve both comparison labels when effective merge bases deduplicate; cache immutable base checkpoint by mergeBaseSha, full report by comparisonKey including tips.
- [ ] Run same suite PASS.
- [ ] Commit `feat: resolve layer and cumulative merge base comparisons`.

### Task 5 (action X2): Closed whole-directory base checkpoints

**Files:** Create `gitcortex-action:src/index-cache.mjs`, `gitcortex-action:test/index-cache.test.mjs`, `gitcortex-action:test/index-cache.integration.test.mjs`.

**Interfaces:** `indexCacheKey({repositoryId,mergeBaseSha,gcxVersion,indexSchema,identityVersion,platform,arch}) -> string`; exact restore only. `ensureComparisonStore({comparison,repoDir,storeRoot,cache,execFile,gcx}) -> Promise<{hit,scratchRoot,manifest}>`. Manifest binds repo ID, full merge base, gcx/schema/identity versions, actual absolute repo/common-dir identity and directory inventory/digests. Use fixed `/home/runner/work/_temp/gitcortex-analysis/repo.git` and isolated store roots on ubuntu-24.04; mismatch rebuilds. Do not depend on relocation being supported.

- [ ] Write unit regressions `wrong_sha_cache_rebuilds`, `schema_or_identity_mismatch_rebuilds`, `warm_base_race_is_success`; real released-gcx/Kuzu tests `exact_base_hit_skips_run_tree`, `unchanged_lower_sha_is_reused`, `head_mutation_cannot_poison_base_checkpoint`, `open_database_is_not_saved`, `missing_sha_or_schema_sidecar_rebuilds`. Integration assertions inspect actual output/readiness and index subprocess counts, not only fake exec.
- [ ] Run `node --test test/index-cache*.test.mjs` with E3 binary; new behavior FAIL, record each regression. Do not skip real tests in action CI.
- [ ] Implement signature. One **base-only** closed DB checkpoint per merge base; cache whole resolved repository data directory including graph.kuzu and any required graph.kuzu.wal payload, all `.sha`, schema_version and durable metadata/dirty state; exclude runtime locks/sockets/logs via explicit lifecycle inventory. Acquire exclusive ownership on the existing stable serve.lock inode (never rename/unlink the source lock; analysis may use Linux flock with a fixed trusted snapshot worker), wait for every gcx child to exit before snapshot, fail closed on lock/open owner. Restore checkpoint into private writable scratch root, then index head into **that same DB**. Never combine parent and trunk DBs by copying tables/files. Separate comparisons get separate scratch DBs, head indexed per comparison; drop “head indexed once”. Base==head reuses same key. Never save scratch/head under base key.
- [ ] Run suites PASS; exact base hit skips only base indexing, head still runs per distinct comparison. Ref-scoped cache misses rebuild; do not promise sibling PR reuse. Cache save “unable to reserve cache”/existing exact key after validated local snapshot is benign success; other failures emit diagnostics, not false hits. Warm job saves trusted tip as potential future merge base; no promise every PR hits it.
- [ ] Commit `perf: reuse closed base checkpoints with complete metadata`.

### Task 6a (action X3): Exact-release installer

**Files:** Create `gitcortex-action:src/install.mjs`, `gitcortex-action:test/install.test.mjs`.

**Interfaces:** `installGcx({version,platform,arch,destination,fetch}) -> Promise<string>`; exact tested release only, no latest/source fallback.

- [ ] Write `checksum_failure_stops_install`, `unsupported_target_is_actionable`, `glibc_failure_names_ubuntu_24_04`, `release_download_cannot_change_executable_path`.
- [ ] Run `node --test test/install.test.mjs`; expected FAIL.
- [ ] Implement signature, verify same-release SHA256 before safe extraction and executable use. Trust release/pinned downloader; describe checksum's corruption-only guarantee. Probe binary before analysis; glibc loader failure reports tested ubuntu-24.04, no silent compile fallback.
- [ ] Run same suite PASS and released binary smoke on ubuntu-24.04.
- [ ] Commit `feat: install exact tested gcx Linux release`.

### Task 6b (action X4): Analyze isolated comparisons

**Files:** Create `gitcortex-action:src/analyze.mjs`, `gitcortex-action:test/analyze.test.mjs`.

**Interfaces:** `analyze({plan,execFile,gcx,cache,outDir,limits}) -> Promise<AnalysisEnvelope>`. Files exactly `manifest.json`, `layer.impact.json`, `layer.graph.json`, optional cumulative equivalents. Manifest v1 binds repository/PR/head repo, head, parent/trunk tips, each merge base and comparison key, run ID/attempt, release/schema, readiness, lengths/SHA256s. Deduplicated/unavailable cumulative omitted with reason.

- [ ] Write `analysis_uses_objects_not_checkout_init`, `sha_inputs_never_become_shell`, `analysis_failure_still_writes_diagnostic`, `two_ranges_use_two_head_indexes`, `warm_base_never_caches_head`, `artifact_contains_full_not_display_graph`, `manifest_records_merge_bases`.
- [ ] Run `node --test test/analyze.test.mjs`; expected FAIL.
- [ ] Implement signature via T4/T5, exact merge-base→head `impact-json` per range; capture exit 3 diagnostics, limit depth/time/output bytes, never mark partial ready. Warm-base mode indexes only trusted default-branch push tip; no head. Write diagnostic envelope even if install/index fails at wrapper boundary.
- [ ] Run same suite PASS plus real-gcx offline schema smoke.
- [ ] Commit `feat: analyze immutable PR comparison stores`.

### Task 6c (action X5): Composite runtime and artifacts

**Files:** Create `gitcortex-action:action.yml`, `gitcortex-action:src/analyze-entry.mjs`, `gitcortex-action:src/publish-entry.mjs`, `gitcortex-action:.github/workflows/test.yml`, `gitcortex-action:test/action.test.mjs`.

**Interfaces:** Inputs `mode: analyze|publish|warm-base`, exact `gcx-version`, `depth:2` (0–8), `max-nodes:100` (1–100), optional trunk-ref, token, retention-days:14, publish run-id. Outputs readiness/artifact-name/report-path; name `gitcortex-impact-<run-id>-<attempt>`. No executable/path input. Pin setup-node to Node 24 for analysis; github-script has its own pinned bundled runtime, uses separate conditional steps: publisher imports fixed `pathToFileURL(process.env.GITHUB_ACTION_PATH + '/src/publish-entry.mjs')`; analysis/warm-base imports fixed `'/src/analyze-entry.mjs'` under that same trusted action root. This single fixed bootstrap import is the audited exception to the publisher dynamic-code prohibition; no computed artifact/input import.

- [ ] Write `bootstrap_loads_action_path_not_workspace`, `failure_uploads_diagnostic_always`, `inputs_are_bounded`, `publisher_never_installs_gcx_or_restores_cache`.
- [ ] Run `node --test test/action.test.mjs`; actionlint new action/workflow; expected FAIL.
- [ ] Implement wrapper signatures and pinned upload/cache/download actions. Analysis has 15-minute timeout; upload diagnostic under always(). Test CI runs Node/actionlint and T5 real-gcx integration with E3 release pin.
- [ ] Run same suite PASS and offline end-to-end fixture.
- [ ] Commit `feat: wire composite action runtime and diagnostic artifacts`.

### Task 7 (action X6): Data-only download and API identity

**Files:** Create `gitcortex-action:src/validate-artifact.mjs`, `gitcortex-action:test/validate-artifact.test.mjs`; modify `gitcortex-action:action.yml`.

**Interfaces:** `validateRun({github,workflowRun,expectedWorkflowPath}) -> Promise<ValidatedRun|null>`; `readArtifactDirectory(dir,limits) -> Promise<DirectoryEntry[]>`; `validateArtifact({entries,manifest,validatedRun,currentPr}) -> ValidatedAnalysis`. **entries is a readdir/lstat directory listing**, never ZIP members. Use pinned actions/download-artifact with verified artifact ID, repository, triggering run-id and token into a fresh temp directory; no custom dependency-free ZIP parser. Preflight API artifact size ≤50 MiB, validate extracted total ≤50 MiB/5 files, each JSON ≤16 MiB, graph ≤100k nodes/500k edges, JSON depth ≤32; reject nested paths, symlinks, nonregular/unknown files before read. Downloader extraction is a trusted pinned dependency; post-extraction limits are not advertised as pre-extraction bomb protection, job timeout/disk isolation remain necessary.

- [ ] Write `forged_target_is_rejected`, `empty_run_prs_resolves_unique_fork`, `ambiguous_candidate_skips`, `manifest_run_pull_head_must_match`, `wrong_head_repo_rejected`, `wrong_workflow_or_attempt_rejected`, `symlink_or_extra_file_rejected`, `oversized_or_unknown_schema_rejected`, `hash_mismatch_rejected`.
- [ ] Run `node --test test/validate-artifact.test.mjs`; expected FAIL.
- [ ] Implement signatures: verify base repo/run/workflow ID/path/event=pull_request and artifact run/attempt first. Empty run PR array: discover bounded candidates by commit-associated PR API, then **GET /pulls/{n}** for unique open current head.sha + head.repo.id + base.repo.id match. Never select PR from manifest alone; no check-suite fallback. Require `manifest.head == workflow_run.head_sha == pull.head.sha`; synthetic/other SHA mismatch skips safely with diagnostic. Validate tips/merge bases against trusted API/object plan; artifact hashes only corruption evidence. Missing/invalid artifact for unique trusted target synthesizes unknown neutral report. Ambiguous identity skips writes.
- [ ] Run same suite PASS; publisher opens no database, spawns no subprocess, trusts no artifact Markdown/module.
- [ ] Commit `fix: validate downloaded data against current PR identity`.

### Task 8 (action X8): Convergent sticky publication

**Files:** Create `gitcortex-action:src/publish.mjs`, `gitcortex-action:test/publish.test.mjs`; modify `gitcortex-action:src/publish-entry.mjs`, `gitcortex-action:action.yml`.

**Interfaces:** `publish({github,context,analysis,writeSummary}) -> Promise<{commentId,checkId}>`. Exact marker `<!-- gitcortex:impact:v1 -->`, expected Actions bot author only. Check name `GitCortex impact`, actual PR head SHA, external_id `gitcortex-impact:<repo>:<pr>:<head>`; list/update own check. Use T3 sole renderer, trusted API URLs, hidden run/attempt ordering metadata.

- [ ] Write `concurrent_first_create_converges_to_one` with interleaved empty listings/two creates, `pagination_finds_existing_marker`, `human_marker_is_not_owned`, `rerun_updates_same_check`, `older_attempt_cannot_overwrite`, `force_push_discards_old_run`, `fork_success_is_neutral`, `failed_analysis_publishes_unknown_neutral`, `write_denied_keeps_summary`.
- [ ] Run `node --test test/publish.test.mjs`; expected FAIL.
- [ ] Implement signature. Workflow concurrency **always** groups `workflow_run.head_repository.id + workflow_run.head_branch` with cancel-in-progress false, independent of empty pull_requests. Re-fetch current PR tips/head and run ordering before writes. Paginate bot markers, choose oldest by created_at/id. After create re-list, retain oldest, delete only own later duplicates, update retained comment only if candidate metadata is newer; bounded retries converge or report retryable failure. Losing creator must not overwrite newer winner. Clean existing duplicates similarly. Same render goes to comment/check summary/job summary. Forks always labelled unverified and neutral regardless of artifact claims. API write denial retains summary/artifact with explicit diagnostic.
- [ ] Run same suite PASS with interleaving mocks; sandbox retry/out-of-order fork acceptance.
- [ ] Commit `feat: publish convergent informational impact checks`.

### Task 9a (action X9): Consumer workflows and action docs

**Files:** Create `gitcortex-action:examples/gcx-impact.yml`, `gitcortex-action:examples/gcx-impact-publish.yml`, `gitcortex-action:test/workflows.test.mjs`, `gitcortex-action:README.md`; modify `gitcortex-action:package.json` for a pinned test-only YAML parser.

**Interfaces:** Analysis `pull_request` opened/synchronize/reopened/edited/ready_for_review, no branch/path filter; job guard `action != 'edited' || github.event.changes.base != null`. Default-branch push warm-base separate guard/job. Publisher workflow_run completed for `GitCortex Impact Analysis`, no success-only filter, **no checkout**, same T8 fork-safe concurrency group. Both ubuntu-24.04 with explicit permissions/timeouts/retention; publisher lives on default branch. Use released action SHA and gcx E3 release, not placeholders at shipping time.

- [ ] Write parsed-YAML `fork_has_read_only_permissions` (moved here from analyze tests), `generated_pair_uses_workflow_run_without_checkout`, `edited_without_base_change_skips`, `every_pr_layer_runs`, `publisher_concurrency_handles_empty_prs`, `workflow_permissions_are_separated`, `all_dependencies_and_runner_are_pinned`. Static `publish_import_graph_is_data_only`: recursively audit publisher entry/validate/render/publish imports; forbid child_process, eval/Function/vm, nonliteral imports except exact T6c bootstrap; publish-entry must not transitively import installer/analyzer/cache; separate fixed entry modules are required.
- [ ] Run `node --test test/workflows.test.mjs`; actionlint examples and CI; expected FAIL.
- [ ] Implement workflows and docs: two workflows on default branch, fork approval, informational neutral fork results, release trust/checksum caveat, cache ref scope/trunk warming limits, names/paths privacy, retention, stack override and coverage. No branch-protection enrollment.
- [ ] Run suite PASS, all Node tests/actionlint; sandbox same-repo/fork/failure/>100-node/native/manual-stack/rebase/retarget/retry acceptance. Record actual URLs only after execution.
- [ ] Commit in action repo only `docs: ship pinned analysis and publisher workflows`.

### Task 9b (GitCortex E4): Atomic managed workflow migration and deinit

**Files:** Create `GitCortex:crates/gitcortex-cli/src/cmd/init/ci.rs`; modify `GitCortex:crates/gitcortex-cli/src/cmd/init/universal.rs`, `GitCortex:crates/gitcortex-cli/src/cmd/init/mod.rs`, `GitCortex:crates/gitcortex-cli/src/cmd/deinit.rs` and their unit tests.

**Interfaces:** Preserve `write_ci_workflow(repo_root: &Path) -> anyhow::Result<()>`; destinations `.github/workflows/gcx-blast-radius.yml` and `gcx-impact-publish.yml`. New markers `# GitCortex managed impact analysis v1` / `# GitCortex managed impact publisher v1` plus canonical full-content fingerprint (excluding fingerprint line); marker alone does not authorize replacement/removal. Recognize exact known legacy generated templates. `is_managed_ci_workflow` shares ownership checks for upgrade/deinit; modified publisher blocks **both** replacements, even with marker retained. Preflight both; stage/rollback writes on failure; report both paths.

- [ ] Write `legacy_managed_workflow_upgrades_both_files`, `edited_publisher_blocks_upgrade`, `custom_publisher_prevents_partial_upgrade`, `marker_alone_is_not_ownership`, `pair_write_failure_rolls_back`, `deinit_removes_both_managed_files`, `deinit_preserves_user_edited_publisher`, `deinit_dry_run_lists_both`.
- [ ] Run `cargo test -p gitcortex cmd::init` and `cargo test -p gitcortex cmd::deinit`; expected new cases FAIL.
- [ ] Implement signatures with templates pinned to action X9 merged SHA + gcx E3 release; reuse ownership verifier in deinit; do not broaden legacy substring matching. Tests pin generated YAML contract to action examples.
- [ ] Run same suites PASS; `just ci`.
- [ ] Commit in GitCortex only `feat: migrate and remove managed impact workflow pair`.

### Task 9c (GitCortex E5): CLI rollout docs

**Files:** Create `GitCortex:docs/PR-IMPACT-ACTION.md`; modify `GitCortex:README.md`, `GitCortex:docs/REFERENCE.md`.

**Interfaces:** Document E2/E3 CLI flags and exits, pair migration/ownership, pinned action release, merge-base semantics, separate action-repo release ownership and informational fork limits.

- [ ] Write doc/link assertions `impact_examples_use_supported_release`, `impact_cli_flags_match_help`; initially FAIL on missing guide.
- [ ] Run docs contract check and `typos docs/`; record FAIL for missing guide.
- [ ] Implement docs using shipped action X9/E4 pins, exact ubuntu-24.04 examples and acceptance matrix, no invented run URLs.
- [ ] Run same checks PASS and links check if available.
- [ ] Commit `docs: explain PR impact rollout and trust limits`.

### Task 10 (action X10, later optional): Explicit gates

**Files:** Create `gitcortex-action:src/policy.mjs`, `gitcortex-action:test/policy.test.mjs`; modify `gitcortex-action:action.yml`, `gitcortex-action:src/publish.mjs`, `gitcortex-action:README.md`.

**Interfaces:** `evaluatePolicy(analysis,{failOn,maxAffected,gateScope}) -> {conclusion,reasons}`; trusted publisher inputs only, defaults empty gates/layer. Public API change heuristic = changed explicitly public declarations, not compatibility proof. Fork trust cap still applies, gates cannot elevate unverified results.

- [ ] Write `default_high_impact_is_success`, `threshold_uses_uncapped_count`, `unknown_is_neutral`, `artifact_cannot_control_policy`, `missing_cumulative_never_falls_back`, `fork_policy_cannot_claim_verified_result`.
- [ ] Run `node --test test/policy.test.mjs`; expected FAIL.
- [ ] Implement signature for explicitly enabled evaluable gates; baseline never auto-enrolls rulesets or advertises unavailable gate inputs.
- [ ] Run same suite PASS plus default acceptance/actionlint.
- [ ] Commit `feat: add explicit impact policy gates`.

## Required changes to other plans

- **Plan 02 T4 — shared revision path/readiness:** retain `gcx index --branch`; share resolver/apply with Plan 05 T2a `--rev COMMIT` (full SHA key, no --key). Use concrete `IncrementalIndexer::run_tree`. In `readiness.rs` add shared `inspect_commit(store,key,sha)` without checkout/status probes; preserve existing checkout `inspect`. Publish dirty=false and indexed SHA only after complete tree apply; failed mutation remains unverified. Pin bare/unborn-fetched commit tests and coordinate readiness edits before E2.
- **Plan 03 T1/T3/T4/T5 — checkpoint lifecycle and identity:** document entire resolved data directory inventory (graph.kuzu plus required WAL via T4 payload_files, `.sha`, `.dirty`, schema_version, repo/branch metadata; runtime lock/socket/log exclusion), closed exclusive snapshot under the existing stable serve.lock inode and identity version. Commit keys are immutable tree snapshots, distinguish from worktree detached keys in registry/GC; GC must not remove active comparison keys. C5 common-dir identity migration bumps cache identity version; Plan 05 rejects path/common-dir mismatch and rebuilds, never merges checkpoint DBs or silently relocates stores.
- **Plan 04 T1/T2 — graph adaptation boundary:** retain existing single-seed ScopedGraph/to_mermaid APIs for exports; expose construction/serialization from explicit nodes+edges with base-only IDs and no live-store re-resolution. Plan 05 T2b adapter supplies union nodes/edges and uses head SHA as branch label, `seed=None`, untruncated totals; multi-seed changed/caller/deleted classes, hop and presence live in impact envelope `node_evidence`, rendered only by Plan 05 T3 JS. `meta.scope` remains ExportScope, never readiness. Add `explicit_union_preserves_base_only_nodes` schema pin; no second Rust impact-comment renderer.
- **Plan 04 T3 — shared exit helper:** D3 owns `CliExit`/`exit_code` and main wiring; Plan 05 E2/E3 consume that merged helper instead of creating a parallel exit policy. Export ambiguous/missing seed or invalid ref = 2; not ready = 3; unrelated errors = 1. Preserve contextual-error downcast test and existing CLI pins.
- **Plan 07 Stack E / stack map / per-layer gates — separate repositories:** GitCortex E1=T1 semantic collector → E2=T2a revision readiness (consumes D3 shared exit helper) → E3=T2b impact JSON; E2 requires merged B4 and D3 SHAs, E3 requires merged D2 explicit-union schema SHA and E1/E2, plus shared D3 exit agreement (not a circular implementation dependency). Record E1/E2/E3 merged SHAs and release containing all three. Separate `gitcortex-action` stack X1=T4 comparisons → X2=T5 cache → X3=T6a install → X4=T6b analyze → X5=T6c composite → X6=T7 validate → X7=T3 render → X8=T8 publish → X9=T9a workflows/docs; X10=T10 optional later. Action X2 real-Kuzu CI pins E3 release and its E1–E3 SHAs; X3/X9 use same exact release. E4=T9b migration starts only after action X9 merged/released SHA exists; E5=T9c docs follows E4. Separate repository creation/first push approvals and local Node/actionlint/real-gcx gates; no cross-repo PR base or mixed-repo commit. D4 is optional export functionality, not an impact renderer prerequisite. Split any layer exceeding ~400 lines before dispatch; record resulting SHA edges in ledger.
