---
name: ci-debugger
description: Catalog of known GitCortex CI failures and one-shot fixes. Use when a GitHub Actions job fails or you suspect drift between local and CI state.
---

# CI Debugger

GitCortex CI has a small set of recurring failure modes. Match against this catalog before deep-diving.

## Workflow files

- `.github/workflows/ci.yml` — fmt/clippy + multi-OS build matrix + nextest
- `.github/workflows/audit.yml` — cargo-audit
- `.github/workflows/release.yml` — cargo-dist on tag push
- `.github/workflows/viz-ci.yml` — viz frontend lint/build
- `.github/workflows/publish-npm.yml`, `publish-pypi.yml` — wrappers
- `.github/workflows/publish.yml` — orchestrator

## Known failures

### 1. `cargo fmt --check` fails on PR
**Symptom**: `rust-fmt-clippy` job red, diff in output.
**Cause**: Local commits without running fmt.
**Fix**: `cargo fmt --all` locally, commit, push. Prevent recurrence: the project's PostToolUse hook auto-formats on Edit/Write — make sure it's active in `.claude/settings.json`.

### 2. `gitcortex-mcp` build fails with `include_bytes!` error
**Symptom**: `couldn't find file viz/dist/index.html` or similar.
**Cause**: Viz bundle missing. `gitcortex-mcp/build.rs` embeds the viz dist.
**Fix**: Workflow must run `cd viz && npm ci && npm run build` BEFORE any cargo step. Check `ci.yml` for the `Build viz frontend` step ordering. Was broken in commit 5baf149 (stale viz paths).

### 3. Windows MSVC link errors (kuzu LNK4286)
**Symptom**: `LINK : warning LNK4286: symbol '?runAlgorithmEdgeCompute@...'`.
**Cause**: KuzuDB upstream MSVC ABI incompatibility. Not fixable from this repo.
**Fix**: Windows is dropped from the build matrix (commit 4e1d354). Do not re-add until Kuzu upstream is patched. See [project_kuzu_windows] memory.

### 4. Linux GCC 11 build failure on kuzu
**Symptom**: kuzu native build fails on Ubuntu 22.04 runner.
**Cause**: Kuzu requires newer GCC for C++20 features.
**Fix**: Use `ubuntu-latest` (24.04+) or explicit `gcc-13` install in the workflow. See [project_kuzu_windows].

### 5. Viz CI lint failure with tsconfig.tsbuildinfo
**Symptom**: viz-ci complains about uncommitted file.
**Cause**: Stale tsbuildinfo not gitignored.
**Fix**: Verify `viz/.gitignore` has `tsconfig.tsbuildinfo` (added in commit add3fef).

### 6. cargo-audit failure on transitive CVE
**Symptom**: `audit.yml` red with a CVE you didn't introduce.
**Cause**: Indirect dep flagged. Often unfixable without upstream bump.
**Fix**: Add the advisory ID to `deny.toml`/`audit.toml` ignore list with a one-line justification and an issue link. Do not blanket-ignore.

### 7. nextest hang on macOS runner
**Symptom**: Test job times out, no output for >10 min.
**Cause**: Hook tests spawning subprocesses without timeout.
**Fix**: Wrap subprocess-spawning tests with `tokio::time::timeout` or `#[ignore]` on macOS with a tracking issue.

## Diagnostic commands

```bash
gh run list --workflow=ci.yml --limit 5
gh run view <run-id> --log-failed
gh workflow view ci.yml
```

## When the catalog doesn't match

Use the **rust-build-fixer** agent on the raw error output. Once resolved, add the new failure mode to this skill so the next occurrence is one-shot.
