---
name: release-gcx
description: End-to-end release flow for GitCortex — version bump across Cargo workspace + npm + pip, tag, cargo-dist binaries, registry pushes, post-release context refresh. Use when shipping a new version.
---

# Release GitCortex

GitCortex ships via cargo-dist (binaries), crates.io (Rust), npm (Node wrapper), PyPI (Python wrapper). Versions must stay in lockstep across all four.

## Pre-flight checklist

- [ ] On `main`, working tree clean, in sync with `origin/main`.
- [ ] CI green on the commit being released (`gh run list --branch main --limit 1`).
- [ ] `cargo audit` clean (or known-deferred CVEs documented in ADR).
- [ ] CHANGELOG.md updated with notable changes since last tag.
- [ ] README badges/version mentions match the version being released.

Run `/release-check` to gate these automatically.

## Version bump (must be atomic — one PR or one commit)

```
1. Cargo.toml (workspace)        version = "X.Y.Z"
2. crates/*/Cargo.toml           inherits via workspace.package.version — check no override
3. npm wrapper package.json       "version": "X.Y.Z"  (if separate from cargo-dist npm)
4. python wrapper pyproject.toml  version = "X.Y.Z"
5. README.md                      version mentions in install snippets if any
6. CHANGELOG.md                   move Unreleased → X.Y.Z (date)
```

Verify with: `git grep -nE 'version\s*=\s*"[0-9]'` — every match should show new version.

## Tag + release

```bash
git commit -am "chore(release): X.Y.Z"
git tag vX.Y.Z
git push origin main --tags
```

cargo-dist's `release.yml` workflow fires on the tag → builds binaries for the supported targets (macOS arm64/x64, Linux x64/aarch64; Windows is intentionally dropped — see [project_kuzu_windows]) → uploads to GitHub Releases → triggers npm/pypi publish workflows.

## Registry publish (Rust)

`cargo publish` is NOT automated. Manual order (dependency-bottom-up):

```
cargo publish -p gitcortex-core
cargo publish -p gitcortex-indexer
cargo publish -p gitcortex-store
cargo publish -p gitcortex-viz
cargo publish -p gitcortex-mcp
cargo publish -p gitcortex-cli
```

Each publish: wait ~30s for crates.io index update before the next. If a publish fails mid-chain, you cannot yank-and-retry the same version — bump to X.Y.Z+1 immediately.

## Post-release

- [ ] `gh release view vX.Y.Z` — confirm binaries attached.
- [ ] `npm view gitcortex@X.Y.Z` — version present.
- [ ] `pip index versions gitcortex` — version present.
- [ ] Smoke test: `curl ...gcx-installer.sh | sh` on a fresh shell → `gcx --version` prints X.Y.Z.
- [ ] Commit context refresh: run gcx hook on this repo (`gcx hook` after the release commit lands).
- [ ] Announce: GitHub Release notes, any social channels.

## Hotfix flow

For X.Y.Z+1 patch releases: branch from the tag (`git checkout -b hotfix/X.Y.Z+1 vX.Y.Z`), cherry-pick the fix, repeat the bump + tag + publish dance. Do not fast-forward main without re-merging.

## Verify

`/release-check` runs the pre-flight gate. Use **rust-build-fixer** if any cargo command fails. Update [project_release_v010] memory after release with anything that surprised you.
