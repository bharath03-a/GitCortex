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

`gcx init --editor none` creates the local index, writes `.gitcortex/ignore` and `.gitcortex/AGENT_GUIDE.md`, and installs non-blocking Git hook blocks that keep committed state current. It does not modify global editor configuration.

## 3. Try three core workflows

Replace the sample names with symbols from your repository:

```bash
# Find likely definitions with compact, paged evidence
gcx query search auth --limit 10 --format agent-json

# Understand one symbol before editing it
gcx query symbol-context handle_request --format agent-json

# Inspect change impact between branches
gcx blast-radius --base main --head HEAD --format text
```

Useful follow-ups:

```bash
gcx query find-callers handle_request
gcx query find-callees handle_request
gcx query find-implementors Repository
gcx query find-type-usages User
gcx query tour --limit 12
```

## 4. Connect one assistant only after the local checks pass

```bash
gcx init --editor codex
# alternatives: claude, cursor, windsurf, copilot, antigravity

gcx doctor
```

The default MCP server exposes one compact `gcx` dispatch tool to reduce fixed schema overhead. Use `gcx serve --full` only for clients that require individual tool definitions.

## 5. Optional PR blast-radius workflow

```bash
gcx init --ci --editor none
```

This writes `.github/workflows/gcx-blast-radius.yml`. The workflow indexes the exact immutable PR base and head revisions, uploads the report as a build artifact, and posts a sticky comment for same-repository branches. Fork PRs retain the artifact without attempting a write-token comment.

Review the generated workflow before committing it.

## 6. Roll back safely

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
