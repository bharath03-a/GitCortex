# GitCortex 10-minute quickstart

This trial keeps all graph data on your machine and starts with no AI-editor configuration. It is designed to be easy to inspect and undo.

## 1. Install

Choose one supported package path:

```bash
brew install bharath03-a/tap/gitcortex
# or: pipx install gitcortex
# or: cargo install gitcortex --locked
```

Confirm the binary:

```bash
gcx --version
```

## 2. Build a local index without configuring an editor

From a Git repository:

```bash
cd path/to/repository
gcx init --editor none
gcx doctor
gcx status
```

`gcx init --editor none` explicitly runs CLI-only setup everywhere (including inside an interactive terminal). It creates the local index, writes `.gitcortex/ignore` and `.gitcortex/AGENT_GUIDE.md`, and installs four non-blocking Git hook blocks that keep committed state current. It does not modify editor files or global configuration.

> **Interactive vs non-interactive `gcx init`:** omitting `--editor` on the **first** run in an interactive terminal opens a one-time checkbox chooser (your choice is remembered per repo per machine). In CI, pipes, or any non-TTY shell, omitting `--editor` stays CLI-only (`none`).

Check `gcx status` before trusting empty results:

- **`ready`** (`exit 0`): the index matches `HEAD` cleanly (or a live `gcx serve` overlay checkpoint verified uncommitted edits).
- **`unverified`** (`exit 0` with warning; `gcx doctor` exits `1`): observed counts only — uncommitted source or Markdown changes are not yet certified. Review `git status`, inspect source/tests, and commit (or let a live `gcx serve` session settle).

## 3. Try three core workflows

Replace `<fragment>` and `<symbol>` with real symbol names from your repository:

```bash
# 1. Find likely definitions with compact, paged evidence
gcx query search <fragment> --limit 10 --format agent-json

# 2. Inspect one symbol's definition, callers, callees, and usages
gcx query symbol-context <symbol> --format agent-json

# 3. Check direct callers before editing
gcx query find-callers <symbol>
```

Compare your current checkout against `main` (`--base` defaults to the current checkout branch and `--head` defaults to `HEAD`, so pass `--base main --head HEAD` explicitly when comparing a feature branch against `main`):

```bash
gcx blast-radius --base main --head HEAD --format text
```

Useful follow-ups, visual inspection, and portable exports:

```bash
gcx query find-callees <symbol>
gcx query find-implementors <trait_or_interface>
gcx query tour --limit 12

# Interactive local graph UI (all JS/CSS/fonts embedded in `gcx`; Ctrl-C to stop)
gcx viz
# Legacy HTML export embeds graph JSON but loads vis-network from a remote CDN (not guaranteed offline)
# Portable readiness-checked Mermaid / versioned graph-json exports (see EXPORT.md):
gcx export --format graph-json --out graph.json
gcx export --format mermaid --seed <symbol> --direction up --depth 2
```

## 4. Connect one assistant only after the local checks pass

```bash
gcx init --editor codex
# accepted tokens: claude, cursor, windsurf, copilot, antigravity, codex, all

gcx doctor
```

Every installer mirrors the shared rules from `.gitcortex/AGENT_GUIDE.md` into that editor's managed rules block and registers the compact `gcx` MCP server (`gcx serve`). Because generated editor/Markdown files can leave the working tree dirty, review `git status` and commit or ignore them intentionally before expecting `ready` status. **Agy CLI** (`agy-cli`) is not a supported `--editor` token and is omitted from `gcx doctor`.

| Editor | `--editor` token | MCP config path (scope) | Read hook / rules | Trust / reload step | Verification status |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `claude` | `.mcp.json` (local; `~/.claude.json` with `--global-editor-config`) | `CLAUDE.md` + `PreToolUse` `Read` (`gcx __hook-context`, non-blocking) | Trust workspace MCP / reconnect (`/mcp`) | Generated config + MCP protocol + Read-hook automated; real client discovery not exercised |
| Codex | `codex` | `.codex/config.toml` (local) | `AGENTS.md` + `PreToolUse` `view_file` (`gcx __hook-context`, non-blocking) | Restart / reload Codex session | Generated config + MCP protocol + Read-hook automated; real client discovery not exercised |
| Cursor | `cursor` | `.cursor/mcp.json` (local) | `.cursor/rules/gitcortex.mdc` (MCP-only) | Enable / reload `gitcortex` in Cursor MCP settings | Generated config + MCP protocol automated; real client discovery not exercised |
| Antigravity | `antigravity` | `.agents/plugins/gitcortex/mcp_config.json` (local plugin; `~/.antigravity/mcp.json` with `--global-editor-config`) | `.agents/plugins/gitcortex/rules/AGENTS.md` + `PreToolUse` `Read` (`gcx __hook-context`, non-blocking) | Trust / reload local plugin in Antigravity | Generated config + MCP protocol + Read-hook automated; real client discovery not exercised |
| Windsurf | `windsurf` | `~/.codeium/windsurf/mcp_config.json` (global; requires `--global-editor-config`) | `.windsurfrules` (MCP-only) | Reload Windsurf MCP servers | Generated config + MCP protocol automated; real client discovery not exercised |
| GitHub Copilot | `copilot` | `.vscode/mcp.json` (local) | `.github/copilot-instructions.md` (MCP-only) | Trust workspace and start `gitcortex` in VS Code MCP panel | Generated config + MCP protocol automated; real client discovery not exercised |

