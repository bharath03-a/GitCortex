use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
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

const HOOK_SHEBANG: &str = "#!/usr/bin/env sh\nset -e\n";

pub fn run() -> Result<()> {
    let repo_root = repo_root()?;
    install_hooks(&repo_root)?;
    initial_index(&repo_root)?;
    Ok(())
}

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

fn install_hooks(repo_root: &Path) -> Result<()> {
    let hooks_dir = repo_root.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    for (name, body) in HOOK_NAMES {
        let path = hooks_dir.join(name);
        if path.exists() {
            // Append our line if not already present, don't clobber existing hooks.
            let existing = fs::read_to_string(&path)?;
            if existing.contains("gcx hook") {
                eprintln!("hook already installed: {name}");
                continue;
            }
            fs::write(&path, format!("{existing}\n{body}"))?;
        } else {
            fs::write(&path, format!("{HOOK_SHEBANG}{body}"))?;
        }
        // chmod +x
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
        eprintln!("installed hook: {name}");
    }
    Ok(())
}

fn initial_index(repo_root: &Path) -> Result<()> {
    let mut store = KuzuGraphStore::open(repo_root)
        .context("failed to open graph store")?;

    let branch = current_branch(repo_root)?;

    if store.last_indexed_sha(&branch)?.is_some() {
        eprintln!("already indexed — skipping initial index");
        return Ok(());
    }

    eprintln!("running initial index for branch '{branch}'…");
    let indexer = IncrementalIndexer::new(repo_root)
        .context("failed to create indexer")?;

    let (diff, head_sha) = indexer
        .run(None)
        .context("initial index failed")?;

    store
        .apply_diff(&branch, &diff)
        .context("failed to apply diff")?;
    store
        .set_last_indexed_sha(&branch, &head_sha)
        .context("failed to persist sha")?;

    eprintln!(
        "indexed {} nodes, {} edges",
        diff.added_nodes.len(),
        diff.added_edges.len()
    );
    Ok(())
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
        // Detached HEAD — use the SHA instead.
        let sha = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(repo_root)
            .output()
            .context("git rev-parse HEAD failed")?;
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}
