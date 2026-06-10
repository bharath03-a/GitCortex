# GitCortex Reference

Complete reference for the `gcx` CLI, MCP server tools, configuration, and graph schema.

---

## Table of Contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [CLI reference](#cli-reference)
  - [gcx init](#gcx-init)
  - [gcx serve](#gcx-serve)
  - [gcx hook](#gcx-hook)
  - [gcx query](#gcx-query)
  - [gcx blast-radius](#gcx-blast-radius)
  - [gcx viz](#gcx-viz)
  - [gcx export](#gcx-export)
  - [gcx status](#gcx-status)
  - [gcx clean](#gcx-clean)
  - [gcx doctor](#gcx-doctor)
  - [gcx update](#gcx-update)
- [MCP server](#mcp-server)
  - [Editor setup](#editor-setup)
  - [Compact mode](#compact-mode)
  - [Tool reference](#tool-reference)
  - [Prompt reference](#prompt-reference)
  - [Slash commands (Claude Code)](#slash-commands-claude-code)
- [Configuration reference](#configuration-reference)
  - [.gitcortex/config.toml](#gitcortexconfigtoml)
  - [.gitcortex/ignore](#gitcortexignore)
- [Graph schema](#graph-schema)
  - [Node kinds](#node-kinds)
  - [Edge kinds](#edge-kinds)
  - [Node metadata](#node-metadata)
  - [Cyclomatic complexity](#cyclomatic-complexity)
- [Data storage](#data-storage)
- [Environment variables](#environment-variables)

---

## Installation

### Cargo (source)

```bash
cargo install gitcortex
```

### npm

```bash
npm install -g gitcortex
```

### PyPI

```bash
pip install gitcortex
```

### Pre-built binary

Download from the [releases page](https://github.com/bharath03-a/GitCortex/releases) for macOS (arm64/x86_64) or Linux (x86_64/aarch64).

> **Note:** Windows is not supported. KuzuDB 0.11 does not build under MSVC.

---

## Quick start

```bash
# 1. Go to your repo
cd /path/to/your/repo

# 2. Install hooks + run initial full index + register MCP for detected editors
gcx init

# 3. Verify setup
gcx doctor

# 4. Query the graph
gcx query lookup-symbol MyStruct
gcx query find-callers run --depth 2

# 5. Start the MCP server (editors do this automatically)
gcx serve
```

After `gcx init`, every `git commit`, `git pull`, `git rebase`, and `git checkout` keeps the graph current automatically.

---

## CLI reference

### Global flags

These flags apply to every subcommand.

| Flag | Values | Default | Description |
|------|--------|---------|-------------|
| `--color` | `auto`, `always`, `never` | `auto` | ANSI colour output. Also respects `NO_COLOR`, `CLICOLOR=0`, `TERM=dumb`. |

---

### gcx init

Install git hooks, run the initial full index, and register the MCP server for detected editors.

```
gcx init [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--ci` | Write `.github/workflows/gcx-blast-radius.yml` for the GitHub Actions PR bot. |
| `--editor <EDITOR>` | Force a specific editor config. One of: `claude`, `cursor`, `windsurf`, `copilot`, `antigravity`, `codex`, `all`. Defaults to auto-detecting from editor environment variables. |

**What `gcx init` does:**

1. Copies `hooks/post-commit`, `hooks/post-merge`, `hooks/post-rewrite`, `hooks/post-checkout` into `.git/hooks/`.
2. Runs a full index of the current HEAD.
3. Writes MCP server config for the detected editor(s).
4. Writes `.claude/commands/gcx/` slash commands (when Claude is detected).

**Examples:**

```bash
gcx init                        # auto-detect editors, register all
gcx init --editor claude        # Claude Code only
gcx init --editor codex         # Codex / OpenAI compact mode
gcx init --editor all           # all editors
gcx init --ci                   # + GitHub Actions workflow
```

---

### gcx serve

Start the MCP server on stdio (the transport used by all MCP clients).

```
gcx serve [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--compact` | Expose only the single dispatch tool (`gcx`) instead of all 15 individual tools. Reduces per-turn schema overhead ~95%. Codex uses this by default. |

**Notes:**
- The server auto-detects the current branch from `git symbolic-ref HEAD` at startup. All tools default to that branch.
- Editors launch `gcx serve` automatically via their MCP config. You rarely need to run this manually.

**Examples:**

```bash
gcx serve             # full surface (15 tools + prompts)
gcx serve --compact   # single dispatch tool only
```

---

### gcx hook

Incremental re-index triggered by git hooks. Not normally run by hand.

```
gcx hook [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--branch-switch` | Called from `post-checkout`. Records the new branch without re-indexing files. |

**Logic:**

```
sha = read last_indexed_sha for current branch
if sha == HEAD: exit (no-op)
diff = git diff sha..HEAD (filtered by .gitcortex/ignore)
apply diff to graph store
write HEAD → last_indexed_sha
```

---

### gcx query

One-shot graph queries from the terminal. Useful for scripts, CI, and manual inspection.

```
gcx query <SUBCOMMAND>
```

All query subcommands accept `--branch <NAME>` (default: `main`).

#### gcx query lookup-symbol

```
gcx query lookup-symbol <NAME> [--branch <BRANCH>]
```

Find all nodes whose name exactly matches `<NAME>`. Prints kind, qualified name, file, and line.

```bash
gcx query lookup-symbol AuthConfig
gcx query lookup-symbol run --branch feat/auth
```

#### gcx query find-callers

```
gcx query find-callers <NAME> [--depth <1-5>] [--branch <BRANCH>]
```

Find callers of a function. `--depth 1` (default) returns direct callers; higher values walk multiple hops.

```bash
gcx query find-callers validate_token
gcx query find-callers apply_diff --depth 3
```

#### gcx query find-callees

```
gcx query find-callees <NAME> [--depth <1-5>] [--branch <BRANCH>]
```

Find all functions that `<NAME>` calls, tracing forward through the call graph.

```bash
gcx query find-callees handle_request --depth 2
```

#### gcx query list-definitions

```
gcx query list-definitions <FILE> [--branch <BRANCH>]
```

List all symbols in a source file ordered by line number.

```bash
gcx query list-definitions src/auth.rs
gcx query list-definitions src/handlers/user.ts --branch main
```

#### gcx query symbol-context

```
gcx query symbol-context <NAME> [--branch <BRANCH>]
```

360° view: definition location + callers + callees + type usages.

```bash
gcx query symbol-context apply_diff
```

#### gcx query find-implementors

```
gcx query find-implementors <NAME> [--branch <BRANCH>]
```

Find all structs/classes that implement or inherit `<NAME>`.

```bash
gcx query find-implementors GraphStore
gcx query find-implementors Serializable --branch main
```

#### gcx query trace-path

```
gcx query trace-path <FROM> <TO> [--branch <BRANCH>]
```

Find the shortest call path between two functions (up to 6 hops).

```bash
gcx query trace-path main apply_diff
```

#### gcx query find-unused

```
gcx query find-unused [--kind <KIND>] [--branch <BRANCH>]
```

Find symbols with no callers or type references — dead code candidates.

`--kind` accepts: `function`, `method`, `struct`, `trait`, `interface`, `enum`, `constant`.

```bash
gcx query find-unused
gcx query find-unused --kind function
gcx query find-unused --kind struct --branch feat/cleanup
```

#### gcx query get-subgraph

```
gcx query get-subgraph <NAME> [--depth <1-5>] [--direction <in|out|both>] [--branch <BRANCH>]
```

Return all nodes and edges within N hops of a seed symbol.

| Flag | Default | Description |
|------|---------|-------------|
| `--depth` | `2` | Hops to expand from seed |
| `--direction` | `both` | `in` (ancestors), `out` (descendants), `both` |

```bash
gcx query get-subgraph KuzuGraphStore --depth 2
gcx query get-subgraph handle_request --direction out --depth 3
```

#### gcx query wiki

```
gcx query wiki <NAME> [--branch <BRANCH>]
```

Render a markdown wiki page for a symbol: signature, doc-comment, top callers/callees, cyclomatic complexity.

```bash
gcx query wiki apply_diff
```

#### gcx query search

```
gcx query search <QUERY> [--limit <N>] [--branch <BRANCH>]
```

Fuzzy search over name + qualified path. Ranked: exact → prefix → substring. Functions and structs are ranked higher.

```bash
gcx query search auth --limit 20
gcx query search "graph store" --branch main
```

#### gcx query tour

```
gcx query tour [--seed <NAME>] [--limit <N>] [--branch <BRANCH>]
```

Generate a guided tour through the codebase.

- Without `--seed`: picks highest-centrality public entry points.
- With `--seed`: BFS-walks outward from that symbol along call edges.

```bash
gcx query tour                      # global tour
gcx query tour --seed main          # tour from main()
gcx query tour --seed Auth --limit 20
```

---

### gcx blast-radius

Show the blast radius of changes between two branches.

```
gcx blast-radius [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--base <BRANCH>` | `main` | Branch to compare against (merge target) |
| `--head <BRANCH>` | `HEAD` | Branch with changes |
| `--depth <N>` | `2` | BFS depth for transitive caller discovery |
| `--format <FORMAT>` | `text` | Output format: `text`, `json`, `github-comment` |

**Formats:**
- `text` — human-readable terminal output
- `json` — machine-readable `{ changed, callers, risk_level }` object
- `github-comment` — Markdown suitable for posting as a sticky PR comment (used by the CI bot)

**Examples:**

```bash
gcx blast-radius
gcx blast-radius --base main --head feat/auth
gcx blast-radius --format json | jq '.risk_level'
gcx blast-radius --format github-comment > /tmp/comment.md
```

---

### gcx viz

Visualise the knowledge graph in a browser or as DOT output.

```
gcx viz [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--branch <BRANCH>` | `main` | Branch to visualise |
| `--format <FORMAT>` | `web` | `web` (browser), `dot` (Graphviz DOT), `mermaid` |
| `--port <PORT>` | `5678` | HTTP port for web mode |

**Examples:**

```bash
gcx viz                         # open browser at localhost:5678
gcx viz --format dot | dot -Tsvg > graph.svg
gcx viz --format mermaid
gcx viz --branch feat/auth --port 8080
```

---

### gcx export

Export the knowledge graph as a codebase map, JSON, or a CLAUDE.md symbol table.

```
gcx export [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--branch <BRANCH>` | current branch | Branch to export |
| `--format <FORMAT>` | `markdown` | `markdown` (codebase map) or `json` (symbols + edges) |
| `--claude-md` | false | Upsert a top-symbols table into `CLAUDE.md` (overrides `--format`) |
| `--top <N>` | `40` | Number of top-ranked symbols to inject with `--claude-md` |

**Format: markdown**

Writes `.gitcortex/context.md` — a structured Markdown map of all files, structs, functions. Commit it to give teammates instant codebase context.

**Format: json**

Emits `{ branch, sha, symbols[], edges[] }` to stdout. Each symbol carries `id`, `name`, `qualified_name`, `kind`, `file`, `line`, `visibility`. Edges reference symbol `id`s.

**Flag: --claude-md**

Upserts a compact, centrality-ranked symbol table between `<!-- gcx:symbols start -->` / `<!-- gcx:symbols end -->` markers in `CLAUDE.md`. AI assistants read the top-N most-referenced symbols (name → `file:line`) with zero tool calls.

**Examples:**

```bash
gcx export                          # writes .gitcortex/context.md
gcx export --branch feat/auth
gcx export --format json > graph.json
gcx export --claude-md              # inject top 40 symbols into CLAUDE.md
gcx export --claude-md --top 60
```

---

### gcx status

Show indexed node and edge counts for a branch.

```
gcx status [--branch <BRANCH>]
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

---

### gcx clean

Wipe the graph store for this repo. The next `gcx init` or commit triggers a fresh full index.

```
gcx clean
```

> **Irreversible.** All branch graphs for this repo are deleted.

---

### gcx doctor

Diagnose setup issues: hooks installed, store accessible, index current, MCP registered.

```
gcx doctor
```

```
  [ok] gcx v0.4.0 on PATH (/usr/local/bin/gcx)
  [ok] git repository detected
  [ok] post-commit hook installed
  [ok] post-merge hook installed
  [ok] post-rewrite hook installed
  [ok] post-checkout hook installed
  [ok] graph store accessible  (1 842 nodes, 4 217 edges on main)
  [ok] index is current  (HEAD abc1234)
  [ok] MCP registered  (Claude Code)
  [--] MCP not configured for Cursor  (run: gcx init --editor cursor)
```

---

### gcx update

Check for a newer release and print the right update command for your install method.

```
gcx update
```

---

## MCP server

The MCP server exposes the knowledge graph to AI coding assistants via the [Model Context Protocol](https://modelcontextprotocol.io).

### Editor setup

`gcx init` writes the appropriate config for each editor automatically.

#### Claude Code

`~/.claude.json` or project `.mcp.json`:

```json
{
  "mcpServers": {
    "gitcortex": {
      "command": "gcx",
      "args": ["serve"],
      "type": "stdio"
    }
  }
}
```

#### Cursor / Windsurf / Copilot

`~/.cursor/mcp.json` (adjust path per editor):

```json
{
  "mcpServers": {
    "gitcortex": {
      "command": "gcx",
      "args": ["serve"],
      "type": "stdio"
    }
  }
}
```

#### Codex (OpenAI)

Codex uses compact mode by default to reduce token overhead:

```toml
[mcp_servers.gitcortex]
command = "gcx"
args = ["serve", "--compact"]
startup_timeout_sec = 30
```

---

### Compact mode

`gcx serve --compact` exposes only the `gcx` dispatch tool instead of the 15 individual tools.

- Reduces per-turn MCP schema overhead ~95%
- All queries still work via the single `gcx` tool
- MCP prompts may not be available in clients that only load exposed tools

Switch any editor to compact mode by changing `args` to `["serve", "--compact"]` in its MCP config.

---

### Tool reference

All tools accept an optional `branch` parameter. When omitted, defaults to the branch active when `gcx serve` was started.

---

#### `gcx` — single dispatch

The compact server exposes only this tool. Use it to keep per-turn schema overhead minimal.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `action` | `string` | yes | One of: `lookup_symbol`, `find_callers`, `find_callees`, `find_unused_symbols`, `get_subgraph`, `search_code`, `start_tour`, `wiki_symbol`, `trace_path`, `list_definitions`, `symbol_context`, `list_symbols_in_range`, `branch_diff_graph` |
| `params` | `object` | yes | Same fields as the individual tool for the chosen action |

**Example:**

```json
{
  "action": "find_callers",
  "params": { "function_name": "apply_diff", "depth": 2 }
}
```

---

#### `lookup_symbol`

Find all nodes matching a name.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `name` | `string` | yes | — | Unqualified symbol name to search for |
| `fuzzy` | `boolean` | no | `false` | When `true`, returns all symbols whose name *contains* `name`. When `false`, exact match only. |
| `branch` | `string` | no | current | Branch name |

**Returns:** array of node objects.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Stable UUID for this node |
| `kind` | `string` | Node kind (see [node kinds](#node-kinds)) |
| `name` | `string` | Unqualified name |
| `qualified_name` | `string` | Fully qualified path |
| `file` | `string` | Repo-relative source file path |
| `start_line` | `integer` | Definition start line (1-indexed) |
| `end_line` | `integer` | Definition end line |
| `visibility` | `string` | `Pub`, `PubCrate`, or `Private` |
| `is_async` | `boolean` | — |
| `is_unsafe` | `boolean` | — |

**Example:**

```json
{ "name": "AuthConfig", "fuzzy": false }
```

---

#### `find_callers`

Find all callers of a function, with optional multi-hop depth.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `function_name` | `string` | yes | — | Unqualified name of the function |
| `depth` | `integer` | no | `1` | Hops to walk up the call graph. Range: 1–5. |
| `branch` | `string` | no | current | Branch name |

**Returns (depth=1):**

| Field | Type | Description |
|-------|------|-------------|
| `summary` | `string` | Human-readable summary with risk level |
| `function` | `string` | Queried function name |
| `depth` | `integer` | Requested depth |
| `risk_level` | `string` | `LOW` (0–2 callers), `MEDIUM` (3–10), `HIGH` (11–30), `CRITICAL` (31+) |
| `total_callers` | `integer` | True caller count (not capped) |
| `returned` | `integer` | Callers in this response |
| `truncated` | `boolean` | `true` when more callers exist than returned |
| `callers` | `array` | Caller node objects (capped at 25) |

**Returns (depth>1):** `hops` array, each hop has `hop`, `total`, `truncated`, `callers`.

**Example:**

```json
{ "function_name": "apply_diff", "depth": 2 }
```

---

#### `find_callees`

Find all functions that `function_name` calls, tracing forward.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `function_name` | `string` | yes | — | Starting function name |
| `depth` | `integer` | no | `1` | Hops to walk forward. Range: 1–5. |
| `branch` | `string` | no | current | — |

**Returns:** `{ function, depth, hops[] }` where each hop contains `{ hop, callees[] }`.

---

#### `symbol_context`

Get a 360° view of a symbol in one call.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Unqualified symbol name |
| `branch` | `string` | no | — |

**Returns:**

| Field | Type | Description |
|-------|------|-------------|
| `definition` | `object` | `kind`, `name`, `qualified_name`, `file`, `start_line`, `end_line`, `visibility`, `is_async`, `complexity` |
| `callers` | `array` | Nodes that call this symbol |
| `callees` | `array` | Nodes this symbol calls |
| `used_by` | `array` | Nodes that reference this symbol as a type |

`complexity` is `null` for non-function/method nodes and for languages where complexity is not computed.

**Example:**

```json
{ "name": "apply_diff" }
```

---

#### `list_definitions`

List all symbols in a source file, ordered by line number.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `file` | `string` | yes | Repo-relative path to the source file |
| `branch` | `string` | no | — |

**Returns:** array of `{ kind, name, qualified_name, start_line, end_line, loc, visibility, is_async }`.

**Example:**

```json
{ "file": "crates/gitcortex-store/src/kuzu/mod.rs" }
```

---

#### `branch_diff_graph`

Show which nodes were added or removed between two branches.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from_branch` | `string` | yes | Base branch (merge target) |
| `to_branch` | `string` | yes | Head branch with changes |

**Returns:** `{ from, to, added_nodes[], removed_nodes[] }`.

**Example:**

```json
{ "from_branch": "main", "to_branch": "feat/auth" }
```

---

#### `detect_changes`

Map current staged (or HEAD) changes to affected symbols + their callers.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `branch` | `string` | no | Branch to query indexed graph against |

**Returns:** `{ risk_level, total_affected, changed_symbols[] }`. Each symbol includes its direct callers. Returns a text message if no changes are detected.

Risk levels: `LOW` (≤5 affected), `MEDIUM` (6–20), `HIGH` (21–50), `CRITICAL` (51+).

---

#### `find_implementors`

Find all concrete types that implement a trait or interface.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `trait_name` | `string` | yes | Trait, interface, or abstract class name |
| `branch` | `string` | no | — |

**Returns:** `{ trait, implementors[] }`.

**Example:**

```json
{ "trait_name": "GraphStore" }
```

---

#### `trace_path`

Find the shortest call path between two symbols (up to 6 hops).

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | `string` | yes | Starting function/method name |
| `to` | `string` | yes | Target function/method name |
| `branch` | `string` | no | — |

**Returns:** `{ from, to, found, path[] }`. `path` is an ordered array of nodes. Empty array if no path exists within 6 hops.

**Example:**

```json
{ "from": "main", "to": "apply_diff" }
```

---

#### `list_symbols_in_range`

Find all symbols whose span overlaps a file + line range.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `file` | `string` | yes | Repo-relative file path |
| `start_line` | `integer` | yes | Range start (1-indexed, inclusive) |
| `end_line` | `integer` | yes | Range end (1-indexed, inclusive) |
| `branch` | `string` | no | — |

**Returns:** `{ file, range: { start, end }, symbols[] }`.

Use this to map a stack trace, diff hunk, or grep result to the symbols responsible.

**Example:**

```json
{ "file": "src/auth.rs", "start_line": 42, "end_line": 85 }
```

---

#### `find_unused_symbols`

Find symbols with zero callers or type references — dead code candidates.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `kind` | `string` | no | all | Filter by node kind: `function`, `method`, `struct`, `trait`, `interface`, `enum`, `constant` |
| `limit` | `integer` | no | `30` | Max results (capped at 200). `count` always reports the true total. |
| `branch` | `string` | no | current | — |

**Returns:** `{ branch, unused_symbols[], count, returned, truncated }`.

**Example:**

```json
{ "kind": "function", "limit": 50 }
```

---

#### `get_subgraph`

Return the neighbourhood subgraph centred on a seed symbol.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `seed_name` | `string` | yes | — | Seed symbol name (unqualified) |
| `depth` | `integer` | no | `1` | Hops to expand (1–5). Depth 2+ on high-degree nodes can return large graphs — raise deliberately. |
| `direction` | `string` | no | `"both"` | `"in"` (ancestors/callers), `"out"` (descendants/callees), `"both"` |
| `limit` | `integer` | no | `30` | Max nodes returned (capped at 200). Edges are filtered to the kept node set. |
| `branch` | `string` | no | current | — |

**Returns:** `{ seed, depth, direction, node_count, edge_count, returned_nodes, returned_edges, truncated, nodes[], edges[] }`.

Each edge: `{ src: id, dst: id, kind }`.

**Example:**

```json
{ "seed_name": "KuzuGraphStore", "depth": 2, "direction": "both" }
```

---

#### `wiki_symbol`

Render a Markdown wiki page for a symbol.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Symbol name (unqualified) |
| `branch` | `string` | no | — |

**Returns:** `{ symbol, branch, markdown }`.

The Markdown includes: kind + file + line, cyclomatic complexity (for functions/methods), visibility, doc-comment (if any), signature, top callers, top callees.

**Example:**

```json
{ "name": "apply_diff" }
```

---

#### `search_code`

Ranked fuzzy search over name + qualified path.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | `string` | yes | — | Substring matched against `name` and `qualified_name` |
| `limit` | `integer` | no | `10` | Max results (capped at 200) |
| `branch` | `string` | no | current | — |

**Returns:** `{ query, branch, count, hits[] }`. Ranked: exact match → prefix match → substring match. Functions and structs ranked higher than structural nodes.

**Example:**

```json
{ "query": "auth", "limit": 20 }
```

---

#### `start_tour`

Generate a guided tour through the codebase's most important symbols.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `seed` | `string` | no | — | When given, BFS-walks outward from this symbol along call edges. When omitted, picks highest-centrality entry points. |
| `limit` | `integer` | no | `12` | Tour steps (capped at 50) |
| `branch` | `string` | no | current | — |

**Returns:** `{ branch, seed, steps[], markdown }`. Each step includes the symbol name, kind, file, rationale, and complexity.

**Example:**

```json
{ "seed": "main", "limit": 20 }
```

---

### Prompt reference

Prompts are multi-step workflows your AI assistant executes automatically using the tools above. Available in full server mode.

In Claude Code, invoke them with `/mcp__gitcortex__<prompt_name>`.

#### `detect_impact`

Pre-commit blast radius analysis. Maps changed files to affected callers and scores risk.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `changed_files` | `string` | yes | Comma-separated list of repo-relative file paths |
| `branch` | `string` | no | Branch to query (default `main`) |

**What the assistant does:** calls `list_definitions` on each file, then `find_callers` on changed symbols, then summarises with a risk level (LOW / MEDIUM / HIGH / CRITICAL) and recommended actions.

#### `generate_map`

Generates an architecture diagram from the knowledge graph.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `branch` | `string` | no | Branch to document (default `main`) |

**Output:** Architecture overview prose + Mermaid `graph TD` module map + key types table + core execution flows + dependency notes.

---

### Slash commands (Claude Code)

`gcx init` installs four slash commands into `.claude/commands/gcx/`:

| Command | Description |
|---------|-------------|
| `/gcx-lookup <name>` | Find all definitions matching a name |
| `/gcx-callers <name>` | Find all callers of a function |
| `/gcx-file <path>` | List all definitions in a file |
| `/gcx-blast-radius` | Show blast radius of changes vs main |

---

## Configuration reference

### .gitcortex/config.toml

Committed to the repo and shared with your team. Created by `gcx init`.

```toml
[index]
languages = ["rust", "python", "typescript", "go", "java"]
max_file_size_kb = 500

[lld]
enabled = false
srp_method_threshold = 10
isp_method_threshold = 7

[store]
backend = "local"
```

#### `[index]` section

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `languages` | `string[]` | `["rust","python","typescript","go","java"]` | Languages to index. Valid values: `rust`, `python`, `typescript`, `javascript`, `go`, `java`. Files in other languages are skipped. |
| `max_file_size_kb` | `integer` | `500` | Skip files larger than this. Prevents indexing minified or generated blobs. |

#### `[lld]` section

LLD (Low-Level Design) annotation settings. Currently controls Pass 2 (background quality analysis).

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | `boolean` | `false` | Enable Pass 2 background LLD annotation. When `false`, only cyclomatic complexity (computed in Pass 1) is populated. |
| `srp_method_threshold` | `integer` | `10` | Structs/classes with more methods than this threshold are flagged with `GodStruct` smell. |
| `isp_method_threshold` | `integer` | `7` | Traits/interfaces with more methods than this threshold are flagged with `FatInterface` smell. |

#### `[store]` section

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `backend` | `string` | `"local"` | Storage backend. Only `"local"` (embedded KuzuDB) is available in v0.4. A remote backend is planned. |

---

### .gitcortex/ignore

`.gitignore`-syntax exclusion patterns for files and directories to skip during indexing. Committed to the repo.

```gitignore
# always skip build artifacts
target/
build/
dist/
vendor/

# skip generated files
**/*.generated.rs
**/*.pb.rs
**/*.min.js
```

The file is read by the [`ignore` crate](https://docs.rs/ignore) and supports the full `.gitignore` pattern syntax including negation (`!`), directory anchors, and double-star globs.

---

## Graph schema

### Node kinds

Every named, referenceable syntactic entity becomes a node.

| Kind | Languages | Description |
|------|-----------|-------------|
| `folder` | all | Directory node |
| `file` | all | Source file |
| `module` | all | `mod foo { }`, Python module, Go package |
| `struct` | Rust/Go/TS/Java | `struct Foo`, `class Foo` |
| `enum` | all | `enum Bar` |
| `trait` | Rust | `trait Baz` |
| `interface` | TS/Go/Java/Python | `interface Foo`, `Protocol` subclass |
| `type_alias` | Rust/TS/Python | `type Alias = ...` |
| `function` | all | Free-standing function |
| `method` | all | Method inside a class / impl block |
| `property` | TS/Python | Class property, `@property` |
| `constant` | all | `const` / `static` |
| `macro` | Rust | `macro_rules!` or proc-macro |
| `annotation` | Java | `@interface` annotation type |
| `enum_member` | all | Variant inside an enum |

### Edge kinds

| Kind | Description |
|------|-------------|
| `contains` | Parent–child: `File→Module`, `Module→Struct`, `Struct→Method` |
| `calls` | Resolved call site: `Function→Function` or `Method→Method` |
| `implements` | `impl Trait for Struct`, `class Foo implements Bar` |
| `inherits` | `class Foo extends Bar`, embedded struct in Go |
| `uses` | Type appears as a parameter or return type |
| `imports` | `use path::to::Thing`, `import` |
| `throws` | Java `throws ExceptionType` — method → exception class |
| `annotated` | Node decorated by `#[attr]`, `@decorator`, `@annotation` |

### Node metadata

Every node carries the following metadata:

| Field | Type | Description |
|-------|------|-------------|
| `loc` | `u32` | Lines of code in the node's body |
| `visibility` | `Pub \| PubCrate \| Private` | Symbol visibility |
| `is_async` | `bool` | `async fn`, `async def`, `async function` |
| `is_unsafe` | `bool` | Rust `unsafe fn` |
| `is_static` | `bool` | Java `static`, Python `@staticmethod`, Go package-level fn |
| `is_abstract` | `bool` | Java/TS `abstract`, Python `NotImplemented` stubs |
| `is_final` | `bool` | Java `final`, TS `readonly` |
| `is_property` | `bool` | Python `@property`, TS getter/setter |
| `is_generator` | `bool` | Python `yield`, TS `function*` |
| `is_const` | `bool` | Rust `const fn`, TS `const` assertion |
| `generic_bounds` | `string[]` | e.g. `["T: Send", "T: 'static"]` |

### Cyclomatic complexity

Cyclomatic complexity is computed during Pass 1 (synchronous parse) for all function and method nodes across all 5 supported languages. The formula is:

```
complexity = 1 + (number of decision points in the function body)
```

Decision points by language:

| Language | Counted nodes |
|----------|--------------|
| Rust | `if_expression`, `while_expression`, `for_expression`, `match_expression`, `match_arm`, `&&`, `\|\|`, `?` (try) |
| Python | `if_statement`, `elif_clause`, `while_statement`, `for_statement`, `except_clause`, `with_statement`, `boolean_operator` |
| TypeScript/JS | `if_statement`, `else_clause`, `while_statement`, `for_statement`, `for_in_statement`, `catch_clause`, `ternary_expression`, `&&`, `\|\|`, `??` |
| Go | `if_statement`, `else_clause`, `for_statement`, `case_clause`, `communication_case`, `&&`, `\|\|` |
| Java | `if_statement`, `else`, `while_statement`, `for_statement`, `enhanced_for_statement`, `catch_clause`, `&&`, `\|\|`, `ternary_expression` |

Complexity is exposed in:
- `symbol_context` response as `definition.complexity`
- `wiki_symbol` markdown header
- `gcx query wiki <name>` terminal output

A function with no branching has complexity `1`. Each decision point adds `1`.

---

## Data storage

All graph data is machine-local and never committed.

```
~/.local/share/gitcortex/{repo_id}/
    graph.kuzu           # KuzuDB database (all branches, table-namespaced)
    main.sha             # last indexed commit SHA for branch "main"
    feat__auth.sha       # last indexed SHA for branch "feat/auth"
```

`{repo_id}` is derived from the repo's origin URL or absolute path.

Branch names are sanitised for use as KuzuDB table prefixes: `/` → `__`, `-` → `_`, leading digits escaped.

---

## Environment variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Tracing log filter for `gcx`. E.g. `RUST_LOG=gitcortex_store=debug gcx serve`. |
| `NO_COLOR` | Disable ANSI colour output (same effect as `--color never`). |
| `CLICOLOR=0` | Disable colour (same as above). |
| `TERM=dumb` | Disable colour (same as above). |
| `GCX_STORE_PATH` | Override the default `~/.local/share/gitcortex` store directory. |
