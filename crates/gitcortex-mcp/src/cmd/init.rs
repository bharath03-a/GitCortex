use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;

const HOOK_NAMES: &[(&str, &str)] = &[
    ("post-commit", "gcx hook\n"),
    ("post-merge", "gcx hook\n"),
    ("post-rewrite", "gcx hook\n"),
    ("post-checkout", "gcx hook --branch-switch\n"),
];

const HOOK_SHEBANG: &str = "#!/usr/bin/env sh\nset -e\nexport PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH\"\n";

const MCP_JSON: &str = r#"{
  "mcpServers": {
    "gitcortex": {
      "command": "gcx",
      "args": ["serve"]
    }
  }
}
"#;

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

const CLAUDE_MD_SECTION: &str = r#"
## GitCortex Knowledge Graph

This repo is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
Use the MCP server (`gcx serve`, configured in `.mcp.json`) or these slash commands:

- `/gcx-lookup <name>` — find all definitions matching a name
- `/gcx-callers <name>` — find all callers of a function
- `/gcx-file <path>` — list all definitions in a file
- `/gcx-blast-radius` — show blast radius of changes vs main
"#;

// ── Agent skills ─────────────────────────────────────────────────────────────

const SKILLS: &[(&str, &str)] = &[
    (
        "exploring.md",
        r#"# Exploring Unfamiliar Code

Use the GitCortex knowledge graph to navigate unfamiliar parts of the codebase fast.

## Workflow

1. **Find a symbol** — `lookup_symbol` to locate any struct, function, or trait by name
2. **See a file's shape** — `list_definitions` on any file to get all definitions at a glance
3. **Trace callers** — `find_callers` to understand who calls a function and build a call chain
4. **Visualise** — run `gcx viz` to open the interactive graph in the browser

## When to use
- Starting a task in an unfamiliar module
- Understanding how a piece of code fits into the larger system
- Navigating a large codebase without reading every file

## Examples
- "Where is `GraphStore` defined?" → `lookup_symbol(name: "GraphStore")`
- "What does `indexer.rs` contain?" → `list_definitions(file: "crates/gitcortex-indexer/src/indexer.rs")`
- "What calls `apply_diff`?" → `find_callers(function_name: "apply_diff")`
"#,
    ),
    (
        "debugging.md",
        r#"# Debugging with the Call Graph

Trace bugs backward through the call chain using the knowledge graph.

## Workflow

1. **Locate the failing function** — `lookup_symbol` to find it and confirm the file/line
2. **Find direct callers** — `find_callers` to identify what triggered the bad code path
3. **Walk up the chain** — repeat `find_callers` on each caller to reach the entry point
4. **Check file context** — `list_definitions` on the relevant file to see surrounding code

## Key insight
`find_callers` traverses `Calls` edges in the knowledge graph — this is the actual parsed call graph,
not a grep. Use it iteratively to reconstruct the full execution path to a crash or wrong value.

## When to use
- Tracking down where a corrupted value originates
- Finding all the places that can trigger a bug
- Understanding the execution path to an error
"#,
    ),
    (
        "impact-analysis.md",
        r#"# Impact Analysis Before Making Changes

Before modifying a function, struct, or trait — understand everything that depends on it.

## Workflow

1. **Look up the symbol** — `lookup_symbol(name: "YourSymbol")`
2. **Find direct callers** — `find_callers(function_name: "your_function")`
3. **Walk the blast radius** — repeat `find_callers` on each caller; stop when callers are entry points
4. **After changes** — run `gcx blast-radius --base main --head HEAD` for a full risk report

## Risk heuristic
| Caller count | Risk | Recommended action |
|---|---|---|
| 0–2 | LOW | Safe to refactor directly |
| 3–10 | MEDIUM | Add tests for callers before changing |
| 10+ | HIGH | Plan carefully, consider a compatibility shim |
| Core trait method | CRITICAL | All implementors must change — audit every impl |

## When to use
- Before renaming a public function or struct
- Before changing a function signature
- Before modifying a trait definition that has multiple implementors
"#,
    ),
    (
        "refactoring.md",
        r#"# Safe Refactoring with Dependency Mapping

Use the knowledge graph to plan refactors in the right order and avoid breaking changes.

## Workflow

1. **Map current structure** — `list_definitions` on every file in the module being refactored
2. **Find all dependents** — `find_callers` and `lookup_symbol` to identify callers and uses
3. **Check trait implementations** — look for structs that implement traits you're changing
4. **Plan the order** — change leaf nodes first (no callers), then work toward roots
5. **Verify after** — `branch_diff_graph(from: "main", to: "HEAD")` to confirm only intended nodes changed

## Patterns safe to refactor
- Private functions with zero external callers
- Structs used in only one file
- Methods on a struct with a single `impl` block

## Patterns that need care
- Public trait methods — every implementor must be updated
- Functions called from many files — run impact analysis first
- Structs that implement multiple traits — changing fields affects all trait impls

## When to use
- Extracting a module into a separate crate
- Renaming a public API across many files
- Changing a function signature that many callers depend on
"#,
    ),
];

// ── Slash command files ───────────────────────────────────────────────────────

