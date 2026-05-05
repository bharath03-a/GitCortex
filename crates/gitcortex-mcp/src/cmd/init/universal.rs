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

    if store.last_indexed_sha(&branch)?.is_none() {
        let indexer = IncrementalIndexer::new(repo_root).context("failed to create indexer")?;
        let (diff, head_sha) = indexer.run(None).context("initial index failed")?;
        store.apply_diff(&branch, &diff).context("apply diff")?;
        store
            .set_last_indexed_sha(&branch, &head_sha)
            .context("persist sha")?;
    }

    let nodes = store.list_all_nodes(&branch)?.len();
    let edges = store.list_all_edges(&branch)?.len();
    Ok((nodes, edges))
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
