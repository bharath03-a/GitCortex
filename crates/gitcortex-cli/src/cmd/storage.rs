use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;
use gitcortex_store::{
    branch,
    usage::{
        apply_prune, inspect_repo_dir, plan_prune, scan_store_root, PrunePolicy, RepoStatus,
        RepoUsage,
    },
};

use crate::style::{header_style, hint_style, name_style, paint};

#[derive(Subcommand)]
pub enum StorageCmd {
    /// Inspect local GitCortex graph storage usage (current repository by default).
    List {
        /// List every store directory under the local GitCortex data root.
        #[arg(long)]
        all: bool,
        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Reclaim orphaned stores, historical backups, or stale unknown stores (dry-run by default).
    Prune {
        /// Select stores whose repository directory was deleted (parent still exists).
        #[arg(long)]
        orphaned: bool,
        /// Select historical `*.bak-*` dirs and leftover `graph.kuzu.pre-compact-*` payloads.
        #[arg(long)]
        backups: bool,
        /// Select metadata-less `unknown` stores whose `graph.kuzu` is older than DAYS days.
        #[arg(long, value_name = "DAYS")]
        unknown_older_than: Option<u32>,
        /// Perform the deletions (without `--apply`, only prints the plan).
        #[arg(long)]
        apply: bool,
    },
}

pub fn run(cmd: StorageCmd) -> Result<()> {
    match cmd {
        StorageCmd::List { all, json } => run_list(all, json),
        StorageCmd::Prune {
            orphaned,
            backups,
            unknown_older_than,
            apply,
        } => run_prune(orphaned, backups, unknown_older_than, apply),
    }
}

fn current_repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse --show-toplevel failed")?;
    if !output.status.success() {
        anyhow::bail!(
            "not inside a git repository (use `gcx storage list --all` to inspect every local store)"
        );
    }
    let raw = String::from_utf8(output.stdout)?.trim().to_owned();
    if raw.is_empty() {
        anyhow::bail!("git rev-parse --show-toplevel returned an empty path");
    }
    Ok(PathBuf::from(raw))
}

fn run_list(all: bool, json: bool) -> Result<()> {
    let entries: Vec<RepoUsage> = if all {
        scan_store_root(&branch::data_root())?
    } else {
        let repo_root = current_repo_root()?;
        let repo_id = branch::storage_repo_id(&repo_root);
        let dir = branch::data_dir(&repo_id);
        if dir.exists() {
            vec![inspect_repo_dir(&dir)?]
        } else {
            Vec::new()
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if entries.is_empty() {
        if all {
            println!(
                "{}",
                paint(
                    hint_style(),
                    &format!(
                        "No GitCortex stores found in {}",
                        branch::data_root().display()
                    )
                )
            );
        } else {
            println!(
                "{}",
                paint(
                    hint_style(),
                    "No local GitCortex store for this repository yet. Run `gcx init` to create one."
                )
            );
        }
        return Ok(());
    }

    let mut total_bytes = 0u64;
    for entry in &entries {
        total_bytes = total_bytes.saturating_add(entry.bytes);
        let status = status_label(entry.status);
        let size = format_bytes(entry.bytes);
        let target = entry
            .meta
            .as_ref()
            .map(|m| m.worktree_root.display().to_string())
            .unwrap_or_else(|| entry.dir.display().to_string());
        println!(
            "{}  {:>9}  {:>3} br  {}  {}",
            paint(header_style(), &format!("{status:<11}")),
            paint(name_style(), &size),
            entry.branches.len(),
            paint(hint_style(), &entry.repo_id),
            target,
        );
    }

    if all {
        println!(
            "\n{} {} across {} store(s) in {}",
            paint(header_style(), "total:"),
            paint(name_style(), &format_bytes(total_bytes)),
            entries.len(),
            branch::data_root().display(),
        );
    }
    Ok(())
}

fn run_prune(
    orphaned: bool,
    backups: bool,
    unknown_older_than: Option<u32>,
    apply: bool,
) -> Result<()> {
    let policy = if !orphaned && !backups && unknown_older_than.is_none() {
        PrunePolicy::default()
    } else {
        PrunePolicy {
            orphaned,
            backups,
            unknown_older_than_days: unknown_older_than,
        }
    };

    let root = branch::data_root();
    let plan = plan_prune(&root, &policy)?;
    if plan.items.is_empty() {
        println!(
            "{}",
            paint(
                hint_style(),
                &format!("Nothing to prune in {}.", root.display())
            )
        );
        return Ok(());
    }

    for (usage, reason) in &plan.items {
        println!(
            "{}  {:>9}  {}",
            paint(header_style(), &format!("{reason:<18}")),
            paint(name_style(), &format_bytes(usage.bytes)),
            usage.dir.display(),
        );
    }

    if !apply {
        println!(
            "\n{} {} across {} item(s). Re-run with `--apply` to delete.",
            paint(header_style(), "dry-run:"),
            paint(name_style(), &format_bytes(plan.bytes)),
            plan.items.len(),
        );
        return Ok(());
    }

    let report = apply_prune(&plan)?;
    println!(
        "\n{} {} reclaimed.",
        paint(header_style(), "pruned:"),
        paint(name_style(), &format_bytes(report.removed)),
    );
    if !report.skipped_locked.is_empty() {
        println!(
            "{} {} locked item(s) skipped:",
            paint(header_style(), "skipped:"),
            report.skipped_locked.len(),
        );
        for path in &report.skipped_locked {
            println!("  {}", path.display());
        }
    }
    Ok(())
}

fn status_label(status: RepoStatus) -> &'static str {
    match status {
        RepoStatus::Active => "active",
        RepoStatus::Orphaned => "orphaned",
        RepoStatus::Unreachable => "unreachable",
        RepoStatus::Unknown => "unknown",
        RepoStatus::Backup => "backup",
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}
