# GitCortex PR 5 and Combined Release Execution Plan

> **For the implementer:** Execute this plan task-by-task with strict RED → GREEN → REFACTOR for code changes. Do not publish or tag until PR 5 is merged and every release gate below passes.

**Goal:** Finish the five-PR R1 release train with one final validation/release-preparation PR, then publish one combined GitCortex release backed by reproducible correctness, retrieval, client, installation, and competitor evidence.

**Architecture:** PR 5 must not add another broad product area. It validates and packages the behavior delivered by PRs 1–4: reliable branch-aware indexing, typed read-only question planning, bounded progressive evidence, safe PR impact analysis, and reversible adoption. Benchmark and release automation should consume pinned manifests and emit provenance-rich machine-readable output before any public claim is changed.

**Tech stack:** Rust workspace, KuzuDB, tree-sitter, Python benchmark harness, React/Vite visualization, GitHub Actions/cargo-dist, npm, PyPI, crates.io, Homebrew.

---

## 1. Release-train status and product boundary

### Completed PRs

| PR | Outcome | Merge commit |
|---|---|---|
| [#77](https://github.com/bharath03-a/GitCortex/pull/77) | Reliable and secure indexing | `6468fd0` |
| [#80](https://github.com/bharath03-a/GitCortex/pull/80) | Typed question planning and precise search | `2721fd5` |
| [#82](https://github.com/bharath03-a/GitCortex/pull/82) | Token-efficient progressive exploration | `4f04249` |
| [#85](https://github.com/bharath03-a/GitCortex/pull/85) | Safe adoption and PR workflow | `95a5421` |

### Remaining delivery shape

- Exactly one implementation PR remains: **PR 5 — combined validation and release preparation**.
- PR 5 merges into `main` before any tag is created.
- One combined release is published after PR 5 merges.
- Do not create extra feature PRs for PR-5 findings. Fix release blockers as granular commits inside PR 5 unless a security emergency requires otherwise.

### Product north star

> GitCortex is a local-first, branch-aware code knowledge graph that helps developers and coding agents find the right code, understand change impact, and answer accurately with less exploration and total token usage.

### Explicit non-goals

- Generic document or enterprise knowledge platform work.
- Hosted source-code ingestion.
- Adding languages merely to increase a count.
- Arbitrary LLM-generated Cypher, SQL, shell, or database queries.
- Public “universally better” claims based on a narrow or unpinned benchmark.
- Redesigning visualization or unrelated developer tooling during the release gate.

---

## 2. Prerequisites

Install or verify:

```bash
git --version
gh auth status
rustc --version
cargo --version
cargo audit --version
node --version
npm --version
python3 --version
dist --version
jq --version
```

Publishing later requires credentials or trusted publishing configuration for:

- GitHub Releases
- npm
- PyPI
- crates.io (`CARGO_REGISTRY_TOKEN` or `cargo login`)
- Homebrew tap (`HOMEBREW_TAP_TOKEN` and the tap repository described in `docs/HOMEBREW-RELEASE.md`)

Do not print credentials into logs or commit them. Stop before publishing if any required channel is not configured.

---

## 3. Start PR 5 from merged main

### Step 1: verify PR 4 and update main

```bash
cd "/Users/bharathvelamala/Documents/Open Source/GitCortex"
gh pr view 85 --json state,mergedAt,mergeCommit,url
git fetch origin
git checkout main
git pull --ff-only origin main
git status --short --branch
git log -5 --oneline
```

Expected:

- PR #85 is `MERGED`.
- `main` contains merge commit `95a5421` or its verified successor.
- Worktree is clean.

### Step 2: create the final branch

```bash
git checkout -b release/r1-combined-validation
git status --short --branch
```

Do not branch from the old PR-4 local branch.

---

## 4. Resolve the version-policy conflict before changing versions

Current workspace version: `0.7.6` in `Cargo.toml`.

`RELEASING.md` currently says both:

1. schema or public CLI/MCP surface changes require a minor bump; and
2. pre-traction releases must remain patch releases.

This release contains a schema evolution and public query/planning behavior. Do not silently choose a version that violates one of those rules.

### Required decision

Use one of these paths and record the reason in PR 5:

- **Recommended compatibility-first path:** release `0.8.0`, and amend the pre-traction rule so required schema/public-contract minor bumps are allowed even before the marketing traction threshold.
- **Patch path:** release `0.7.7` only if the repository owner explicitly decides the additive surface and automatic re-index are patch-compatible, then update `RELEASING.md` to remove the contradiction.

Set the decision once:

```bash
export NEW_VERSION="0.8.0"   # recommended; replace only after explicit policy decision
```

Acceptance gate:

- `RELEASING.md` has one unambiguous version rule.
- Release notes explicitly state whether users receive a one-time graph re-index.
- No version file is changed before this decision is documented.

Suggested commit:

```bash
git add RELEASING.md
git commit -m "docs: resolve release version policy"
```

---

## 5. Task A — Add a machine-checkable combined release gate

**Objective:** Make release readiness reproducible instead of relying on a prose checklist.

**Likely files:**

- Create: `scripts/r1_release_gate.py`
- Create: `scripts/test_r1_release_gate.py`
- Create: `docs/benchmarks/R1-RELEASE-GATE.md`
- Modify: `RELEASING.md`

### TDD sequence

1. Write tests that fail when:
   - required benchmark result files are absent;
   - result provenance lacks binary hash, suite hash, repository commit, client/model, rounds, or cache state;
   - a retrieval task is invalid;
   - a declared public claim lacks a matching result identifier;
   - workspace/package versions disagree;
   - the worktree is dirty at the release-candidate check.
2. Run:

```bash
python3 -m unittest scripts/test_r1_release_gate.py -v
```

Expected: RED for missing implementation.

3. Implement the smallest gate that reads committed manifests/results and exits non-zero with actionable messages.
4. Re-run the focused tests until GREEN.
5. Run the gate against an intentionally incomplete fixture and verify non-zero exit.
6. Document exact inputs and outputs in `docs/benchmarks/R1-RELEASE-GATE.md`.

Suggested commit:

```bash
git add scripts/r1_release_gate.py scripts/test_r1_release_gate.py \
  docs/benchmarks/R1-RELEASE-GATE.md RELEASING.md
git commit -m "test: add combined R1 release gate"
```

---

## 6. Task B — Freeze fair competitor evaluation inputs

**Objective:** Compare GitCortex with Graphify and CodeGraph on shared workflows without rewarding unsupported breadth or penalizing missing unrelated features.

### Competitors

- Graphify: `https://github.com/Graphify-Labs/graphify`
- CodeGraph: `https://github.com/colbymchenry/codegraph`

### Pinning procedure

For each competitor:

```bash
git clone <canonical-url> /tmp/<competitor>-r1
cd /tmp/<competitor>-r1
git fetch --tags --force
git checkout <chosen-release-tag-or-commit>
git rev-parse HEAD
```

Record in a committed manifest:

- canonical repository URL;
- exact 40-character commit;
- release/tag if applicable;
- install command from that pinned revision’s own documentation;
- runtime and dependency versions;
- exact supported workflow mapping;
- unsupported workflows marked `unsupported`, never silently mapped to a weaker task.

**Likely files:**

- Create: `tools/agent-bench/competitors.toml`
- Create: `tools/agent-bench/competitor_run.py`
- Create: `tools/agent-bench/test_competitor_run.py`

### Shared workflows

Use held-out tasks covering the core product boundary:

1. exact symbol lookup with duplicate short names;
2. callers/callees and qualified identity;
3. implementation/type-usage lookup;
4. incremental freshness after an edit;
5. branch-aware change impact;
6. repository-question routing or the competitor’s closest documented equivalent;
7. total-session exploration cost on the same final-answer task.

### Fairness rules

- Same repository commits and task text.
- Same machine class and clean checkout procedure.
- Recommended setup for each product.
- Three or more repetitions per condition.
- Cold and warm runs reported separately.
- Correctness and required evidence scored before latency/tokens.
- Unsupported is reported, not converted into a fabricated zero or fallback result.
- Failures and losses remain in the report.
- Do not use Graphify’s generic document ingestion as a required GitCortex workflow.
- Do not claim GitCortex is globally better based on these focused workflows.

### TDD sequence

1. Write parser/manifest tests before implementing adapters.
2. Add one smoke task per competitor.
3. Verify every trace records provenance.
4. Add timeout and output-size bounds.
5. Ensure failed competitor commands are retained as failures, not discarded.

Suggested commit:

```bash
git add tools/agent-bench/competitors.toml \
  tools/agent-bench/competitor_run.py \
  tools/agent-bench/test_competitor_run.py
git commit -m "test: add pinned competitor evaluation lane"
```

---

## 7. Task C — Run deterministic GitCortex validation

Build the exact candidate binary:

```bash
cargo build --release --locked --bin gcx
sha256sum target/release/gcx 2>/dev/null || shasum -a 256 target/release/gcx
target/release/gcx --version
```

Run harness tests:

```bash
cd tools/agent-bench
python3 -m unittest discover -p "test_*.py"
cd ../..
```

Run the full pinned retrieval suite:

```bash
python3 tools/agent-bench/bench.py run \
  --gcx target/release/gcx \
  --label r1-candidate
```

Replay the result:

```bash
python3 tools/agent-bench/bench.py replay \
  tools/agent-bench/results/<r1-candidate-result>.jsonl
```

Compare with the last accepted release baseline:

```bash
python3 tools/agent-bench/bench.py compare \
  tools/agent-bench/results/<previous-release>.jsonl \
  tools/agent-bench/results/<r1-candidate-result>.jsonl
```

Required retrieval gates:

- 20/20 pinned tasks valid, unless the committed suite has intentionally changed with reviewed ground truth.
- No required evidence regression.
- `relevance_non_inferior` passes.
- Every payload remains inside its configured budget.
- Exact binary/suite/repository hashes are present.
- Search pagination does not duplicate or omit evidence across offsets.
- Qualified duplicate-name tasks remain correct.

Do not overwrite historical reports. Use a new result label.

---

## 8. Task D — Run native-client and total-session evaluation

Use the same pinned suite and alternate baseline/graph arm order.

### Codex graph-CLI lane

```bash
python3 tools/agent-bench/agent_run.py \
  --gcx target/release/gcx \
  --client codex \
  --model gpt-5.4-mini \
  --reasoning low \
  --label r1-codex \
  --rounds 3
```

Label this lane `codex-graph-cli`, not MCP, unless the client actually exposes the MCP tool during the run.

### Claude Code MCP lane

```bash
python3 tools/agent-bench/agent_run.py \
  --client claude \
  --gcx target/release/gcx \
  --model haiku \
  --reasoning low \
  --label r1-claude \
  --rounds 3
```

### Agy MCP lane, only if configured and available

```bash
python3 tools/agent-bench/agent_run.py \
  --client agy \
  --gcx target/release/gcx \
  --model "Gemini 3.6 Flash (Low)" \
  --reasoning low \
  --label r1-agy \
  --rounds 3
```

Required reporting:

- Correctness and required final-answer evidence.
- Tool calls, follow-up reads, turns, elapsed time.
- Client-reported total tokens and cache accounting.
- Cold/warm state.
- Unsupported client capability explicitly marked.
- Per-task wins and losses; no aggregate claim that hides invalid tasks.

Stop and investigate if a graph arm is cheaper only because it produced a worse or incomplete answer.

---

## 9. Task E — Run Graphify and CodeGraph evaluation

After their pinned smoke tasks pass:

```bash
python3 tools/agent-bench/competitor_run.py \
  --manifest tools/agent-bench/competitors.toml \
  --competitor graphify \
  --rounds 3 \
  --label r1-graphify

python3 tools/agent-bench/competitor_run.py \
  --manifest tools/agent-bench/competitors.toml \
  --competitor codegraph \
  --rounds 3 \
  --label r1-codegraph
```

Generate a report:

- Create: `docs/benchmarks/r1-competitor-evaluation.md`
- Include raw-result paths and hashes.
- Include environment, versions, commits, task definitions, repetitions, and cache state.
- Include unsupported workflows and failures.
- Separate these dimensions:
  - indexing freshness;
  - graph/identity correctness;
  - question-to-query behavior;
  - branch/PR impact;
  - evidence precision and coverage;
  - total-session tokens/tool calls;
  - setup and rollback experience.

Public claim gate:

- Claims may say “won X of Y valid tasks on this pinned suite.”
- Claims may say “used fewer exploratory reads on these named workflows.”
- Claims must not say “always better,” “universally more accurate,” or similar.
- If GitCortex loses a core workflow, either fix and rerun the full affected lane or publish the loss plainly.

Suggested commit after results are final:

```bash
git add docs/benchmarks/r1-competitor-evaluation.md \
  tools/agent-bench/results/<approved-result-files>
git commit -m "docs: publish pinned R1 evaluation"
```

If raw traces are intentionally ignored because they contain large or sensitive client output, commit a checksummed summary plus exact local paths and preserve raw traces in the release artifact/archive instead.

---

## 10. Task F — Compatibility, migration, install, and rollback tests

### Schema/re-index behavior

Verify upgrade from the last published version using a disposable repository and disposable GitCortex data/cache paths.

Required cases:

- old graph opens with actionable migration/re-index behavior;
- explicit `gcx init` performs the permitted rebuild;
- ordinary query does not destructively wipe an incompatible graph;
- hooks remain non-blocking;
- stable branch identities remain collision-safe;
- semantic index v2/stale formats fail closed and rebuild to v3;
- `gcx deinit --dry-run` previews only managed files;
- user-owned same-path workflow is preserved.

### Package/install smoke matrix

Build/package locally without publishing:

```bash
cargo package --workspace --allow-dirty
npm --prefix viz ci
npm --prefix viz run lint
npm --prefix viz test
npm --prefix viz run build
npm pack --prefix npm/packages/gitcortex
python3 -m build python
```

Use fresh temporary environments to install produced artifacts, then run:

```bash
gcx --version
gcx init --editor none
gcx doctor
gcx query search main --limit 5
gcx deinit --dry-run
gcx deinit --purge
```

### Real release-binary E2E

Follow `RELEASING.md`, but do not use `echo`, `sed`, or unreviewed mass replacements when editing repository files. Use targeted edits and inspect diffs.

Acceptance:

- install succeeds from every locally built package;
- version is identical on all package surfaces;
- a small Rust fixture indexes and queries successfully;
- diagnostics return zero only for healthy setup;
- rollback leaves unrelated files untouched.

---

## 11. Task G — Reconcile documentation and public claims

Audit all numerical claims:

```bash
rg -n "[0-9]+(\.[0-9]+)?×|[0-9]+(\.[0-9]+)?%|cheaper|faster|token|better" \
  README.md docs npm python viz
```

For every claim, require:

- suite name;
- GitCortex release/candidate;
- client/model;
- rounds;
- cache state;
- repository set;
- result/report link.

Known historical values must not be blended:

- 1.06× geomean / 7.7% aggregate;
- 1.35× / 23.3%;
- 1.82× Codex uncached smoke result;
- 3.27× Codex, 3.26× Claude, 1.89× Agy/Antigravity-era result.

Keep a historical claim only when clearly labeled with its original suite. Remove or rewrite ambiguous hero claims.

Files likely to change:

- `README.md`
- `docs/benchmarks/*.md`
- `docs/DEMO_SCRIPT.md`
- `docs/ADOPTION.md`
- product-page source under `viz/` if it contains release claims
- npm/Python package READMEs

Suggested commit:

```bash
git add README.md docs viz npm python
git commit -m "docs: align release claims with pinned evidence"
```

---

## 12. Task H — Version bump, changelog, and release notes

After all benchmark/compatibility gates pass, update every version surface.

Likely files:

- `Cargo.toml`
- `Cargo.lock`
- `npm/packages/*/package.json`
- `python/src/gitcortex/__init__.py`
- `README.md` example output
- any package lockfiles containing the package version

Use targeted edits. Then verify:

```bash
rg -n '0\.7\.6|"version"|__version__' \
  Cargo.toml Cargo.lock npm/packages python/src README.md
cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | [.name,.version] | @tsv'
```

Update `CHANGELOG.md` with:

- reliable incremental indexing and branch identity;
- typed read-only question planning;
- progressive bounded search and CLI/MCP parity;
- safe PR impact workflow and diagnostics;
- security fixes;
- migration/re-index note;
- known limitations;
- benchmark links with scoped claims.

Create a release-note draft using the required `RELEASING.md` sections:

- Highlights
- Breaking changes / schema migration
- New CLI/MCP behavior
- Dependency/security notes
- Migration notes
- Known limitations
- Reproducible benchmark summary

Suggested commit:

```bash
git add Cargo.toml Cargo.lock npm python README.md CHANGELOG.md RELEASING.md
git commit -m "chore: prepare GitCortex ${NEW_VERSION}"
```

---

## 13. Final PR-5 local gate

Run exactly:

```bash
just fmt-check
just clippy
just test
just build
cargo audit --no-fetch

cd viz
npm ci
npm run lint
npm test
npm run build
npm audit --omit=dev --audit-level=high
cd ..

cd tools/agent-bench
python3 -m unittest discover -p "test_*.py"
cd ../..

python3 scripts/r1_release_gate.py
dist plan --output-format=json | jq '.releases[] | {name, artifacts: (.artifacts | length)}'
git diff --check
git status --short --branch
```

Also run the release binary E2E from `RELEASING.md` in a disposable repository.

Required state:

- no test/lint/audit failures;
- no dirty generated files;
- benchmark gate passes;
- release plan includes all four supported binary targets;
- docs contain no unsupported claim;
- package versions agree;
- rollback test passes.

---

## 14. Independent review gate

Before pushing PR 5, request two static reviews of `origin/main...HEAD`:

1. specification/release compliance;
2. security, correctness, packaging, and claims review.

Reviewers must verify:

- no arbitrary natural-language query execution;
- exact branch/qualified identity behavior remains intact;
- benchmark fairness and provenance;
- no secrets in traces or manifests;
- schema/version/migration consistency;
- package/version agreement;
- release workflow least privilege;
- rollback instructions;
- no unsupported Graphify/CodeGraph claim.

Fix every blocker with a regression when code behavior changes, rerun full gates, and review the exact new head.

---

## 15. Open and merge PR 5

Push:

```bash
git push -u origin release/r1-combined-validation
```

Open:

```bash
gh pr create \
  --base main \
  --head release/r1-combined-validation \
  --title "release: validate and prepare GitCortex ${NEW_VERSION}" \
  --body-file .github/pr5-release-body.md
```

The PR body must include:

- exact benchmark result IDs;
- competitor commits;
- compatibility/migration result;
- package matrix result;
- full validation commands;
- scoped claims and known losses;
- proposed version;
- statement that no release is published until merge.

Enable auto-merge only after exact-head approval:

```bash
gh pr merge --auto --squash --delete-branch
```

Monitor:

```bash
gh pr checks --watch --interval 15
```

After merge:

```bash
gh pr view <PR_NUMBER> --json state,mergedAt,mergeCommit,url
git checkout main
git pull --ff-only origin main
git status --short --branch
```

Do not tag the feature branch.

---

## 16. Pre-tag verification on merged main

Re-run the release gate against merged `main`:

```bash
git rev-parse HEAD
python3 scripts/r1_release_gate.py
just ci
cargo audit --no-fetch
dist plan --output-format=json > /tmp/gitcortex-dist-plan.json
jq '.releases[] | {name, artifacts: (.artifacts | length)}' /tmp/gitcortex-dist-plan.json
git status --short --branch
```

Tag only if merged main is clean and identical in behavior to the approved candidate.

---

## 17. Tag and publish the one combined release

Create an annotated tag:

```bash
git tag -a "v${NEW_VERSION}" -m "GitCortex v${NEW_VERSION}"
git show "v${NEW_VERSION}" --no-patch
git push origin "v${NEW_VERSION}"
```

Watch release workflows:

```bash
gh run list --limit 10
gh run watch <RELEASE_RUN_ID>
```

Verify GitHub Release:

```bash
gh release view "v${NEW_VERSION}" --json url,name,tagName,isDraft,isPrerelease,assets
```

Required before other announcements:

- all four platform archives present;
- checksums present;
- signatures/certificates present where configured;
- release notes match the merged changelog;
- no draft or accidental prerelease state.

Then verify npm and PyPI workflows. Publish crates.io in dependency order exactly as documented in `RELEASING.md`; wait for indexing between dependency layers.

Do not declare the release complete until package installation has been verified from each channel actually published.

---

## 18. Post-publish channel verification

Use clean temporary environments.

### GitHub binary

Download one platform archive, verify checksum/signature, run:

```bash
gcx --version
gcx --help
```

### npm

```bash
npm view gitcortex@"${NEW_VERSION}" version
npx --yes gitcortex@"${NEW_VERSION}" --version
```

### PyPI

```bash
python3 -m venv /tmp/gcx-pypi-check
/tmp/gcx-pypi-check/bin/pip install "gitcortex==${NEW_VERSION}"
/tmp/gcx-pypi-check/bin/gcx --version
```

### crates.io

```bash
cargo search gitcortex --limit 1
cargo install gitcortex --version "${NEW_VERSION}" --locked --root /tmp/gcx-cargo-check
/tmp/gcx-cargo-check/bin/gcx --version
```

### Homebrew

Only when the tap prerequisite is configured:

```bash
brew update
brew info bharath03-a/tap/gitcortex
brew install bharath03-a/tap/gitcortex
gcx --version
```

Record channel URLs and verification timestamps in the release issue/report.

---

## 19. Launch sequence

Launch in this order:

1. GitHub Release and changelog.
2. Installation channels verified.
3. README/product page updated to the exact released version.
4. Evidence-based demo published.
5. OSS/community announcement.
6. Individual-developer outreach.
7. Team pilot invitations.
8. Organization/security materials only after install/rollback evidence is available.

Every numerical announcement must link to `docs/benchmarks/r1-competitor-evaluation.md` or another exact pinned report.

---

## 20. Rollback and stop conditions

### Stop before merge/tag when

- any required evidence task is invalid;
- qualified identity or incremental freshness regresses;
- competitor setup is unpinned or unfair;
- a package version differs;
- a claim lacks provenance;
- migration can destroy data outside explicit init;
- a secret appears in traces;
- release artifacts do not match the plan.

### After a tag but before downloads

Follow `RELEASING.md`. Prefer fixing and rerunning; do not conceal failed workflows.

### After artifacts are downloaded/published

Never rewrite or delete the release tag. Yank affected registry versions when supported, document the issue, and cut a new patch release.

### Partial registry publication

- npm/PyPI: rerun idempotent workflows where supported.
- crates.io late failure: wait for indexing or bump patch for affected descendants as documented.
- Homebrew failure: keep other channels live, fix the tap/formula, and document channel status.

---

## 21. Final completion checklist

The five-PR project is complete only when all boxes are checked:

- [ ] PR #85 verified merged and local main updated.
- [ ] PR 5 created from merged main.
- [ ] Version-policy contradiction resolved.
- [ ] Combined machine-readable release gate passes.
- [ ] Retrieval suite is fully valid and relevance non-inferior.
- [ ] Native-client runs use pinned tasks and at least three rounds.
- [ ] Graphify commit and workflow pinned.
- [ ] CodeGraph commit and workflow pinned.
- [ ] Competitor report includes wins, losses, unsupported tasks, and provenance.
- [ ] Migration/re-index behavior verified.
- [ ] Install/rollback matrix passes.
- [ ] All public claims reconciled.
- [ ] Versions agree on every package surface.
- [ ] Changelog and release notes complete.
- [ ] Full Rust/frontend/audit gates pass.
- [ ] Independent exact-head reviews approve PR 5.
- [ ] PR 5 merges into main.
- [ ] Merged main passes pre-tag verification.
- [ ] One combined tag/release published.
- [ ] GitHub/npm/PyPI/crates.io/Homebrew channels verified or explicitly marked unavailable.
- [ ] Launch materials link only to released, reproducible evidence.

Only then mark PR 5 and the overall release train complete.