const SLASH_COMMANDS: &[(&str, &str)] = &[
    (
        "gcx-lookup.md",
        "Run `gcx query lookup-symbol $ARGUMENTS` and show the results. \
Display each match with its kind, qualified name, file path, and line number.",
    ),
    (
        "gcx-callers.md",
        "Run `gcx query find-callers $ARGUMENTS` and show the results. \
List every caller with its kind, name, file, and line number. Briefly describe the call chain.",
    ),
    (
        "gcx-file.md",
        "Run `gcx query list-definitions $ARGUMENTS` and show the results. \
Display all definitions ordered by line number with their kind, name, visibility, and location.",
    ),
    (
        "gcx-blast-radius.md",
        "Run `gcx blast-radius --base main --head HEAD --format text` and show the results. \
Summarise which functions changed and which callers are affected, and highlight the risk level.",
    ),
];

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run(ci: bool) -> Result<()> {
    let repo_root = repo_root()?;
    let start = Instant::now();

    let hooks = install_hooks(&repo_root)?;
    let (nodes, edges) = initial_index(&repo_root)?;
    write_mcp_json(&repo_root)?;
    let commands = write_slash_commands(&repo_root)?;
    let skills = write_skills(&repo_root)?;
    update_claude_md(&repo_root)?;

    if ci {
        write_ci_workflow(&repo_root)?;
    }

    let ms = start.elapsed().as_millis();
    println!();
    println!("GitCortex initialised  ({ms}ms)");
    println!("  Graph:    {nodes} nodes | {edges} edges");
    println!("  Hooks:    {hooks} installed");
    println!("  MCP:      .mcp.json  (2 prompts + 4 tools)");
    println!("  Skills:   .claude/skills/gcx/  ({skills} agent skills)");
    println!("  Commands: .claude/commands/gcx/  ({commands} slash commands)");
    if ci {
        println!("  CI:       .github/workflows/gcx-blast-radius.yml");
    }
    println!();

    Ok(())
}

// ── Hooks ─────────────────────────────────────────────────────────────────────

fn install_hooks(repo_root: &Path) -> Result<usize> {
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
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
        installed += 1;
    }
    Ok(installed)
}

// ── Indexing ──────────────────────────────────────────────────────────────────

fn initial_index(repo_root: &Path) -> Result<(usize, usize)> {
    let mut store =
        KuzuGraphStore::open(repo_root).context("failed to open graph store")?;
    let branch = current_branch(repo_root)?;

    let existing_sha = store.last_indexed_sha(&branch)?;
    if existing_sha.is_none() {
        let indexer =
            IncrementalIndexer::new(repo_root).context("failed to create indexer")?;
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

// ── MCP config ────────────────────────────────────────────────────────────────

fn write_mcp_json(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".mcp.json");
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("gitcortex") {
            return Ok(());
        }
    }
    fs::write(&path, MCP_JSON).context("write .mcp.json")?;
    Ok(())
}

// ── Slash commands ────────────────────────────────────────────────────────────

fn write_slash_commands(repo_root: &Path) -> Result<usize> {
    let dir = repo_root.join(".claude").join("commands").join("gcx");
    fs::create_dir_all(&dir)?;

    let mut written = 0;
    for (filename, content) in SLASH_COMMANDS {
        let path = dir.join(filename);
        if !path.exists() {
            fs::write(&path, content).with_context(|| format!("write {filename}"))?;
            written += 1;
        }
    }
    Ok(written)
}

// ── Agent skills ─────────────────────────────────────────────────────────────

fn write_skills(repo_root: &Path) -> Result<usize> {
    let dir = repo_root.join(".claude").join("skills").join("gcx");
    fs::create_dir_all(&dir)?;

    let mut written = 0;
    for (filename, content) in SKILLS {
        let path = dir.join(filename);
        if !path.exists() {
            fs::write(&path, content).with_context(|| format!("write skill {filename}"))?;
            written += 1;
        }
    }
    Ok(written)
}

// ── CLAUDE.md update ──────────────────────────────────────────────────────────

fn update_claude_md(repo_root: &Path) -> Result<()> {
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir)?;
    let path = claude_dir.join("CLAUDE.md");

    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex Knowledge Graph") {
            return Ok(());
        }
        let updated = format!("{existing}{CLAUDE_MD_SECTION}");
        fs::write(&path, updated).context("update CLAUDE.md")?;
    } else {
        fs::write(&path, CLAUDE_MD_SECTION.trim_start()).context("write CLAUDE.md")?;
    }
    Ok(())
}

// ── CI workflow ───────────────────────────────────────────────────────────────

fn write_ci_workflow(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&dir)?;
    let path = dir.join("gcx-blast-radius.yml");
    if !path.exists() {
        fs::write(&path, GH_WORKFLOW).context("write gcx-blast-radius.yml")?;
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed — are you inside a git repository?")?;
    if !output.status.success() {
        anyhow::bail!("not inside a git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8(output.stdout)?.trim().to_owned(),
    ))
}

fn current_branch(repo_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        .output()
        .context("git symbolic-ref failed")?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    } else {
        let sha = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(repo_root)
            .output()
            .context("git rev-parse HEAD failed")?;
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}
