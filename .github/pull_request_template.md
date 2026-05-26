<!--
Thanks for sending a pull request! Please fill out the relevant sections below.
The bot will request review from owners of the paths you touched (see .github/CODEOWNERS).
-->

## Summary

<!-- One or two sentences. What changes, and why? -->

## Type

- [ ] feat — new user-facing feature
- [ ] fix — bug fix
- [ ] refactor — internal change with no behaviour difference
- [ ] perf — measurable performance improvement
- [ ] docs — documentation only
- [ ] test — added or changed tests
- [ ] chore — tooling / deps / CI
- [ ] BREAKING — public API or CLI change

## Domain (tick all that apply)

- [ ] `gitcortex-core` — types, traits, schema
- [ ] `gitcortex-indexer` — tree-sitter parsers, git diff, language support
- [ ] `gitcortex-store` — KuzuDB queries, store implementation
- [ ] `gitcortex-mcp` — CLI, MCP server, Axum viz HTTP server
- [ ] `viz/` — React + Cosmograph frontend
- [ ] docs / ADRs
- [ ] CI / release / packaging

## Checklist

- [ ] `cargo fmt --all` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` passes (or relevant subset)
- [ ] If `viz/` touched: `npm run lint && npm run test && npm run build` pass
- [ ] If user-facing: README and/or CHANGELOG updated
- [ ] If a new public API was added: rustdoc comments explain the intent
- [ ] If risky: ADR added under `docs/adr/`

## Test plan

<!-- How did you verify this works? Include commands, expected output, screenshots if UI. -->

## Notes for the reviewer

<!-- Anything non-obvious, or things you want explicit pushback on. -->
