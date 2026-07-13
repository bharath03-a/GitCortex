#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path};

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;

use super::helpers::current_branch;

const HOOK_NAMES: &[(&str, &str)] = &[
    ("post-commit", "gcx hook\n"),
    ("post-merge", "gcx hook\n"),
    ("post-rewrite", "gcx hook\n"),
    ("post-checkout", "gcx hook --branch-switch\n"),
];

const HOOK_SHEBANG: &str =
    "#!/usr/bin/env sh\nset -e\nexport PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH\"\n";

const AGENT_GUIDE: &str = r#"# GitCortex Agent Guide

This repository is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
The MCP server is configured in `mcp.json` (or your editor's equivalent). Use these
tools to navigate the codebase — they read the live knowledge graph, not grep output.

## Available MCP Tools

| Tool | Description |
|------|-------------|
| `lookup_symbol(name)` | Find any struct, function, trait, or class by name |
| `find_callers(function_name)` | Who calls this function? |
| `find_callees(function_name, depth)` | What does this function call? (forward trace) |
| `list_definitions(file)` | All symbols defined in a file |
| `find_implementors(trait_name)` | Who implements this trait or interface? |
| `trace_path(from, to)` | All call paths from A to B |
| `list_symbols_in_range(file, start, end)` | Symbols overlapping a line range |
| `find_unused_symbols(branch)` | Dead code candidates (0 callers) |
| `get_subgraph(seed_name, depth, direction)` | Everything around a symbol |
| `detect_changes(base_branch)` | Changed symbols + blast radius vs another branch |

## Workflows

**Navigating unfamiliar code**
1. `lookup_symbol("ThingYouHeardAbout")` — confirm it exists and find the file
2. `list_definitions("path/to/file.rs")` — see the full shape of a file
3. `get_subgraph("ThingYouHeardAbout", 2, "both")` — visualise its neighbours

**Debugging**
1. `lookup_symbol("failingFn")` — confirm location
2. `find_callers("failingFn")` — walk up the call chain
3. Repeat until you reach an entry point

**Impact analysis before changing a public API**
1. `find_callers("publicFn")` — direct callers
2. `get_subgraph("publicFn", 3, "in")` — full upstream blast radius
3. `find_implementors("TraitYouAreChanging")` — all implementors that must change

**Safe refactoring**
1. `find_unused_symbols(branch)` — find candidates for deletion
2. `list_symbols_in_range(file, start, end)` — map a diff hunk to graph nodes
3. `trace_path(from, to)` — verify a code path before removing an intermediate

## Slash commands (Claude Code / Cursor)
- `/gcx-lookup <name>` — `lookup_symbol` with formatted output
- `/gcx-callers <name>` — `find_callers` with call chain summary
- `/gcx-file <path>` — `list_definitions` ordered by line
- `/gcx-blast-radius` — changed symbols + risk report vs main
"#;

pub fn install_hooks(repo_root: &Path) -> Result<usize> {
    let hooks_dir = repo_root.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let mut installed = 0;
    for (name, body) in HOOK_NAMES {
        let path = hooks_dir.join(name);
        if path.exists() {
            let existing = fs::read_to_string(&path)?;
            if existing.contains("gcx hook") {
                continue;
            }
            fs::write(&path, format!("{existing}\n{body}"))?;
        } else {
            fs::write(&path, format!("{HOOK_SHEBANG}{body}"))?;
        }
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms)?;
        }
        installed += 1;
    }
    Ok(installed)
}

pub fn initial_index(repo_root: &Path) -> Result<(usize, usize)> {
    let mut store = KuzuGraphStore::open(repo_root).context("failed to open graph store")?;
    let branch = current_branch(repo_root)?;
    let head_sha = head_sha(repo_root)?;
    let last_sha = store.last_indexed_sha(&branch)?;

    if last_sha.as_deref() != Some(head_sha.as_str()) {
        let indexer = IncrementalIndexer::new(repo_root).context("failed to create indexer")?;
        let (diff, indexed_head_sha) = indexer
            .run(last_sha.as_deref())
            .context("initial index failed")?;
        store.apply_diff(&branch, &diff).context("apply diff")?;
        store
            .set_last_indexed_sha(&branch, &indexed_head_sha)
            .context("persist sha")?;
    }

    let nodes = store.list_all_nodes(&branch)?.len();
    let edges = store.list_all_edges(&branch)?.len();
    if nodes == 0 && edges > 0 {
        anyhow::bail!(
            "graph store looks inconsistent: {nodes} nodes but {edges} edges on {branch}; run `gcx clean && gcx init`"
        );
    }
    Ok((nodes, edges))
}

