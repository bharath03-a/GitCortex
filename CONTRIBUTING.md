# Contributing to GitCortex

Thanks for your interest in contributing. GitCortex spans three engineering domains, and we've tried to make it possible to work productively in any one of them without learning the others.

If something here is wrong or unclear, please open a PR — meta-contributions to this file count.

## TL;DR — fresh-clone dev loop

```bash
git clone https://github.com/bharath03-a/GitCortex.git
cd GitCortex
mise install                          # rust 1.95 + node 20 (see mise.toml)
just bootstrap                        # cargo fetch + npm ci
just dev                              # backend on :5678 + viz HMR on :5173
```

If `mise` and `just` are foreign to you, see [`docs/DEV_SETUP.md`](docs/DEV_SETUP.md) for the long-form setup.

## Pick your track

GitCortex has three working surfaces. Pick the one that matches what you want to fix or build, and skim only the relevant section.

### Track 1 — Frontend / UX

You want to: improve the graph viewer, fix a UI bug, add a new panel, tweak styling.

You'll edit: `crates/gitcortex-mcp/viz/src/`

Your dev loop:

```bash
just bootstrap                        # once
gcx viz --port 5678 &                 # backend serves /data + /api/*
cd crates/gitcortex-mcp/viz
npm run dev                           # Vite HMR at :5173, proxies to :5678
```

You can ignore everything outside `viz/`. No Rust knowledge required after the first `just bootstrap`. Tests live in `viz/src/__tests__/` (vitest). Lint with `npm run lint`, format with `npm run format`.

What you should know:
- **Tailwind v4** with custom design tokens in `viz/src/theme/tokens.css`. Prefer the tokens (`--color-accent`, `--color-elevated`) over hard-coded hex.
- **Cosmograph v2** is the canvas renderer. Its [docs](https://cosmograph.app/docs) are sparse — read the type defs in `node_modules/@cosmograph/cosmograph/cosmograph/config/interfaces/`.
- **No shadcn/Radix** — components are hand-rolled. Use `lucide-react` for icons.
- Data flows from `/data` once, then `/api/*` for on-demand details. Avoid adding new fetches in render paths — extend the initial `/data` payload instead.

### Track 2 — Parser / Indexer

You want to: fix a parsing bug, add support for a new language, improve cross-file edge resolution.

You'll edit: `crates/gitcortex-indexer/src/parser/`

Your dev loop:

```bash
cargo test -p gitcortex-indexer       # unit tests against fixture files
cargo run -p gitcortex -- query lookup-symbol Foo    # one-shot CLI
```

What you should know:
- Each language has its own file (`rust.rs`, `python.rs`, `typescript.rs`, `go.rs`, `java.rs`) and a Tree-sitter query file at `parser/queries/<lang>.scm`.
- The output is always a `GraphDiff` (defined in `gitcortex-core/src/graph.rs`). Cross-file edges that can't be resolved against the local diff are pushed into `deferred_calls` / `deferred_uses` / etc. and resolved by the store after insertion.
- Add fixtures to `tests/integration/fixtures/` and write regression tests in `crates/gitcortex-mcp/tests/full_pipeline.rs`.

### Track 3 — MCP / Server / CLI

You want to: add a new MCP tool, change blast-radius scoring, add a new `gcx` subcommand, improve the Axum API.

You'll edit: `crates/gitcortex-mcp/src/`

Your dev loop:

```bash
cargo test -p gitcortex-mcp           # integration tests
cargo run -p gitcortex -- serve       # MCP server on stdio
cargo run -p gitcortex -- viz         # HTTP viz server
```

What you should know:
- The async boundary is in this crate only. Indexer and store calls are synchronous — wrap them in `tokio::task::spawn_blocking` when called from an `async fn` handler.
- MCP tool definitions are in `src/mcp/tools.rs`. Adding a tool means adding a method to the `GraphStore` trait (in `gitcortex-core`) and a tool wrapper here.
- Axum routes for the viz are in `src/cmd/viz.rs`. The `gcx` subcommands live in `src/cmd/*.rs`.

## Pull request checklist

Before opening a PR:

- [ ] `cargo fmt --all` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` passes
- [ ] If you touched `viz/`: `npm run lint && npm run test && npm run build` clean
- [ ] If you added a public API: rustdoc comments explain *why*, not just *what*
- [ ] If you changed user-facing behaviour: updated `README.md` and `CHANGELOG.md`
- [ ] If you added a workspace member or removed one: `dist-workspace.toml` still valid
- [ ] Commit messages follow Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`, `ci:`)

`just ci` runs all of the above locally and mirrors what GitHub Actions runs.

## Commit message format

```
<type>(<scope>): <short summary in present tense>

<optional body explaining why, with hard wrap at 72 cols>

<optional footer: BREAKING CHANGE: <description>>
```

Examples:

```
feat(indexer): support Python `match` statement
fix(viz): debounce search input to avoid layout thrash
refactor(store): split kuzu.rs into module
chore(deps): bump axum 0.7.5 → 0.7.6
```

The `scope` is the crate name (`core`, `indexer`, `store`, `mcp`, `viz`) or a top-level area (`ci`, `docs`, `deps`).

## Code style

- **Rust:** `rustfmt` (see `rustfmt.toml`) + `clippy` (see `clippy.toml`). No `unwrap()` or `expect()` in library code; use `?` and propagate `Result`. No `unsafe` without a comment block explaining the invariant.
- **TypeScript:** `eslint` (flat config in `viz/eslint.config.js`) + `prettier` (`viz/.prettierrc`). Strict mode is on; do not add `any` without justification.
- **Comments:** lead with *why*, not *what*. The code already says what.
- **Tests:** every bug fix gets a regression test. Every new feature gets a happy-path test and at least one edge case.

## Security disclosure

If you discover a vulnerability, please email the maintainer privately rather than opening a public issue. See `SECURITY.md` once we publish it (TODO).

## Project governance

GitCortex is currently maintained by a single author. CODEOWNERS (`.github/CODEOWNERS`) lists who reviews which paths. As the team grows, we'll add more reviewers per domain.

## Code of Conduct

By participating, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md) (Contributor Covenant 2.1).
