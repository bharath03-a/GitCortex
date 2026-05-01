# Changelog

All notable changes to GitCortex are documented here.

## [0.1.0] - 2026-04-30

Initial release.

### Features

**Incremental indexing**
- tree-sitter AST parsing for Rust, TypeScript, Python, and Go
- Indexes only changed files on every commit — <500ms on typical diffs
- Branch-namespaced graph: switching branches instantly gives you that branch's graph

**Graph schema**
- Node kinds: File, Folder, Module, Struct, Enum, Trait, TypeAlias, Function, Method, Constant, Macro
- Edge kinds: Contains, Calls, Implements, Uses, Imports
- Cross-file edge resolution for all edge kinds

**Git hooks (drift-proof)**
- `post-commit`, `post-merge`, `post-rewrite`, `post-checkout` installed by `gcx init`
- Hook prints a live graph summary after each commit

**CLI commands**
- `gcx init` — install hooks, run initial index, register MCP server globally
- `gcx hook` — incremental update triggered by git hooks
- `gcx serve` — MCP server on stdio
- `gcx query` — one-shot CLI queries (lookup-symbol, find-callers, list-definitions)
- `gcx viz` — interactive force-directed graph in the browser; DOT export
- `gcx blast-radius` — BFS transitive caller risk report (text / github-comment / json)
- `gcx export` — writes `.gitcortex/context.md` codebase map
- `gcx status` — node and edge counts by kind
- `gcx clean` — wipe graph store for fresh re-index

**MCP server**
- 4 tools: `lookup_symbol`, `find_callers`, `list_definitions`, `branch_diff_graph`
- Registered globally in `~/.claude.json` — works across all Claude Code sessions
- 4 agent skills and 4 slash commands installed into `.claude/`

**CI integration**
- `gcx init --ci` writes `.github/workflows/gcx-blast-radius.yml`
- Posts blast-radius report as a sticky PR comment on every pull request
