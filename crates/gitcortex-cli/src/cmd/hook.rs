use std::time::Instant;

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::{kuzu::KuzuGraphStore, meta};

use crate::cmd::{export, init::helpers::repo_root, serve_lock, storage};

fn maybe_run_daily_branch_gc(repo_root: &std::path::Path, store: &mut KuzuGraphStore) {
    let gc_t0 = Instant::now();
    let repo_id = store.repo_id().to_owned();
    if !meta::branch_gc_due_now(&repo_id) {
        if std::env::var_os("GCX_TIMING").is_some() {
            eprintln!(
                "[gcx-timing] hook branch_gc skipped (not due, {}ms)",
                gc_t0.elapsed().as_millis()
            );
        }
        return;
    }

    match storage::run_gc_on_store(repo_root, store) {
        Ok(report) => {
            if let Err(err) = meta::mark_branch_gc_now(&repo_id) {
                tracing::debug!("failed to record last_branch_gc: {err}");
            }
            if std::env::var_os("GCX_TIMING").is_some() {
                eprintln!(
                    "[gcx-timing] hook branch_gc ran: dropped={} kept={} ({}ms)",
                    report.dropped.len(),
                    report.kept,
                    gc_t0.elapsed().as_millis()
                );
            }
            if !report.dropped.is_empty() {
                tracing::info!(
                    dropped = report.dropped.len(),
                    kept = report.kept,
                    "automatic daily branch GC completed"
                );
            }
        }
        Err(err) => {
            tracing::debug!("automatic daily branch GC skipped: {err:#}");
        }
    }
}

pub fn run(branch_switch: bool) -> Result<()> {
    let t0 = Instant::now();
    let repo_root = repo_root()?;

    // The long-lived MCP server owns the embedded Kuzu connection. Its watcher
    // synchronizes commits and branch changes, so a hook must not contend for
    // the same process-exclusive database lock.
    if serve_lock::is_active(&repo_root)? {
        tracing::debug!(
            branch_switch,
            "active MCP server will synchronize Git state"
        );
        return Ok(());
    }

    let branch = current_branch(&repo_root)?;
    let mut store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    if branch_switch {
        maybe_run_daily_branch_gc(&repo_root, &mut store);
    }

    let last_sha = store.last_indexed_sha(&branch)?;
    if store.working_tree_dirty(&branch)? {
        anyhow::bail!(
            "graph contains an unverified working-tree overlay; run gcx init to repair it"
        );
    }

    let rules_before = gitcortex_mcp::mcp::ignore_rules::rules_hash(&repo_root).ok();
    let rules_mismatch = rules_before.as_ref().map_or(true, |rules| {
        !gitcortex_mcp::mcp::ignore_rules::indexed_rules_match(&repo_root, &branch, rules)
            .unwrap_or(false)
    });
    let indexer = IncrementalIndexer::new(&repo_root).context("failed to create indexer")?;

    let (mut diff, head_sha) = indexer
        .run(if rules_mismatch {
            None
        } else {
            last_sha.as_deref()
        })
        .context("incremental index failed")?;
    if rules_mismatch {
        diff.removed_files.extend(
            store
                .list_all_nodes(&branch)?
                .into_iter()
                .filter(|node| node.kind != gitcortex_core::schema::NodeKind::Folder)
                .map(|node| node.file),
        );
    }

    if diff.is_empty() {
        // Empty commits still advance the indexed revision.
        store.bump_mutation_epoch(&branch)?;
        if let Some(rules) = rules_before.as_ref() {
            if gitcortex_mcp::mcp::ignore_rules::rules_hash(&repo_root).ok() == rules_before {
                gitcortex_mcp::mcp::ignore_rules::persist_indexed_rules(
                    &repo_root,
                    &branch,
                    rules.as_deref(),
                )?;
            }
        }
        store.set_last_indexed_sha(&branch, &head_sha)?;
        let elapsed = t0.elapsed().as_millis();
        eprintln!("gcx  [{branch}]  (no changes)  ({elapsed}ms)");
        tracing::debug!(elapsed_ms = elapsed, "no changes");
        return Ok(());
    }

    let snapshots = diff.source_hashes.iter().cloned().collect();
    let verified =
        gitcortex_mcp::mcp::readiness::snapshots_match_commit(&repo_root, &head_sha, &snapshots)?;
    store.bump_mutation_epoch(&branch)?;
    store.set_working_tree_dirty(&branch, true)?;
    store
        .apply_diff(&branch, &diff)
        .context("failed to apply diff")?;
    if let Some(rules) = rules_before.as_ref() {
        if gitcortex_mcp::mcp::ignore_rules::rules_hash(&repo_root).ok() == rules_before {
            gitcortex_mcp::mcp::ignore_rules::persist_indexed_rules(
                &repo_root,
                &branch,
                rules.as_deref(),
            )?;
        }
    }
    store
        .set_last_indexed_sha(&branch, &head_sha)
        .context("failed to persist sha")?;
    store.set_working_tree_dirty(&branch, !verified)?;

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
