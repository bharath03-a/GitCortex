Dogfood GitCortex on its own repo. Installs the local `gcx` binary, indexes this workspace, and prints a sanity tour.

Steps:

1. Build and install from source:
   `cargo install --path crates/gitcortex-cli --force`

2. Confirm version: `gcx --version` matches `Cargo.toml` workspace version.

3. If `.git/hooks/post-commit` does not contain `gcx hook`, run `gcx init` in the repo root. Otherwise skip (already initialised).

4. Force a full index of current branch:
   `gcx hook --force` (or equivalent reindex flag — check `gcx hook --help`)

5. Smoke queries:
   - `gcx query tour --limit 5` — top 5 centrality symbols
   - `gcx query lookup-symbol GraphStore` — should find the trait
   - `gcx query find-callers apply_diff` — should list parser/indexer callers
   - `gcx query list-definitions crates/gitcortex-core/src/schema.rs`

6. Report: index path (`~/.local/share/gitcortex/...`), branch indexed, node/edge count if surfaced, any query that returned empty unexpectedly.

If install fails, surface the cargo error and offer **rust-build-fixer**. If a query returns empty when it shouldn't, that's a real bug — report as a finding, don't paper over it.
