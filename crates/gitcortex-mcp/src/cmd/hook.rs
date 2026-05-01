use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::cmd::export;

pub fn run(branch_switch: bool) -> Result<()> {
    let t0 = Instant::now();
    let repo_root = repo_root()?;

    if branch_switch {
        // post-checkout: no re-index needed, the target branch has its own graph.
        tracing::debug!(
            elapsed_ms = t0.elapsed().as_millis(),
            "branch-switch: no-op"
        );
        return Ok(());
    }

    let branch = current_branch(&repo_root)?;
    let mut store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    let last_sha = store.last_indexed_sha(&branch)?;

    let indexer = IncrementalIndexer::new(&repo_root).context("failed to create indexer")?;

    let (diff, head_sha) = indexer
        .run(last_sha.as_deref())
        .context("incremental index failed")?;

    if diff.is_empty() {
        tracing::debug!(elapsed_ms = t0.elapsed().as_millis(), "no changes");
        return Ok(());
    }

    store
        .apply_diff(&branch, &diff)
        .context("failed to apply diff")?;
    store
        .set_last_indexed_sha(&branch, &head_sha)
        .context("failed to persist sha")?;

    let elapsed = t0.elapsed().as_millis();
    let added_n = diff.added_nodes.len();
    let added_e = diff.added_edges.len();
    let removed = diff.removed_files.len();

    // Brief summary visible to the user after every commit.
    eprintln!(
        "gcx  [{branch}]  +{added_n} nodes  +{added_e} edges  -{removed} files  ({elapsed}ms)"
    );

    tracing::info!(
        branch,
        added_nodes = added_n,
        added_edges = added_e,
        removed_files = removed,
        elapsed_ms = elapsed,
        "indexed"
    );
    if elapsed > 500 {
        tracing::warn!(elapsed_ms = elapsed, "hook exceeded 500ms budget");
    }

    // Regenerate context.md if it was previously exported (opt-in).
    export::refresh_if_exists(&repo_root, &store, &branch);

    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    Ok(PathBuf::from(
        String::from_utf8(output.stdout)?.trim().to_owned(),
    ))
}

fn current_branch(repo_root: &std::path::Path) -> Result<String> {
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
            .output()?;
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}
