# gitcortex

A code knowledge graph for Git repositories. GitCortex indexes your codebase on every commit and answers questions like "what calls this function?" or "what changed between these two branches?" — so your AI coding assistant can work with real structure instead of scanning raw files.

## Install

```bash
pip install gitcortex
# or
pipx install gitcortex
# or
uv tool install gitcortex
```

Supports macOS (Apple Silicon + Intel), Linux (x86_64 + arm64), and Windows (x64). No Rust or compiler required — a pre-built binary is bundled in the wheel.

## Quick start

```bash
cd your-repo
gcx init       # index the repo + install git hooks
gcx serve      # start the MCP server for your AI assistant
```

After `gcx init`, the graph updates automatically on every `git commit`, `merge`, `rebase`, and `checkout`. No manual re-runs needed.

## What it does

GitCortex builds a queryable graph of your codebase — functions, structs, classes, interfaces, call relationships, inheritance chains — and keeps it current automatically.

- **Works with**: Rust, Python, TypeScript, JavaScript, Go, Java
- **Integrates with**: Claude Code, Cursor, Windsurf, GitHub Copilot, Antigravity
- **Per-branch graphs**: switching branches gives you the graph for that branch instantly
- **Zero runtime dependency**: single self-contained binary, nothing else to install

## Commands

### `gcx init`

Index the current repo, install git hooks, and register the MCP server with your editor.

```bash
gcx init                     # auto-detects your editor
gcx init --editor cursor     # target a specific editor: claude, cursor, windsurf, copilot
gcx init --editor all        # write configs for every supported editor
```

### `gcx serve`

Start the MCP server so your AI assistant can query the knowledge graph.

```bash
gcx serve
```

Once running, your AI assistant has access to tools like `find_callers`, `lookup_symbol`, `list_definitions`, `trace_path`, and more.

### `gcx query`

Query the graph from the terminal without an AI assistant.

```bash
gcx query lookup-symbol MyStruct
gcx query find-callers process_request
gcx query list-definitions src/lib.rs
```

### `gcx blast-radius`

See which parts of the codebase are affected by changes between two branches — useful before merging.

```bash
gcx blast-radius --base main --head feat/my-feature
gcx blast-radius --base main --head feat/my-feature --format github-comment
```

### `gcx viz`

Open an interactive graph in your browser.

```bash
gcx viz                         # opens on port 5678
gcx viz --port 9000
gcx viz --branch feat/my-feature
gcx viz --format dot > graph.dot   # export as Graphviz DOT
```

### `gcx export`

Generate a readable Markdown map of the codebase at `.gitcortex/context.md`. The git hook keeps it up to date automatically.

```bash
gcx export
```

### `gcx status`

Show node and edge counts for the current branch.

```bash
gcx status
```

### `gcx clean`

Wipe the local graph store and re-index from scratch on the next commit.

```bash
gcx clean
```

## MCP tools available to your AI assistant

| Tool | What it answers |
|---|---|
| `lookup_symbol` | Where is `MyStruct` defined? |
| `find_callers` | What calls `process_request`? |
| `find_callees` | What does `handle_request` call? |
| `list_definitions` | What's defined in `src/auth.rs`? |
| `find_implementors` | What implements `AuthProvider`? |
| `trace_path` | How do you get from `main` to `validate_token`? |
| `find_unused_symbols` | What's never called (dead code candidates)? |
| `get_subgraph` | Everything within 2 hops of `UserService` |
| `detect_changes` | What changed + who's affected vs main? |
| `symbol_context` | Callers, callees, and uses for a symbol |

All tools accept an optional `branch` parameter.

## Configuration

`.gitcortex/config.toml` (committed to your repo, shared with your team):

```toml
[index]
languages = ["rust", "typescript", "python", "go"]
max_file_size_kb = 500
```

`.gitcortex/ignore` (`.gitignore` syntax — files to exclude from indexing):

```
target/
build/
**/*.generated.rs
```

## License

MIT — free for commercial and open-source use.

[GitHub](https://github.com/bharath03-a/GitCortex) · [Issues](https://github.com/bharath03-a/GitCortex/issues)
