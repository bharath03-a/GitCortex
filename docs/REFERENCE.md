# GitCortex Reference

Complete reference for the `gcx` CLI, MCP server tools, configuration, and graph schema.

---

## Table of Contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [CLI reference](#cli-reference)
  - [gcx init](#gcx-init)
  - [gcx deinit](#gcx-deinit)
  - [gcx serve](#gcx-serve)
  - [gcx index](#gcx-index)
  - [gcx hook](#gcx-hook)
  - [gcx query](#gcx-query)
  - [gcx blast-radius](#gcx-blast-radius)
  - [gcx viz](#gcx-viz)
  - [gcx export](#gcx-export)
  - [gcx status](#gcx-status)
  - [gcx storage](#gcx-storage)
  - [gcx clean](#gcx-clean)
  - [gcx doctor](#gcx-doctor)
  - [gcx update](#gcx-update)
- [Recovery](#recovery)
- [MCP server](#mcp-server)
  - [Editor setup](#editor-setup)
  - [Compact mode](#compact-mode)
  - [Tool reference](#tool-reference)
  - [Prompt reference](#prompt-reference)
  - [Slash commands (Claude Code)](#slash-commands-claude-code)
- [Configuration reference](#configuration-reference)
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

### Homebrew

```bash
brew install bharath03-a/tap/gitcortex
```

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

# 2. Index and install hooks without configuring any editor (explicit CLI-only setup)
gcx init --editor none
# Or opt into repository-local MCP + rules setup for one editor:
# gcx init --editor codex

# 3. Verify setup
gcx doctor

# 4. Query the graph
gcx query lookup-symbol MyStruct
gcx query find-callers run --depth 2

# 5. Start the MCP server (editors do this automatically)
gcx serve
```

After `gcx init`, every `git commit`, `git pull`, `git rebase`, and `git checkout` keeps the committed graph current automatically.

---

## CLI reference

### Global flags

These flags apply to every subcommand.

| Flag | Values | Default | Description |
|------|--------|---------|-------------|
| `--color` | `auto`, `always`, `never` | `auto` | ANSI colour output. Also respects `NO_COLOR`, `CLICOLOR=0`, `TERM=dumb`. |

---

### gcx init

Install Git hooks and run the initial full index. Use `--editor none` for explicit CLI-only setup everywhere (including interactive terminals).

```
gcx init [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--ci` | Write `.github/workflows/gcx-blast-radius.yml` (two jobs: read-only `blast-radius` + same-repo `comment`). |
| `--editor <EDITOR>` | Select `none`, `auto`, `claude`, `cursor`, `windsurf`, `copilot`, `antigravity`, `codex`, or `all`. On the **first** interactive TTY run without `--editor`, `gcx init` opens a one-time checkbox chooser (remembered per repo per machine); non-TTY runs without `--editor` default to `none`. |
| `--global-editor-config` | Explicitly allow changes to editor configuration outside the repository (required for `windsurf`, optional for `claude` and `antigravity`). |
| `--shared-git-hooks` | Explicitly allow changes when `core.hooksPath` points outside the repository and Git common directory. |

**What `gcx init` does:**

1. Resolves the active Git hooks directory with `git rev-parse --git-path hooks` and adds managed, non-blocking hook blocks (`post-commit`, `post-merge`, `post-rewrite`, `post-checkout`).
2. Runs a full index of the current `HEAD` and writes `.gitcortex/ignore` and `.gitcortex/AGENT_GUIDE.md`.
3. Writes repository-local MCP configuration, mirrors `.gitcortex/AGENT_GUIDE.md` into the editor's managed rules block, and installs non-blocking `PreToolUse` Read hooks (`gcx __hook-context`) for Claude Code, Codex, and Antigravity when those editors are selected.
4. Writes `.claude/commands/gcx/` slash commands only when Claude is selected.

**Examples:**

```bash
gcx init --editor none          # explicit CLI-only setup (no editor configuration)
gcx init                        # interactive first-TTY chooser (CLI-only in non-TTY / CI)
gcx init --editor claude        # repository-local Claude Code setup + Read hook
gcx init --editor auto          # explicitly request environment detection
gcx init --editor codex         # repository-local Codex setup + Read hook
gcx init --editor windsurf --global-editor-config
gcx init --editor all           # all repository-local integrations
gcx init --ci --editor none     # CLI-only + GitHub Actions blast-radius workflow
```

---

### gcx deinit

Remove GitCortex-owned hooks and repository-local editor integrations without disturbing unrelated configuration.

```bash
gcx deinit --dry-run
gcx deinit
gcx deinit --global-editor-config
gcx deinit --shared-git-hooks
gcx deinit --purge
```

By default, graph data and `.gitcortex/` are retained. `--global-editor-config` explicitly permits removing global MCP entries, and `--purge` also deletes this repository's local graph state.

---

### gcx serve

Start an MCP stdio proxy (the transport used by editor clients). The proxy connects to a repository-scoped local daemon that opens KuzuDB once and multiplexes concurrent editors.

```
gcx serve [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--full` | Expose all individual tools in addition to the compact `gcx` dispatch tool. Compact mode is the default. |

**Notes:**
- The first client starts the repository daemon; compact and full clients may then run concurrently.
- The daemon socket is local-only, permissioned for the current user, and removed shortly after the final client disconnects.
- The daemon tracks branch changes while running. Tools without an explicit `branch` use the currently checked-out branch.
- Editors launch `gcx serve` automatically via their MCP config. You rarely need to run this manually.

**Examples:**

```bash
gcx serve          # compact single-dispatch surface (default)
gcx serve --full   # compact dispatch plus all individual tools
```

---

### gcx index

```bash
gcx index --branch <name>
```

Synchronously index a local branch from committed git objects without checking
it out. Prints the branch's total node and edge counts, marked `unchanged` when
the branch was already current. `<name>` must be a local branch; revision syntax
such as `main~1` is refused, and remote-only branches need a local branch first.
While an MCP session owns the graph this command cannot open it, but that session
can index the branch: call any tool with `branch` set to it.

Uses the branch's stored `.gitcortex/ignore`; uncommitted files do not affect the
result. Binary and oversized sources are excluded using the same limits as
checkout indexing. A tracked source symlink is skipped here, since a tree holds
only its target path. Checkout indexing skips it too, logging one warning per
skipped path, and never follows a symlink, so no read can leave the repository.

MCP requests for an unindexed or stale local branch outside the checkout start
a bounded background tree index. Readiness remains under `index`:
`index.state = "indexing"` and `index.next_action` asks the caller to retry.
One job runs at a time; duplicate requests share the pending job. A failed job
reports `missing` with its error and can be retried after 60 seconds. Unknown
local branches report `missing` immediately without scheduling work; a key that
is not a local branch but has an indexed commit, such as a detached short SHA,
answers from that commit without a job. A job never writes the checked-out branch,
whose graph belongs to the watcher: if the branch becomes checked out or its SHA
moves while the job runs, the job stops without applying. An idle daemon waits up
to 10 minutes for active jobs before exiting. While a job writes its result the
store is busy, and requests report `indexing` rather than waiting.

Commit readiness compares the persisted SHA and dirty marker with the requested
commit. Failed applies remain `unverified`; successful tree indexing publishes
`ready` with `working_tree_verified = false`, since it certifies committed blobs.
An empty or stale graph never establishes that dependencies are absent.

#### MCP readiness states and working-tree overlay certification

Every MCP tool response carries an `index` metadata envelope with `state`,
`branch`, `checkout_branch`, `checkout_revision`, `target_revision`,
`indexed_revision`, `working_tree_verified`, and `next_action`:

| `index.state` | `working_tree_verified` | Meaning |
|---|---|---|
| `ready` | `true` | Checkout branch matches `HEAD` cleanly, **or** the dirty working-tree overlay matches the persisted `.overlay` checkpoint with no pending watcher events. |
| `ready` | `false` | Non-checkout branch (or commit tree index) matches its target commit SHA; working-tree files are not applicable. |
| `unverified` | `false` | Checkout has uncommitted edits not yet certified by a settled `.overlay` checkpoint, pending watcher events exist, hashing caps were exceeded, or a previous apply left `.dirty` without a matching checkpoint. |
| `stale` | `false` | Indexed revision differs from the target commit SHA. |
| `missing` | `false` | Branch has not been indexed yet (or background tree indexing failed). |
| `indexing` | `false` | Background tree indexing is currently running for the requested branch. |
| `degraded` | `false` | Live file watcher is unhealthy or reconciling after an error. |
| `unavailable` | `false` | Repository or store state could not be inspected. |

**Overlay checkpoint & skipped-file rule:**
- After a successful live watcher batch on a dirty checkout, the daemon atomically writes `<branch_key>.overlay` next to `<branch_key>.dirty` containing `{ head, branch_key, files: { path -> git_blob_sha1 | null }, ignore: git_blob_sha1 | null }`, where `ignore` is the hash of the working-tree `.gitcortex/ignore` the overlay was indexed under. The `.dirty` marker stays set until the working tree returns to a clean `HEAD`.
- `readiness::inspect` certifies a dirty checkout (`state = "ready"`, `working_tree_verified = true`) only when `.dirty` is set, no watcher events are pending, the checkpoint's `head` and `branch_key` match the checkout, its path set exactly equals `changed_sources(root)`, every file's current git-blob SHA-1 on disk matches the checkpoint (`null` for deleted tracked files whose path is absent on disk), and the current `.gitcortex/ignore` hash matches `ignore`.
- **Ignored and unsupported files** (`.gitcortex/ignore` patterns, `.git/`, `.gitcortex/`, `target/`, `node_modules/`, and non-indexed extensions) are excluded by `IncrementalIndexer::includes_path` from both `changed_sources(root)` and the checkpoint: they do not block certification and are never claimed as indexed. When a file already in the graph becomes ignored, its nodes are removed on its next change.
- **Untracked git-ignored files** (for example `.venv/**/*.py` or `.next/**/*.js`) are never indexed by the watcher and any nodes they left behind are removed; Git status never reports them, so they do not block certification. Tracked files matching a `.gitignore` pattern are still indexed.
- **Ignore-rule changes:** editing or deleting a committed `.gitcortex/ignore` keeps the checkout `unverified`, clean or dirty, until the change is committed or reverted. The watcher reconciles the whole graph when the rules file changes, and indexing a commit that changes the rules re-evaluates every path.
- **Source symlinks** are skipped, never followed. A dirty symlinked source has no recorded hash, so it keeps the checkout `unverified`.
- **Deleted tracked source files** remove graph nodes during the watcher batch and record `null` in the checkpoint; `inspect` verifies the path no longer exists on disk.
- **Oversized (`> 512 KiB`) or binary/non-UTF-8 source files** with supported extensions are skipped by the AST parser, so their symbols are absent from the graph while they remain in `changed_sources(root)`. The watcher removes any `.overlay` checkpoint and `inspect` keeps the checkout `unverified` (`working_tree_verified = false`).
- **Inspection bounds:** `inspect` hashes at most 64 changed files, at most 512 KiB per file, and at most 4 MiB total bytes (checkpoint JSON capped at 256 KiB). Exceeding any cap leaves `state = "unverified"`.

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

All query subcommands accept `--branch <NAME>` (default: current checkout branch, falling back to `main`).

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
gcx query find-callers <NAME> [--depth <1-5>] [--limit <N>] [--budget-tokens <N>] [--format <text|agent-json>] [--branch <BRANCH>]
```

Find callers of a function. `--depth 1` (default) returns direct callers; higher values walk multiple hops.

| Flag | Default | Description |
|------|---------|-------------|
| `--depth` | `1` | Hops to walk up the call graph (`1`–`5`) |
| `--limit` | `25` | Maximum caller rows returned |
| `--budget-tokens` | `600` | Global response budget used by `agent-json` output |
| `--format` | `text` | `text` (human-readable) or `agent-json` |

```bash
gcx query find-callers validate_token
gcx query find-callers apply_diff --depth 3
gcx query find-callers apply_diff --format agent-json --limit 10
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
gcx query symbol-context <NAME> [--limit <N>] [--budget-tokens <N>] [--format <text|agent-json>] [--branch <BRANCH>]
```

360° view: definition location + callers + callees + type usages. On an ambiguous short name, returns qualified candidates instead of relations.

| Flag | Default | Description |
|------|---------|-------------|
| `--limit` | `25` | Max callers/callees/used_by rows returned per relation |
| `--budget-tokens` | `800` | Global response budget used by `agent-json` output |
| `--format` | `text` | `text` (human-readable) or `agent-json` (structured, for AI clients) |

```bash
gcx query symbol-context apply_diff
gcx query symbol-context apply_diff --format agent-json --limit 10
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
gcx query trace-path <FROM> <TO> [--format <text|agent-json>] [--branch <BRANCH>]
```

Find the shortest call path between two functions (up to 6 hops).

```bash
gcx query trace-path main apply_diff
gcx query trace-path main apply_diff --format agent-json
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
gcx query get-subgraph <NAME> [--depth <1-5>] [--direction <in|out|both>] [--limit <N>] [--budget-tokens <N>] [--format <text|agent-json>] [--branch <BRANCH>]
```

Return all nodes and edges within N hops of a seed symbol.

| Flag | Default | Description |
|------|---------|-------------|
| `--depth` | `1` | Hops to expand from seed (`1`–`5`) |
| `--direction` | `both` | `in` (ancestors), `out` (descendants), `both` |
| `--limit` | `20` | Maximum nodes returned |
| `--budget-tokens` | `400` | Global response budget used by `agent-json` output |
| `--format` | `text` | `text` or `agent-json` |

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
gcx query search <QUERY> [--kind <KIND>] [--file <PATH>] [--limit <N>] [--offset <N>] [--include-details] [--budget-tokens <N>] [--format <text|agent-json>] [--branch <BRANCH>]
```

Fuzzy search over name + qualified path. Ranked: exact → prefix → substring. Functions and structs are ranked higher.

| Flag | Default | Description |
|------|---------|-------------|
| `--kind` | none | Optional node kind filter |
| `--file` | none | Optional file path prefix/substring filter |
| `--limit` | `10` | Maximum results per page |
| `--offset` | `0` | Pagination offset |
| `--include-details` | `false` | Include extra signature/doc detail fields |
| `--budget-tokens` | `600` | Global response budget used by `agent-json` output |
| `--format` | `text` | `text` or `agent-json` |

```bash
gcx query search auth --limit 20
gcx query search auth --limit 10 --format agent-json
gcx query search "graph store" --branch main
```

#### gcx query tour

```
gcx query tour [--seed <NAME>] [--limit <N>] [--branch <BRANCH>]
```

Generate a guided tour through the codebase (`--limit` defaults to `6`).

- Without `--seed`: picks highest-centrality public entry points.
- With `--seed`: BFS-walks outward from that symbol along call edges.

```bash
gcx query tour                      # global tour
gcx query tour --seed main          # tour from main()
gcx query tour --seed Auth --limit 20
```

---

### gcx blast-radius

Show the blast radius of changes between two branches or refs.

```
gcx blast-radius [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--base <BRANCH>` | current branch (fallback `main`) | Branch or ref to compare against (pass `--base main` explicitly when comparing a feature branch against `main`) |
| `--head <BRANCH>` | `HEAD` | Branch or ref with changes |
| `--depth <N>` | `2` | BFS depth for transitive caller discovery |
| `--format <FORMAT>` | `text` | Output format: `text`, `json`, `github-comment` |

**Formats:**
- `text` — human-readable terminal output
- `json` — machine-readable `{ base, head, risk, changed_count, affected_count, changed_nodes, affected_callers }` object
- `github-comment` — Markdown suitable for posting as a sticky PR comment (used by the CI workflow)

**Examples:**

```bash
gcx blast-radius --base main --head HEAD --format text
gcx blast-radius --base main --head feat/auth
gcx blast-radius --base main --head HEAD --format json | jq '.risk'
gcx blast-radius --base main --head HEAD --format github-comment > /tmp/comment.md
```

---

### gcx viz

Visualise the knowledge graph in the embedded local web UI or emit graph output.

```
gcx viz [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--branch <BRANCH>` | current branch (fallback `main`) | Branch to visualise |
| `--format <FORMAT>` | `web` | `web` (embedded interactive UI), `html` (legacy HTML + CDN), `dot` (Graphviz DOT), `svg`, `graphml`, `cypher` |
| `--port <PORT>` | `5678` | HTTP port for `web` mode (if occupied, `gcx viz` reports the port and suggests `gcx viz --port <free-port>`) |

**Embedded `web` UI vs legacy `html` vs `gcx export`:**
- `gcx viz` (`--format web`) serves the local React/Cosmograph explorer with all JS, CSS, and fonts embedded inside the `gcx` binary and readable typography (body ≥14px, UI text ≥12px, AA contrast). It holds an exclusive lock on the repository graph store while running (press `Ctrl-C` to stop; standalone CLI readers report the active owner PID and retry guidance while it runs).
- `gcx viz --format html` writes a legacy single-file HTML page that embeds graph JSON inline but loads `vis-network` from a remote CDN (offline/cached availability is not guaranteed; a standalone offline static bundle is not shipped in this release).
- For Mermaid diagrams, versioned `graph-json` v1, or scoped exports (`--seed`, `--depth`, `--direction`, `--edge-kinds`), use [`gcx export`](#gcx-export) (for example `gcx export --format mermaid`).

**Examples:**

```bash
gcx viz                         # open embedded UI at http://127.0.0.1:5678 (Ctrl-C to stop)
gcx viz --format dot | dot -Tsvg > graph.svg
gcx viz --format svg > graph.svg
gcx viz --format html > graph.html
gcx viz --branch feat/auth --port 8080
```

---

### gcx export

Export the knowledge graph as a codebase map, versioned graph JSON, Mermaid, DOT/SVG/GraphML/Cypher/HTML, or a `CLAUDE.md` symbol table. See [EXPORT.md](EXPORT.md) for full format examples and the `graph-json` schema.

```
gcx export [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--branch <BRANCH>` | current branch | Branch to export (validated against Git before opening the store) |
| `--format <FORMAT>` | `markdown` | `markdown`, `json` (legacy), `graph-json` (v1 schema), `mermaid`, `dot`, `svg`, `graphml`, `cypher`, `html` |
| `--seed <SEED>` | none | Scope traversal around one symbol (node ID, exact `qualified_name`, or unique short `name`) |
| `--depth <N>` | `2` | BFS hop depth from `--seed` (`0`–`5`) |
| `--direction <DIR>` | `both` | Traversal direction from `--seed`: `up` (incoming), `down` (outgoing), `both` |
| `--edge-kinds <KINDS>` | all | Comma-separated edge kinds allowed during traversal and output (e.g. `calls,imports`) |
| `--max-nodes <N>` | `5000` (`100` for `mermaid`) | Maximum nodes to emit (seed and closest hops preserved first) |
| `--out <PATH>` | stdout (`.gitcortex/context.md` for `markdown`) | Write output to a file path instead of stdout |
| `--allow-stale` | false | Export even when index readiness is not `ready` (records state in `meta.readiness`) |
| `--force` | false | Overwrite an existing `--out` path |
| `--claude-md` | false | Upsert a top-symbols table into `CLAUDE.md` (overrides `--format`) |
| `--top <N>` | `40` | Number of top-ranked symbols to inject with `--claude-md` |

**Exit codes (scoped graph formats):** `0` success, `1` operational/store error, `2` invalid input (unknown/ambiguous `--seed`, invalid `--branch`, unknown `--edge-kinds`, or argument error), `3` index not ready unless `--allow-stale` is set.

**Format: markdown**

Writes `.gitcortex/context.md` (or `--out`) — a structured Markdown map of all files, structs, and functions.

**Format: json (legacy)**

Emits `{ branch, sha, symbols[], edges[] }` to stdout (or `--out`). Each symbol carries `id`, `name`, `qualified_name`, `kind`, `file`, `line`, `visibility`. Edges reference symbol `id`s.

**Format: graph-json**

Emits a versioned `gitcortex.graph` v1 document conforming to [`docs/schemas/gitcortex-graph-v1.schema.json`](schemas/gitcortex-graph-v1.schema.json) with readiness metadata, scoping parameters, truncation totals, and zero source text.

**Flag: --claude-md**

Upserts a compact, centrality-ranked symbol table between `<!-- gcx:symbols start -->` / `<!-- gcx:symbols end -->` markers in `CLAUDE.md`. AI assistants read the top-N most-referenced symbols (name → `file:line`) with zero tool calls.

**Examples:**

```bash
gcx export                                                            # writes .gitcortex/context.md
gcx export --format graph-json --out graph.json                       # versioned graph v1 JSON
gcx export --format mermaid --seed verify_token --direction up --depth 2
gcx export --format json > symbols.json                               # legacy flat symbols + edges
gcx export --claude-md --top 60                                       # inject top 60 symbols into CLAUDE.md
```

---

### gcx status

Show readiness state and indexed node/edge counts for a branch (defaults to the current checkout branch).

```
gcx status [--branch <BRANCH>]
```

```
state:      ready
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
note:       Verify source and tests; ignored, oversized, unsupported, and dynamic code can be absent from the static graph.
```

**Exit codes:** `0` (`ready` or `unverified` — `unverified` prints a warning that counts are observed only and working-tree coverage is unverified), `1` operational/store/schema error, `2` invalid ref or CLI arguments, `3` not ready (`missing`, `stale`, `indexing`, or `degraded`).

---

### gcx storage

Inspect and maintain machine-local graph storage across the current repository or all repositories under the local data root. All destructive subcommands (`prune`, `gc`, `compact`) run as a **dry-run** unless `--apply` is passed.

```
gcx storage <SUBCOMMAND>
```

| Subcommand | Flags | Description |
|------------|-------|-------------|
| `list` | `[--all] [--json]` | Show disk usage, status, and branch inventory for the current repository (or every local store with `--all`). |
| `prune` | `[--orphaned] [--backups] [--unknown-older-than <DAYS>] [--apply]` | Plan or remove orphaned repo stores, historical `*.bak-*` / `graph.kuzu.pre-compact-*` backups, or old metadata-less stores. |
| `gc` | `[--apply]` | Plan or drop deleted branch tables and stale detached snapshots for the current repository. |
| `compact` | `[--apply]` | Plan or reclaim unused space inside `graph.kuzu` via atomic payload swap. |

```bash
gcx storage list --all
gcx storage prune --orphaned --backups        # dry-run plan
gcx storage prune --orphaned --backups --apply
gcx storage gc                                # dry-run plan
gcx storage compact --apply
```

---

### gcx clean

Wipe the graph store for this repo. The next `gcx init --editor none` (or `gcx init`) triggers a fresh full index.

```
gcx clean
```

> **Irreversible.** All branch graphs for this repo are deleted. Before running `gcx clean` after an upgrade or schema mismatch, inspect `gcx --version`, `gcx doctor`, and `gcx storage list`, stop active owners, and back up the store directory if needed.

---

### gcx doctor

Diagnose setup issues: hooks installed, store accessible, index readiness, and MCP registration. Exits non-zero when required checks fail.

```
gcx doctor
```

```
  [ok] gcx on PATH (/usr/local/bin/gcx)
  [ok] git repository detected
  [ok] post-commit hook installed
  [ok] post-merge hook installed
  [ok] post-rewrite hook installed
  [ok] post-checkout hook installed
  [ok] graph store accessible  (1842 nodes, 4217 edges on main)
  [ok] index is current  (HEAD abc1234)
  [ok] MCP Claude Code: registration found (.mcp.json — reload editor; registration alone does not confirm an active connection)
  [--] MCP Cursor: not configured (run: gcx init --editor cursor)
```

`gcx doctor` checks the six supported editors (`claude`, `codex`, `cursor`, `antigravity`, `windsurf`, `copilot`). Agy CLI is omitted/unsupported.

---

### gcx update

Check for a newer release and print the right update command for your install method.

```
gcx update
```

---

## Recovery

`gcx deinit` only removes hooks and editor files — it does **not** repair or restore a graph store. Git hooks only update the graph on committed Git operations (`post-commit`, `post-merge`, `post-rewrite`, `post-checkout`); they do **not** verify uncommitted working-tree edits.

| Symptom / state | Safe check | Exact command | Expected state after recovery |
| --- | --- | --- | --- |
| Outside a Git repository (`exit 1`) | Confirm repository path (`pwd`) | `cd <repository> && gcx doctor` | Repository detected; `gcx doctor` exits `0` once initialized and clean (no store created outside repo) |
| Existing valid unindexed branch (`status` exits `3`, `missing`) | Validate local branch exists (`git show-ref --verify refs/heads/<branch>`) | `gcx index --branch <branch>` | Branch indexed from committed objects without checkout; `index.state = "ready"` (`working_tree_verified = false`) |
| Missing or deleted graph store (`status` exits `3`, `missing`) | Check active owners (`gcx doctor` / `gcx storage list`) and stop editor MCP sessions or `gcx viz` | `gcx init --editor none` | Local store rebuilt from `HEAD`; `gcx status` exits `0` (`state: ready` on clean tree) |
| Stale committed index (`status` exits `3`, `stale`) | Compare indexed SHA vs `HEAD` (`gcx doctor`) | `gcx hook && gcx doctor` | Incremental diff applied to `HEAD`; `gcx status` and `gcx doctor` exit `0` (`state: ready`) |
| Working-tree `unverified` (`status` exits `0` warning; `doctor` exits `1`) | Inspect dirty code/doc files (`git status`); verify changes in source and tests | Commit intended changes (no automatic commit or blanket ignore), or let live `gcx serve` settle its `.overlay` checkpoint | Clean checkout or settled overlay returns `index.state = "ready"` (`working_tree_verified = true`) |
| Incompatible schema / upgrade failure (`status` or `query` exits `1`, store preserved) | Record `gcx --version`, run `gcx doctor` and `gcx storage list`, stop active owners (editor MCP / `gcx viz`), and back up the store directory if needed | `gcx init --editor none` (or explicitly destructive `gcx clean && gcx init --editor none` only when rebuilding local graph data is intended — never blindly delete stores) | Fresh store created at current schema version; `gcx status` exits `0` (`state: ready`) |

---

## MCP server

The MCP server exposes the knowledge graph to AI coding assistants via the [Model Context Protocol](https://modelcontextprotocol.io).

### Editor setup

`gcx init --editor <name>` writes repository-local configuration for the selected editor and mirrors `.gitcortex/AGENT_GUIDE.md` into that editor's managed rules file. Global configuration requires `--global-editor-config`. **Agy CLI** (`agy-cli`) is unsupported and omitted from both `gcx init --editor` and `gcx doctor`.

| Editor | `--editor` token | MCP config path (scope) | Read hook / rules | Trust / reload step | Verification status |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `claude` | `.mcp.json` (local; `~/.claude.json` with `--global-editor-config`) | `CLAUDE.md` + `PreToolUse` `Read` (`gcx __hook-context`, non-blocking) | Trust workspace MCP / reconnect (`/mcp`) | Generated config + MCP protocol + Read-hook automated; real client discovery not exercised |
| Codex | `codex` | `.codex/config.toml` (local) | `AGENTS.md` + `PreToolUse` `view_file` (`gcx __hook-context`, non-blocking) | Restart / reload Codex session | Generated config + MCP protocol + Read-hook automated; real client discovery not exercised |
| Cursor | `cursor` | `.cursor/mcp.json` (local) | `.cursor/rules/gitcortex.mdc` (MCP-only) | Enable / reload `gitcortex` in Cursor MCP settings | Generated config + MCP protocol automated; real client discovery not exercised |
| Antigravity | `antigravity` | `.agents/plugins/gitcortex/mcp_config.json` (local plugin; `~/.antigravity/mcp.json` with `--global-editor-config`) | `.agents/plugins/gitcortex/rules/AGENTS.md` + `PreToolUse` `Read` (`gcx __hook-context`, non-blocking) | Trust / reload local plugin in Antigravity | Generated config + MCP protocol + Read-hook automated; real client discovery not exercised |
| Windsurf | `windsurf` | `~/.codeium/windsurf/mcp_config.json` (global; requires `--global-editor-config`) | `.windsurfrules` (MCP-only) | Reload Windsurf MCP servers | Generated config + MCP protocol automated; real client discovery not exercised |
| GitHub Copilot | `copilot` | `.vscode/mcp.json` (local) | `.github/copilot-instructions.md` (MCP-only) | Trust workspace and start `gitcortex` in VS Code MCP panel | Generated config + MCP protocol automated; real client discovery not exercised |

#### Claude Code

Project `.mcp.json`, plus optional `~/.claude.json` with `--global-editor-config`:

```json
{
  "mcpServers": {
    "gitcortex": {
      "command": "gcx",
      "args": ["serve"]
    }
  }
}
```

#### Cursor

Repository-local `.cursor/mcp.json` uses the same `mcpServers` entry as Claude (MCP-only; no Read hook).

#### Antigravity

Repository-local plugin at `.agents/plugins/gitcortex/mcp_config.json` (plus optional `~/.antigravity/mcp.json` when `--global-editor-config` is supplied) and a non-blocking `PreToolUse` `Read` hook.

#### Windsurf

Windsurf requires a global MCP file (`~/.codeium/windsurf/mcp_config.json`). GitCortex writes it only when `--global-editor-config` is supplied (MCP-only; no Read hook).

#### GitHub Copilot in VS Code

Repository-local `.vscode/mcp.json` (MCP-only; no Read hook):

```json
{
  "servers": {
    "gitcortex": {
      "type": "stdio",
      "command": "gcx",
      "args": ["serve"]
    }
  }
}
```

#### Codex (OpenAI)

Repository-local `.codex/config.toml` (plus `AGENTS.md` rules and a non-blocking `PreToolUse` `view_file` hook):

```toml
[mcp_servers.gitcortex]
command = "gcx"
args = ["serve"]
startup_timeout_sec = 30
```

---

### Compact mode

`gcx serve` exposes the compact `gcx` dispatch tool by default. `gcx serve --full` additionally exposes every individual tool.

- Reduces per-turn MCP schema overhead ~95%
- All queries still work via the single `gcx` tool with tool arguments nested under `params`
- Every tool response includes an `index` readiness envelope (`index.state`, `index.working_tree_verified`, `index.next_action`)
- MCP prompts may not be available in clients that only load exposed tools

All generated editor configurations use compact mode. Add `--full` only for clients that require individual tool schemas.

**Compact MCP smoke call:**

```json
{
  "name": "gcx",
  "arguments": {
    "action": "lookup_symbol",
    "params": { "name": "MySymbol" }
  }
}
```

Inspect the returned `index.state` (`ready` vs `unverified` / `stale` / `missing`) before trusting an empty symbol or caller result.

---

### Tool reference

All tools accept an optional `branch` parameter. When omitted, the server follows the currently checked-out branch.

---

#### `gcx` — single dispatch

The compact server exposes only this tool. Use it to keep per-turn schema overhead minimal.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `action` | `string` | yes | One of: `lookup_symbol`, `find_callers`, `pre_edit_impact`, `find_callees`, `find_unused_symbols`, `get_subgraph`, `search_code`, `start_tour`, `wiki_symbol`, `trace_path`, `list_definitions`, `symbol_context`, `list_symbols_in_range`, `graph_stats`, `ast_search`, `type_hierarchy`, `find_importers`, `find_type_usages`, `module_dependencies`, `get_call_sites`, `find_god_nodes`, `find_clusters`, `find_cycles`, `health_report` |
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

#### `pre_edit_impact`

Blast radius check to run *before* editing, renaming, or removing a function or method's signature or behavior — same response shape as `find_callers`, called with the same parameters. Wraps `find_callers` behind a tool identity whose description nudges pre-edit use, so the assistant checks impact before writing the edit instead of only after.

**Parameters:** same as [`find_callers`](#find_callers).

**Returns:** same as [`find_callers`](#find_callers).

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
| `status` | `string` | `Ok`, `Ambiguous`, or `NotFound` |
| `symbol` | `object \| null` | Resolved symbol (present when `status` is `Ok`) |
| `candidates` | `array` | Qualified candidates to disambiguate, populated when `status` is `Ambiguous` |
| `callers` | `array` | Nodes that call this symbol |
| `callees` | `array` | Nodes this symbol calls |
| `used_by` | `array` | Nodes that reference this symbol as a type |
| `coverage` | `object` | `total`, `returned`, `truncated`, `confidence_mix` |

On `Ambiguous`, `callers`/`callees`/`used_by` are empty — resolve to a qualified name via `candidates` and re-query.

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

#### `graph_stats`

Aggregate counts for the graph — orientation before drilling in.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `branch` | `string` | no | current | — |

**Returns:** `{ branch, total_nodes, total_edges, nodes_by_kind[], edges_by_kind[] }`. Each `*_by_kind` entry is `{ kind, count }`, sorted by count descending.

**Example:**

```json
{}
```

---

#### `ast_search`

Find symbols by structural attributes rather than name. At least one filter is required.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `kind` | `string` | no | NodeKind: `function`, `method`, `struct`, `trait`, `interface`, `enum`, `constant`, `type_alias`, `property`, `macro`, `annotation`, `enum_member`, `module`, `file`, `folder` |
| `is_async` | `boolean` | no | Match only async (`true`) or only non-async (`false`) symbols |
| `visibility` | `string` | no | `pub`, `pub_crate`, or `private` |
| `min_complexity` | `integer` | no | Inclusive lower bound on cyclomatic complexity (symbols without complexity excluded) |
| `max_complexity` | `integer` | no | Inclusive upper bound on cyclomatic complexity |
| `name_contains` | `string` | no | Case-insensitive name substring |
| `annotation` | `string` | no | Annotation/decorator name substring: `Test` → `@Test`, `route` → `@app.route`, `derive` → `#[derive(...)]` |
| `limit` | `integer` | no | Max results (default 30, capped at 200) |
| `branch` | `string` | no | — |

**Returns:** `{ branch, results[], returned }`. Each result includes kind, name, qualified_name, file, start_line, visibility, is_async, complexity.

**Example:** all async methods:

```json
{ "kind": "method", "is_async": true }
```

complex functions:

```json
{ "kind": "function", "min_complexity": 10 }
```

---

#### `type_hierarchy`

Map both directions of a type's relationships in one call.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Type name (struct/class/trait/interface) |
| `branch` | `string` | no | — |

**Returns:** `{ type, supertypes[], subtypes[] }`. `supertypes` = types it implements/extends; `subtypes` = types that implement/extend it.

**Example:**

```json
{ "name": "GraphStore" }
```

---

#### `find_importers`

Find which files/modules import a given symbol (follows `Imports` edges). Useful before renaming or moving a symbol.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Imported symbol name (unqualified) |
| `branch` | `string` | no | — |

**Returns:** `{ symbol, importers[] }`. Importers are the importing module nodes.

> Resolves in-repo imports only; imports of external/stdlib symbols are not graphed.

**Example:**

```json
{ "name": "GraphStore" }
```

---

#### `find_type_usages`

Find functions/methods that reference a type as a parameter or return type (follows `Uses` edges). The type-level analogue of `find_callers`.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Type name (struct/class/trait/interface/enum) |
| `branch` | `string` | no | — |

**Returns:** `{ type, usages[] }`. Usages are the using functions/methods.

**Example:**

```json
{ "name": "Node" }
```

---

#### `module_dependencies`

List the in-repo modules a given module depends on, resolved by following its imports to the defining module of each imported symbol.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Module name (file stem, e.g. `tools` for `tools.rs`) |
| `branch` | `string` | no | — |

**Returns:** `{ module, depends_on[] }`. Each dependency is `{ name, file }`. Self-dependencies are excluded.

> Only intra-repo dependencies appear; imports of external/stdlib modules are not graphed.

**Example:**

```json
{ "name": "indexer" }
```

---

#### `get_call_sites`

Find every call site of a function: the calling symbol **and** the source line of each call. Where `find_callers` returns only the calling functions, this pinpoints the exact line each call happens on.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | yes | Function/method name |
| `branch` | `string` | no | — |

**Returns:** `{ function, call_sites[], count }`. Each call site is `{ caller, caller_kind, file, line, caller_start_line }` where `line` is the 1-indexed line of the call expression.

**Example:**

```json
{ "name": "apply_diff" }
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

`gcx init --editor claude` installs four slash commands into `.claude/commands/gcx/`:

| Command | Description |
|---------|-------------|
| `/gcx-lookup <name>` | Find all definitions matching a name |
| `/gcx-callers <name>` | Find all callers of a function |
| `/gcx-file <path>` | List all definitions in a file |
| `/gcx-blast-radius` | Show blast radius of changes vs main |

---

## Configuration reference

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

Rule changes apply to files already in the graph: a commit that changes this file re-evaluates every path, and a running `gcx serve` reconciles when the working-tree copy changes. While a committed copy has uncommitted edits, checkout readiness stays `unverified`.

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
| `annotations` | `string[]` | Decorator/annotation names, e.g. `["dataclass"]`, `["Override"]`, `["derive", "Serialize"]`. Captured even when the decorator is external/undefined in-repo. Query via `ast_search`'s `annotation` filter. |

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
- `wiki_symbol` markdown header
- `gcx query wiki <name>` terminal output

A function with no branching has complexity `1`. Each decision point adds `1`.

---

## Data storage

All graph data is machine-local and never committed.

```
<platform-data>/gitcortex/{repo_id}/
    graph.kuzu           # KuzuDB database (all branches, table-namespaced)
    schema_version       # graph schema version marker (mismatch refuses open without deleting)
    repo.json            # repository metadata & identity_version (1 = path-based store)
    branches.json        # tracked branch & detached snapshot metadata
    last_branch_gc       # timestamp of last branch garbage-collection pass
    main.sha             # last indexed commit SHA for branch "main"
    main.dirty           # present while the graph contains an uncommitted overlay or unverified apply
    main.overlay         # atomic JSON checkpoint { head, branch_key, files: path -> blob_hash | null, ignore }
    feat__auth.sha       # last indexed SHA for branch "feat/auth"
```

`{repo_id}` is a stable BLAKE3 digest of the absolute repository path (`identity_version: 1`, so each checkout or linked worktree maintains its own store; shared worktree store identity is not enabled in this release). Linux uses `$XDG_DATA_HOME` (default `~/.local/share`); macOS uses `~/Library/Application Support`. Existing macOS stores under `~/.local/share/gitcortex` remain supported.

Use `gcx storage list [--all]`, `gcx storage prune`, `gcx storage gc`, and `gcx storage compact` to inspect or reclaim disk usage safely (dry-run by default unless `--apply` is passed).

Branch names are sanitised for use as KuzuDB table prefixes: `/` → `__`, `-` → `_`, leading digits escaped.

---

## Environment variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Tracing log filter for `gcx`. E.g. `RUST_LOG=gitcortex_store=debug gcx serve`. |
| `NO_COLOR` | Disable ANSI colour output (same effect as `--color never`). |
| `CLICOLOR=0` | Disable colour (same as above). |
| `TERM=dumb` | Disable colour (same as above). |
| `GCX_STORE_PATH` | Override the platform data directory used for graph state. |
| `GCX_CACHE_PATH` | Override the platform cache directory used for model weights. |
| `GCX_DISABLE_SEMANTIC` | Disable local embedding-model initialization and use text/graph search only. Any set value enables this mode. |
| `XDG_DATA_HOME` | Linux/XDG durable data root when `GCX_STORE_PATH` is unset. |
| `XDG_CACHE_HOME` | Linux/XDG cache root when `GCX_CACHE_PATH` is unset. |
