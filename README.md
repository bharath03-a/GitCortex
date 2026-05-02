# GitCortex

A local-first, branch-aware code knowledge graph for Git repositories. GitCortex (`gcx`) indexes your codebase incrementally on every commit using tree-sitter AST parsing, persists the graph in an embedded KuzuDB database, and exposes it to AI coding assistants via an MCP server.

---

## Why

When you ask Claude Code to work on a large codebase, it either scans dozens of files to build context (burning tokens) or misses the bigger picture entirely. There's no middle ground.

GitCortex gives Claude a pre-built, queryable map of your repo — functions, structs, traits, call relationships, file structure — so instead of reading raw source files it can ask precise questions like "what calls this function?" or "what's defined in this file?" and get structured answers instantly. You get better context at a fraction of the token cost.

---

## How it works

1. `gcx init` installs four git hooks and runs an initial full index.
2. On every local HEAD change the hook fires, diffs only the changed files, and updates the graph in under 500ms.
3. `gcx serve` starts an MCP server on stdio so Claude Code (or any MCP client) can query the graph.
4. `gcx viz` opens an interactive force-directed graph in your browser.

The graph is namespaced per branch — switching branches instantly gives you the graph for that branch with no re-indexing.

---

## Requirements

- Git
- Rust 1.80+ (only needed for source installs — pre-built binaries require nothing)

---

## Installation

**macOS / Linux — pre-built binary (no Rust required):**

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/bharath03-a/GitCortex/releases/latest/download/gcx-installer.sh | sh
```

> Pre-built binaries for macOS (arm64/x86_64) and Linux (x86_64/aarch64) are published
> automatically on every release via GitHub Releases.
> Windows users should build from source (see below) — a prebuilt binary will ship in a future release.

**Cargo (from crates.io):**

```bash
cargo install gitcortex-mcp
```

**Cargo (from git — works before crates.io publish):**

```bash
cargo install --git https://github.com/bharath03-a/GitCortex --bin gcx
```

**Build from source:**

```bash
git clone https://github.com/bharath03-a/GitCortex
cd GitCortex
cargo build --release
./target/release/gcx --help
```

---

## Quick start

```bash
cd your-repo
gcx init
```

That installs the git hooks and indexes the current branch. Every subsequent commit updates the graph automatically.

---

## Commands

### `gcx init`

Installs four git hooks, runs the initial full index, registers the MCP server globally in `~/.claude.json`, and installs Claude Code slash commands and agent skills.

```bash
gcx init            # hooks + index + MCP + slash commands + skills
gcx init --ci       # also writes .github/workflows/gcx-blast-radius.yml
```

Output:
```
GitCortex initialised  (1240ms)
  Graph:    312 nodes | 847 edges
  Hooks:    4 installed
  MCP:      ~/.claude.json  (gcx serve registered globally)
  Skills:   .claude/skills/gcx/  (4 agent skills)
  Commands: .claude/commands/gcx/  (4 slash commands)
```

### `gcx hook`

Called automatically by the git hooks — you rarely invoke this directly.

```bash
gcx hook                   # post-commit / post-merge / post-rewrite
gcx hook --branch-switch   # post-checkout (no re-index, just updates branch pointer)
```

### `gcx serve`

Starts the MCP server on stdio. Wire this up in your `.mcp.json` to give Claude Code access to the knowledge graph.

```bash
gcx serve
```

### `gcx query`

One-shot CLI queries for manual inspection.

```bash
gcx query lookup-symbol MyStruct
gcx query find-callers process_request --branch main
gcx query list-definitions src/lib.rs
```

### `gcx viz`

Visualise the knowledge graph.

```bash
gcx viz                            # open interactive browser UI (default port 5678)
gcx viz --port 9000                # custom port
gcx viz --branch feat/auth         # visualise a different branch
gcx viz --format dot > graph.dot   # export Graphviz DOT to stdout
dot -Tsvg graph.dot -o graph.svg   # render with Graphviz
```

The browser UI is fully self-contained (no CDN) with a dark theme, force-directed layout, pan/zoom, a click-to-inspect panel, and N-hop focus mode — click any node to dim everything outside its 1/2/3-hop neighborhood.

### `gcx blast-radius`

Show which callers are affected by changes between two branches. Powers the PR comment bot.

```bash
gcx blast-radius --base main --head feat/auth
gcx blast-radius --base main --head feat/auth --depth 3
gcx blast-radius --base main --head feat/auth --format github-comment
gcx blast-radius --base main --head feat/auth --format json
```

Example output (`--format text`):
```
Blast Radius Report
────────────────────────────────────────────────────
  feat/auth → main
  Changed: 2  |  Affected: 8  |  Risk: MEDIUM
────────────────────────────────────────────────────
Changed nodes:
  function    validate_token               src/auth.rs:23
  method      build_claims                 src/auth.rs:54

Affected callers:
  [hop 1]  function    handle_request      src/handler.rs:8
  [hop 1]  function    middleware_chain    src/middleware.rs:3
  [hop 2]  function    router              src/main.rs:12
  ...
```

### `gcx export`

Generates `.gitcortex/context.md` — a readable Markdown codebase map organized by file with hierarchical struct→method containment. Once generated, the git hook keeps it fresh after every commit.

```bash
gcx export                   # writes .gitcortex/context.md for current branch
gcx export --branch feat/auth
```

Example output:
```markdown
# Codebase Map

