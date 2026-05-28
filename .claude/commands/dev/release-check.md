Pre-release gate for GitCortex. Run before bumping version. Reports a checklist with PASS/FAIL per item — do not bump or tag.

Checks:

1. **Branch state** — on `main`, clean tree, in sync with origin.
   `git status --porcelain` empty, `git rev-list main..origin/main --count` == 0.

2. **CI green on HEAD** —
   `gh run list --branch main --limit 1 --json conclusion` shows `success`.

3. **Version consistency** — every place that names a version agrees.
   - `cargo metadata --format-version 1 | jq '.workspace_metadata, .packages[].version'`
   - `git grep -nE '"version":\s*"[0-9]'` across `package.json` files
   - `git grep -nE '^version\s*=\s*"[0-9]'` across `pyproject.toml` files
   - README install snippets
   Report any mismatch with file:line.

4. **CHANGELOG ready** — `CHANGELOG.md` has an entry for the version about to ship (i.e. an unreleased section with content, not empty).

5. **Audit clean** — `cargo audit` exits 0, or every advisory is in `audit.toml` ignore list with a justification.

6. **No unwrap regressions in library code** —
   `git grep -nE '\.(unwrap|expect)\(' -- 'crates/gitcortex-core/**/*.rs' 'crates/gitcortex-indexer/**/*.rs' 'crates/gitcortex-store/**/*.rs'`
   should match only known/approved instances (compare against last release's count).

7. **All tests pass** — `cargo nextest run --workspace`.

8. **MCP smoke test** — `gcx --version` works, `gcx serve --help` lists expected tools.

Output format:
```
| # | Check                  | Status | Notes              |
| 1 | Branch state           | PASS   |                    |
| 2 | CI green               | FAIL   | last run errored   |
...
```

End with: `READY TO RELEASE` or `BLOCKED ON: <list>`.

If READY, suggest the next command (`/release-gcx` workflow via the skill, or manual bump).
