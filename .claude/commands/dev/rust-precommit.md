Run the GitCortex pre-commit gate in parallel and report results.

Execute these checks (parallelise where independent):

1. `cargo fmt --all -- --check` — formatting drift
2. `cargo clippy --workspace --all-targets -- -D warnings` — lints
3. `cargo nextest run --workspace` — tests (falls back to `cargo test --workspace` if nextest absent)
4. `cargo check -p gitcortex-mcp` — verifies viz/dist embed works (requires `cd viz && npm run build` to have run at least once)

If any step fails:
- Surface the first failure verbatim.
- Offer to launch the **rust-build-fixer** subagent for fmt/clippy/build issues, or the **rust-reviewer** subagent for clippy lints requiring judgement.

If all pass, print: `READY TO COMMIT` and a one-line summary of what was checked.

Do not commit anything. This command is a gate, not an action.