> Branch: `main` · 312 definitions · SHA: `abc1234`

## src/auth.rs

- `pub struct AuthConfig` :5
  - `pub fn from_env` :10
  - `pub fn is_valid` :20
- `pub async fn validate_token` :30

## src/handler.rs

- `pub fn handle_request` :8
```

Commit `.gitcortex/context.md` to give teammates (and Claude) instant codebase context without an MCP server.

### `gcx status`

Show node and edge counts for the current branch.

```bash
gcx status
gcx status --branch feat/auth
```

```
branch:     main
last sha:   abc1234...
nodes:      312
  function     80
  method       69
  struct       22
  ...
edges:      847
  calls        514
  contains     246
  ...
```

### `gcx clean`

Wipe the graph store for this repo so the next `gcx init` or commit triggers a full re-index.

```bash
gcx clean
```

---

### CI / PR blast radius bot

```bash
gcx init --ci
```

This writes `.github/workflows/gcx-blast-radius.yml`. On every pull request it runs `gcx blast-radius` and posts the result as a sticky PR comment using the `github-comment` format.

---

## MCP integration

`gcx init` registers the MCP server in `~/.claude.json` — no per-project config needed. The server is available in every Claude Code session on this machine automatically.

### Available MCP tools

| Tool | Description |
|---|---|
| `lookup_symbol` | Find all nodes matching a name across the codebase |
| `find_callers` | Find all functions that call a given function |
| `list_definitions` | List all definitions in a source file ordered by line |
| `branch_diff_graph` | Show nodes added or removed between two branches |

All tools accept an optional `branch` parameter (defaults to `"main"`).

### Claude Code slash commands

`gcx init` installs four slash commands into `.claude/commands/gcx/` that are immediately available in Claude Code:

| Command | What it does |
|---|---|
| `/gcx-lookup <name>` | Find all definitions matching a name |
| `/gcx-callers <name>` | Find all callers of a function |
| `/gcx-file <path>` | List all definitions in a file |
| `/gcx-blast-radius` | Show blast radius of changes vs main |

---

## Configuration

### `.gitcortex/config.toml`

Committed to the repo and shared with your team.

```toml
[index]
languages = ["rust", "typescript", "python", "go"]
max_file_size_kb = 500

[lld]
enabled = false         # pass-2 LLD annotation (v0.2)

[store]
backend = "local"       # local only in v0.1; remote backend planned
```

### `.gitcortex/ignore`

`.gitignore`-syntax patterns for files to exclude from indexing.

```gitignore
target/
build/
**/*.generated.rs
**/*.pb.rs
```

---

## Graph schema

### Node kinds

| Kind | Description |
|---|---|
| `file` | Source file |
| `module` | `mod foo { }` |
| `struct` | `struct Foo { }` |
| `enum` | `enum Bar { }` |
| `trait` | `trait Baz { }` |
| `type_alias` | `type Alias = ...` |
| `function` | Free-standing `fn` |
| `method` | `fn` inside an `impl` block |
| `constant` | `const` / `static` |
| `macro` | `macro_rules!` or proc-macro |

### Edge kinds

| Kind | Description |
|---|---|
| `contains` | Parent–child: `File→Module`, `Struct→Method` |
| `calls` | Resolved call site: `Function→Function` |
| `implements` | `impl Trait for Struct` → `Struct→Trait` |
| `uses` | Type appears as parameter or return type |
| `imports` | `use path::to::Thing` |

---

## Data storage

The graph database is stored locally and never committed:

```
~/.local/share/gitcortex/{repo_id}/
    graph.kuzu       # KuzuDB database (all branches, namespaced by table prefix)
    main.sha         # last indexed SHA for branch "main"
    feat__auth.sha   # last indexed SHA for branch "feat/auth"
```

---

## Architecture

```mermaid
flowchart TD
    subgraph repo["Your Repository"]
        hooks["git hooks\npost-commit · post-merge · post-rewrite · post-checkout"]
        files["Source Files — .rs · .ts · .py · .go"]
    end

    subgraph indexer["gitcortex-indexer"]
        differ["git2 differ\nchanged files only"]
        parsers["tree-sitter parsers\nRust · TypeScript · Python · Go"]
        differ --> parsers
    end

    kuzu[("KuzuDB\nbranch-namespaced\ngraph store")]

    subgraph gcx["gitcortex-mcp  ·  gcx"]
        server["MCP server\nlookup_symbol · find_callers\nlist_definitions · branch_diff_graph"]
        blast["gcx blast-radius\nrisk scoring · PR comment"]
        viz["gcx viz\nbrowser graph · DOT export"]
    end

    claude["Claude Code\nMCP tools · slash commands · skills"]
    gh["GitHub Actions\nsticky PR blast-radius comment"]

    hooks -->|"gcx hook — incremental diff"| differ
    files --> differ
    parsers -->|"GraphDiff\nnodes + edges"| kuzu
    kuzu --> server
    kuzu --> blast
    kuzu --> viz
    server --> claude
    blast --> gh
```

The `GraphStore` trait is the extensibility boundary — the local KuzuDB backend can be swapped for a remote backend without touching the indexer or MCP layer.

---

## License

MIT
