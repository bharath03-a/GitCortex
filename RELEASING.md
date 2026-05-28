# Releasing GitCortex

Operational runbook for cutting a new release. Aims for one-shot pushes when CI
is green; rollback paths included for the cases where it isn't.

Skip ahead to [Quick checklist](#quick-checklist) if you've done this before.

---

## Versioning

Single source of truth: `[workspace.package].version` in the workspace
`Cargo.toml`. All crates inherit it via `version.workspace = true`.

| Surface | Where the version lives |
|---|---|
| Workspace crates | `Cargo.toml` `workspace.package.version` |
| Workspace path deps | `Cargo.toml` `workspace.dependencies.gitcortex-* version = "0.X"` |
| npm umbrella + per-platform pkgs | `npm/packages/*/package.json` `version` + `optionalDependencies` |
| Python sdist | `python/src/gitcortex/__init__.py` `__version__` |
| README example output | `README.md` (`gcx doctor` / `gcx update` examples) |

We follow **SemVer 0.x**:

- `0.X.0` — minor; allowed to break (current line)
- `0.X.Y` — patch; bugfix only, no schema bumps, no new MCP tools
- Bump `0.X` when schema (`SCHEMA_VERSION` in `gitcortex-core/src/schema.rs`)
  changes, when CLI/MCP surface gains/removes commands or tools, or when
  parser semantics change in a user-visible way.

When you change `SCHEMA_VERSION`, existing users will have their local graph
wiped and re-indexed automatically on the next git hook. Call this out in the
release notes.

---

## Quick checklist

```text
[ ] 1. Branch from main: feature/v0-X-x
[ ] 2. Cut work commits, keep CI green on every push
[ ] 3. Open PR → main, get review, merge
[ ] 4. Bump versions everywhere (see Bump versions below)
[ ] 5. cargo fmt --all && cargo clippy --all-targets -- -D warnings
[ ] 6. cargo test --workspace && (cd viz && npm test && npm run build)
[ ] 7. dist plan       # sanity-check release.yml is up to date
[ ] 8. Commit + tag:   git tag vX.Y.Z && git push origin vX.Y.Z
[ ] 9. Watch GitHub Actions: release.yml, publish-npm.yml, publish-pypi.yml
[ ] 10. Publish to crates.io (see Publish to crates.io)
[ ] 11. Post-release: write release notes, update CHANGELOG, announce
```

---

## Bump versions

A single workspace bump touches multiple files. There is no `cargo workspace
bump` built-in; do it manually and verify with `grep`.

```bash
NEW_VERSION="0.3.0"

# 1. Cargo workspace
sed -i.bak "s/^version *= *\"[0-9.]*\"/version = \"${NEW_VERSION}\"/" Cargo.toml
# Also bump workspace.dependencies path deps' SemVer minor (e.g. 0.3)
# Manual edit: change every `version = "0.X"` line in the [workspace.dependencies] block.

# 2. npm packages
NPM_MAJOR_MINOR="${NEW_VERSION%.*}"  # not used; we replace exact strings
for f in npm/packages/*/package.json; do
  sed -i.bak "s/\"[0-9.]*\"/\"${NEW_VERSION}\"/g" "$f"
done

# 3. Python sdist
sed -i.bak "s/__version__ = \"[0-9.]*\"/__version__ = \"${NEW_VERSION}\"/" \
  python/src/gitcortex/__init__.py

# 4. README example output (gcx doctor / gcx update)
sed -i.bak "s/0\\.[0-9]*\\.[0-9]*/${NEW_VERSION}/g" README.md

# Clean up backups
find . -name '*.bak' -not -path './target/*' -not -path '*/node_modules/*' -delete
```

Then verify nothing else is stale:

```bash
grep -rE "0\.[0-9]+\.[0-9]+" Cargo.toml README.md \
  npm/packages/*/package.json python/src/gitcortex/__init__.py
```

---

## Local verification (must pass before tagging)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --release
cargo test --workspace
cargo doc --workspace --no-deps

(cd viz && npm ci && npm run lint && npm test && npm run build && \
  npm audit --omit=dev --audit-level=high)

dist plan --output-format=json | jq '.releases[].artifacts | length'
```

Then run a real-world E2E:

```bash
./target/release/gcx --version          # should print the new version
cd /tmp && rm -rf gcx-rls-e2e && mkdir gcx-rls-e2e && cd gcx-rls-e2e
git init -q && echo 'fn main() {}' > main.rs && git add -A && \
  git -c user.email=t@t -c user.name=t commit -q -m init
/path/to/target/release/gcx init
/path/to/target/release/gcx query tour --branch master --limit 5
```

If E2E fails, fix before tagging.

---

## Tag + push

```bash
git tag -a "v${NEW_VERSION}" -m "GitCortex v${NEW_VERSION}"
git push origin "v${NEW_VERSION}"
```

The tag push triggers `.github/workflows/release.yml`. Watch in real time:

```bash
gh run watch
```

The release workflow:

1. Builds platform binaries via `dist` for the targets in
   `dist-workspace.toml` (currently: macOS arm64/x64, Linux x64/arm64).
2. Creates a GitHub Release with binaries + checksums attached.
3. `publish-npm.yml` (on `release: published`) builds + publishes each
   `@gitcortex/gcx-<plat>` and finally the umbrella `gitcortex` to npm.
4. `publish-pypi.yml` (on `release: published`) builds wheels per platform
   tag and publishes to PyPI via `pypa/gh-action-pypi-publish`.

---

## Publish to crates.io

The release workflow does **not** publish to crates.io. Do it manually after
the GitHub Release is live and the npm/PyPI publish jobs are green:

```bash
# Dependency order matters — crates.io must index each before the next
# can pull it. There's no atomic "publish workspace" command.
cargo publish -p gitcortex-core
sleep 30
cargo publish -p gitcortex-indexer
cargo publish -p gitcortex-store
sleep 30
cargo publish -p gitcortex-mcp
cargo publish -p gitcortex-viz
sleep 30
cargo publish -p gitcortex
```

Each step requires:

- A valid `CARGO_REGISTRY_TOKEN` in your environment (or run
  `cargo login` once).
- The workspace path-deps in `Cargo.toml` carrying matching `version = "0.X"`
  pins (so crates.io accepts them without local-path fallback).

If a publish fails:

- **Duplicate version**: bump patch, re-tag, re-run.
- **Missing dependency**: wait longer between steps (crates.io indexing is
  eventually-consistent).
- **Broken metadata**: yank that version and patch.

---

## Release notes

Format: GitHub Release body, in the tag's annotated message, mirror to
`CHANGELOG.md` once that file exists.

Required sections:

- **Highlights** (3–5 bullet points)
- **Breaking changes** (if any) — call out `SCHEMA_VERSION` bumps so users
  expect a one-time re-index
- **New CLI commands / MCP tools** (if any)
- **Dependency notes** (deferred Dependabot bumps, etc.)
- **Migration notes** (if any) — config changes, removed flags

---

## Rollback

| Failure | Recovery |
|---|---|
| Tag pushed, release.yml fails | Delete the GitHub Release (keep the tag), fix, re-push the same tag with `--force` and re-run the workflow. |
| npm publish fails midway | Re-run `publish-npm.yml` from the Actions tab. Individual `@gitcortex/gcx-*` versions are idempotent — npm rejects republishing the same version, which is fine; the workflow will skip them. |
| PyPI wheel publish fails | Same: re-run `publish-pypi.yml`. PyPI rejects duplicate filenames; the workflow surfaces a friendly error. |
| crates.io publish fails late in the chain | Bump patch (`0.X.Y` → `0.X.(Y+1)`), publish only the missing crate + its descendants. The previous crates remain valid. |
| Bad binary published | `cargo yank --vers <X.Y.Z> <crate>` on every affected crate. Cut a patch with the fix. Users who installed via the yanked version aren't auto-upgraded; flag in next release notes. |

Never delete a tag once binaries have been downloaded — bump and re-publish instead.

---

## Notes on platform coverage

- **Windows** is currently dropped. KuzuDB 0.11.3 (final upstream; repo
  archived 2025-10-10) doesn't link cleanly under MSVC. See
  `dist-workspace.toml` for the comment. Restore Windows when the embedded
  graph store is swapped.
- Linux x86_64 runner is pinned to `ubuntu-24.04` (GCC 13) because Kuzu's
  bundled `simsimd` needs `avx512fp16` / `__m512h`, available from GCC 12+
  only.
