#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path};

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;

use super::helpers::current_branch;

pub(crate) const HOOK_BLOCK_START: &str = "# >>> gitcortex managed hook >>>";
pub(crate) const HOOK_BLOCK_END: &str = "# <<< gitcortex managed hook <<<";

const HOOK_NAMES: &[(&str, &str)] = &[
    ("post-commit", "gcx hook"),
    ("post-merge", "gcx hook"),
    ("post-rewrite", "gcx hook"),
    ("post-checkout", "gcx hook --branch-switch"),
];

fn hook_block(command: &str) -> String {
    format!(
        "{HOOK_BLOCK_START}\nexport PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH\"\nif command -v gcx >/dev/null 2>&1; then\n  {command} || printf '%s\\n' 'warning: GitCortex index update deferred; run gcx doctor' >&2\nfi\n{HOOK_BLOCK_END}\n"
    )
}

const AGENT_GUIDE: &str = r#"# GitCortex Agent Guide

This repository has a local GitCortex knowledge graph. When an MCP integration
is configured with `gcx init --editor <name>`, use its compact `gcx` dispatch
tool for cross-file structure and verify behavior in source and tests.

## Useful actions

| Action | Purpose |
|--------|---------|
| `lookup_symbol` | Locate a function, type, method, or constant by name |
| `find_callers` | Trace direct and transitive callers |
| `find_callees` | Trace outgoing calls |
| `list_definitions` | List symbols defined in a file |
| `get_subgraph` | Map a bounded symbol neighbourhood |
| `start_tour` | Get a component-oriented repository overview |
| `detect_changes` | Relate Git changes to affected symbols |

CLI equivalents are available under `gcx query`; run `gcx query --help` for the
current command list. Run `gcx hook` if the index appears stale.

The graph is navigation evidence, not a substitute for reading implementation
and tests before making changes.
"#;

pub fn git_hooks_dir(repo_root: &Path) -> Result<std::path::PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(repo_root)
        .output()
        .context("resolve Git hooks directory")?;
    if !output.status.success() {
        anyhow::bail!("git rev-parse --git-path hooks failed");
    }
    let value = String::from_utf8(output.stdout)?.trim().to_owned();
    let path = std::path::PathBuf::from(value);
    Ok(if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    })
}

pub fn ensure_hooks_scope(repo_root: &Path, allow_shared: bool) -> Result<()> {
    let hooks_dir = git_hooks_dir(repo_root)?;
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(repo_root)
        .output()
        .context("resolve Git common directory")?;
    if !output.status.success() {
        anyhow::bail!("git rev-parse --git-common-dir failed");
    }
    let common = std::path::PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let common = if common.is_absolute() {
        common
    } else {
        repo_root.join(common)
    };
    let repository_owned = hooks_dir.starts_with(repo_root) || hooks_dir.starts_with(common);
    if !repository_owned && !allow_shared {
        anyhow::bail!(
            "Git hooks path {} is shared outside this repository; rerun with --shared-git-hooks to permit modifying it",
            hooks_dir.display()
        );
    }
    Ok(())
}

pub fn install_hooks(repo_root: &Path, allow_shared: bool) -> Result<usize> {
    ensure_hooks_scope(repo_root, allow_shared)?;
    let hooks_dir = git_hooks_dir(repo_root)?;
    fs::create_dir_all(&hooks_dir)?;

    let mut installed = 0;
    for (name, command) in HOOK_NAMES {
        let path = hooks_dir.join(name);
        let block = hook_block(command);
        if path.exists() {
            let existing = fs::read_to_string(&path)?;
            if existing.contains(HOOK_BLOCK_START) {
                continue;
            }
            let base = existing
                .lines()
                .filter(|line| !line.trim_start().starts_with("gcx hook"))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(&path, format!("{}\n{block}", base.trim_end()))?;
        } else {
            fs::write(&path, format!("#!/usr/bin/env sh\n{block}"))?;
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
    let mut store =
        KuzuGraphStore::open_for_init(repo_root).context("failed to open graph store")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_respect_core_hooks_path_and_preserve_existing_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(temp.path())
            .status()
            .expect("git init");
        assert!(status.success());
        let status = std::process::Command::new("git")
            .args(["config", "core.hooksPath", ".custom-hooks"])
            .current_dir(temp.path())
            .status()
            .expect("git config");
        assert!(status.success());

        let hooks = temp.path().join(".custom-hooks");
        fs::create_dir_all(&hooks).expect("hooks dir");
        fs::write(hooks.join("post-commit"), "#!/bin/sh\necho existing\n").expect("existing hook");

        assert_eq!(install_hooks(temp.path(), false).expect("install hooks"), 4);
        let installed = fs::read_to_string(hooks.join("post-commit")).expect("read hook");
        assert!(installed.contains("echo existing"));
        assert!(installed.contains(HOOK_BLOCK_START));
        assert!(installed.contains("gcx hook ||"));
        assert!(!temp.path().join(".git/hooks/post-commit").exists());
    }

    #[test]
    fn shared_hooks_require_explicit_permission() {
        let temp = tempfile::tempdir().expect("tempdir");
        let shared = tempfile::tempdir().expect("shared hooks");
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(temp.path())
            .status()
            .expect("git init")
            .success());
        assert!(std::process::Command::new("git")
            .args(["config", "core.hooksPath"])
            .arg(shared.path())
            .current_dir(temp.path())
            .status()
            .expect("git config")
            .success());
        assert!(ensure_hooks_scope(temp.path(), false).is_err());
        assert!(ensure_hooks_scope(temp.path(), true).is_ok());
    }
}