fn head_sha(repo_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .context("git rev-parse HEAD failed")?;
    if !output.status.success() {
        anyhow::bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

const DEFAULT_GITCORTEX_IGNORE: &str = "\
target/\n\
build/\n\
dist/\n\
vendor/\n\
**/*.generated.rs\n\
**/*.pb.rs\n\
.fastembed_cache/\n\
";

/// Write `.gitcortex/ignore` if it does not already exist.
///
/// The default rules exclude common generated/build artefacts and the
/// fastembed model-weight cache directory that would otherwise appear in
/// the repo root if `cache_dir` were not set explicitly.
pub fn write_gitcortex_ignore(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".gitcortex");
    fs::create_dir_all(&dir)?;
    let path = dir.join("ignore");
    if !path.exists() {
        fs::write(path, DEFAULT_GITCORTEX_IGNORE).context("write .gitcortex/ignore")?;
    }
    Ok(())
}

pub fn write_agent_guide(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".gitcortex");
    fs::create_dir_all(&dir)?;
    let path = dir.join("AGENT_GUIDE.md");
    if !path.exists() {
        fs::write(path, AGENT_GUIDE).context("write AGENT_GUIDE.md")?;
    }
    Ok(())
}

const CLAUDE_STEERING_SNIPPET: &str = r#"
## GitCortex MCP Tools (prefer over grep for code navigation)

This repo is indexed by GitCortex. Use the `gcx` dispatch tool for all code
navigation instead of grep/file-read when the answer requires cross-file
relationships. The index is authoritative for call graphs, type hierarchies,
and blast radius — grep misses them entirely.

Quick reference:
- `gcx(action="find_callers", params={function_name:"X"})` — who calls X
- `gcx(action="get_subgraph", params={seed_name:"X", depth:2})` — X's neighbourhood
- `gcx(action="lookup_symbol", params={name:"X"})` — find X in the graph
- `gcx(action="find_cycles")` — circular import detection
- `gcx(action="start_tour")` — community-grouped codebase overview
- `gcx(action="search_code", params={query:"..."})` — semantic search

Run `gcx hook` if the index seems stale (uncommitted edits are not auto-indexed).
"#;

/// Write a short GitCortex steering snippet into `.claude/CLAUDE.md` so AI
/// agents prefer graph tools over grep. Appends to existing file; skips if
/// the snippet is already present.
pub fn write_claude_steering(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".claude");
    fs::create_dir_all(&dir)?;
    let path = dir.join("CLAUDE.md");
    let existing = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    if existing.contains("GitCortex MCP Tools") {
        return Ok(());
    }
    let mut content = existing;
    content.push_str(CLAUDE_STEERING_SNIPPET);
    fs::write(path, content).context("write .claude/CLAUDE.md")?;
    Ok(())
}

pub fn write_ci_workflow(repo_root: &Path) -> Result<()> {
    const GH_WORKFLOW: &str = r#"name: GitCortex Blast Radius

on:
  pull_request:

jobs:
  blast-radius:
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install gcx
        run: cargo install --git https://github.com/bharath03-a/GitCortex --bin gcx

      - name: Index repository
        run: gcx init

      - name: Run blast-radius analysis
        run: |
          gcx blast-radius \
            --base ${{ github.base_ref }} \
            --head ${{ github.head_ref }} \
            --format github-comment > /tmp/blast-radius.md

      - name: Post PR comment
        uses: marocchino/sticky-pull-request-comment@v2
        with:
          path: /tmp/blast-radius.md
"#;
    let dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&dir)?;
    let path = dir.join("gcx-blast-radius.yml");
    if !path.exists() {
        fs::write(path, GH_WORKFLOW).context("write gcx-blast-radius.yml")?;
    }
    Ok(())
}