`gcx doctor` prints `registration found (...)` when the parsed config entry is valid, and explicitly reminds you that registration alone does not confirm an active editor connection.

### Compact MCP smoke call

The default MCP server (`gcx serve`) exposes one compact `gcx` dispatch tool to reduce per-turn schema overhead (use `gcx serve --full` only when a client requires individual tool schemas). Always nest tool arguments inside `params` and inspect the returned `index` readiness envelope (`index.state`, `index.branch`, `index.working_tree_verified`, `index.next_action`):

```json
{
  "name": "gcx",
  "arguments": {
    "action": "lookup_symbol",
    "params": { "name": "MySymbol" }
  }
}
```

## 5. Optional PR blast-radius workflow

```bash
gcx init --ci --editor none
```

This writes `.github/workflows/gcx-blast-radius.yml` with two jobs:

1. `blast-radius` (`permissions: contents: read`): checks out `pull_request.head.sha` with `fetch-depth: 0` and `persist-credentials: false`, installs the matching `gitcortex` version (`--locked`), detaches and indexes `pull_request.base.sha` and `pull_request.head.sha` with `GCX_BRANCH_OVERRIDE=... gcx init --editor none`, runs `gcx blast-radius --format github-comment`, and uploads the `gitcortex-blast-radius` workflow artifact.
2. `comment` (`needs: blast-radius`, `if: github.event.pull_request.head.repo.full_name == github.repository`, `permissions: actions: read, pull-requests: write`): posts a sticky PR comment for same-repository branches. Fork PRs keep the artifact without attempting a write-token comment.

Review the generated workflow before committing it.

## 6. Recovery

`gcx deinit` only removes hooks and editor files — it does **not** repair or restore a graph store. Git hooks only update the graph on committed Git operations (`post-commit`, `post-merge`, `post-rewrite`, `post-checkout`); they do **not** verify uncommitted working-tree edits.

| Symptom / state | Safe check | Exact command | Expected state after recovery |
| --- | --- | --- | --- |
| Outside a Git repository (`exit 1`) | Confirm repository path (`pwd`) | `cd <repository> && gcx doctor` | Repository detected; `gcx doctor` exits `0` once initialized and clean (no store created outside repo) |
| Existing valid unindexed branch (`status` exits `3`, `missing`) | Validate local branch exists (`git show-ref --verify refs/heads/<branch>`) | `gcx index --branch <branch>` | Branch indexed from committed objects without checkout; `index.state = "ready"` (`working_tree_verified = false`) |
| Missing or deleted graph store (`status` exits `3`, `missing`) | Check active owners (`gcx doctor` / `gcx storage list`) and stop editor MCP sessions or `gcx viz` | `gcx init --editor none` | Local store rebuilt from `HEAD`; `gcx status` exits `0` (`state: ready` on clean tree) |
| Stale committed index (`status` exits `3`, `stale`) | Compare indexed SHA vs `HEAD` (`gcx doctor`) | `gcx hook && gcx doctor` | Incremental diff applied to `HEAD`; `gcx status` and `gcx doctor` exit `0` (`state: ready`) |
| Working-tree `unverified` (`status` exits `0` warning; `doctor` exits `1`) | Inspect dirty code/doc files (`git status`); verify changes in source and tests | Commit intended changes (no automatic commit or blanket ignore), or let live `gcx serve` settle its `.overlay` checkpoint | Clean checkout or settled overlay returns `index.state = "ready"` (`working_tree_verified = true`) |
| Incompatible schema / upgrade failure (`status` or `query` exits `1`, store preserved) | Record `gcx --version`, run `gcx doctor` and `gcx storage list`, stop active owners (editor MCP / `gcx viz`), and back up the store directory if needed | `gcx init --editor none` (or explicitly destructive `gcx clean && gcx init --editor none` only when rebuilding local graph data is intended — never blindly delete stores) | Fresh store created at current schema version; `gcx status` exits `0` (`state: ready`) |

To inspect or reclaim local storage across repositories, use `gcx storage list [--all]`, `gcx storage prune`, `gcx storage gc`, and `gcx storage compact` (all maintenance commands are dry-run until `--apply` is passed).

## 7. Roll back safely

Preview every integration file or hook block GitCortex would remove:

```bash
gcx deinit --dry-run
```

Then remove repository-local integration—including the generated
`.github/workflows/gcx-blast-radius.yml` when present—while retaining graph data:

```bash
gcx deinit
```

To remove repository configuration and machine-local graph data too:

```bash
gcx deinit --purge
```

Global editor entries and external shared hook paths are never changed unless their explicit permission flags are supplied.

## What to evaluate

During the trial, judge GitCortex on concrete workflows:

- Does a committed edit appear without a manual full rebuild?
- Do qualified symbols with duplicate short names return the correct scope?
- Can you find callers and impact with fewer exploratory reads?
- Do answers include enough file/line evidence to verify them?
- Does `gcx doctor` clearly diagnose a broken setup and exit non-zero?

For a team evaluation, continue with [PILOT-GUIDE.md](PILOT-GUIDE.md).
