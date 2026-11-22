use std::{
    fs,
    path::{Path, PathBuf},
};

use gitcortex_core::error::{GitCortexError, Result};
use serde::{Deserialize, Serialize};

use crate::{
    branch,
    meta::{self, BranchEntry, RepoMeta},
};

/// Classification of an on-disk entry inside a repository data directory.
///
/// This is the authoritative inventory for compaction and the closed snapshot
/// contract: a snapshot contains strictly `Payload` + `Durable` entries, never
/// `Runtime` entries, and is refused while any `Unknown` entry exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryClass {
    /// KuzuDB payload files (`graph.kuzu`, `graph.kuzu.wal`).
    Payload,
    /// Durable sidecars and metadata (`schema_version`, `*.sha`, `*.dirty`,
    /// `*.overlay`, `*.ignore`, `*.epoch`, `embeddings_*.bin`,
    /// `editor_prompted`, `repo.json`, `branches.json`, `last_branch_gc`).
    Durable,
    /// Process-local runtime files (`serve.lock`, `mcp.sock`, `daemon.log`,
    /// `calls.jsonl*`, `worktrees/*/`). Never copied or moved.
    Runtime,
    /// Any unrecognized file or directory (including interrupted compaction
    /// artifacts `compact-*/` or `graph.kuzu.pre-compact-*`).
    Unknown,
}

/// Full inventory of a repository data directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataDirInventory {
    pub entries: Vec<(PathBuf, EntryClass)>,
}

impl DataDirInventory {
    /// True if any entry in the data directory is classified as [`EntryClass::Unknown`].
    pub fn has_unknown(&self) -> bool {
        self.entries
            .iter()
            .any(|(_, class)| *class == EntryClass::Unknown)
    }

    /// Paths classified as [`EntryClass::Unknown`].
    pub fn unknown_entries(&self) -> Vec<&Path> {
        self.entries
            .iter()
            .filter_map(|(path, class)| (*class == EntryClass::Unknown).then_some(path.as_path()))
            .collect()
    }

    /// Paths that belong in a closed snapshot (`Payload` + `Durable`).
    ///
    /// Returns an error if any [`EntryClass::Unknown`] entry is present so
    /// interrupted compactions or unexpected files are never silently dropped
    /// or copied.
    pub fn snapshot_entries(&self) -> Result<Vec<&Path>> {
        let unknown = self.unknown_entries();
        if !unknown.is_empty() {
            let names = unknown
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(GitCortexError::Store(format!(
                "refusing snapshot while unknown entries exist in store directory: {names}"
            )));
        }
        Ok(self
            .entries
            .iter()
            .filter_map(|(path, class)| {
                matches!(class, EntryClass::Payload | EntryClass::Durable).then_some(path.as_path())
            })
            .collect())
    }
}

fn classify_top_level_file(file_name: &str) -> EntryClass {
    match file_name {
        "graph.kuzu" | "graph.kuzu.wal" => EntryClass::Payload,
        "schema_version" | "editor_prompted" | "repo.json" | "branches.json" | "last_branch_gc" => {
            EntryClass::Durable
        }
        "serve.lock" | "mcp.sock" | "daemon.log" | "calls.jsonl" => EntryClass::Runtime,
        _ => {
            if file_name.starts_with("calls.jsonl.") {
                return EntryClass::Runtime;
            }
            for suffix in [".sha", ".dirty", ".overlay", ".ignore", ".epoch"] {
                if let Some(stem) = file_name.strip_suffix(suffix) {
                    if !stem.is_empty() {
                        return EntryClass::Durable;
                    }
                }
            }
            if let Some(middle) = file_name
                .strip_prefix("embeddings_")
                .and_then(|s| s.strip_suffix(".bin"))
            {
                if !middle.is_empty() {
                    return EntryClass::Durable;
                }
            }
            EntryClass::Unknown
        }
    }
}

fn collect_worktree_runtime_entries(
    dir: &Path,
    out: &mut Vec<(PathBuf, EntryClass)>,
) -> Result<()> {
    let mut stack = vec![dir.to_path_buf()];
    let mut found_file = false;
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(GitCortexError::Io(err)),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                stack.push(path);
            } else {
                found_file = true;
                out.push((path, EntryClass::Runtime));
            }
        }
    }
    if !found_file {
        out.push((dir.to_path_buf(), EntryClass::Runtime));
    }
    Ok(())
}

/// Build the classified inventory of an explicit repository data directory.
pub fn data_dir_inventory_in_dir(dir: &Path) -> Result<DataDirInventory> {
    let read_dir = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DataDirInventory {
                entries: Vec::new(),
            });
        }
        Err(err) => return Err(GitCortexError::Io(err)),
    };

    let payload_set = branch::payload_files_in_dir(dir);
    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if payload_set.iter().any(|p| p == &path) {
            entries.push((path, EntryClass::Payload));
            continue;
        }

        if ft.is_dir() {
            if name_str == "worktrees" {
                collect_worktree_runtime_entries(&path, &mut entries)?;
            } else {
                entries.push((path, EntryClass::Unknown));
            }
        } else {
            entries.push((path, classify_top_level_file(&name_str)));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(DataDirInventory { entries })
}

/// Build the classified inventory of `branch::data_dir(repo_id)`.
pub fn data_dir_inventory(repo_id: &str) -> Result<DataDirInventory> {
    data_dir_inventory_in_dir(&branch::data_dir(repo_id))
}

// ── Store root scanning (`gcx storage list`) ─────────────────────────────────

/// Ownership/liveness status of a store directory under the data root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoStatus {
    /// `repo.json` exists and its `worktree_root` exists on disk.
    Active,
    /// `repo.json` exists, `worktree_root` is gone, and its parent directory exists.
    Orphaned,
    /// `repo.json` exists, `worktree_root` is gone, and its parent directory is
    /// also missing (e.g. an unmounted external volume). Never auto-pruned.
    Unreachable,
    /// No `repo.json` (upgraded pre-metadata store or unrecognized directory).
    Unknown,
    /// Historical backup directory (`*.bak-*`).
    Backup,
}

/// Usage and metadata summary for a single store directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoUsage {
    pub repo_id: String,
    pub dir: PathBuf,
    pub bytes: u64,
    pub meta: Option<RepoMeta>,
    pub status: RepoStatus,
    pub branches: Vec<BranchEntry>,
}

/// Recursive file size sum in bytes for `path` (does not follow symlinks).
pub fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let meta = match fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = meta.file_type();
        if ft.is_symlink() {
            continue;
        }
        if ft.is_file() {
            total = total.saturating_add(meta.len());
            continue;
        }
        if ft.is_dir() {
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                stack.push(entry.path());
            }
        }
    }
    total
}

fn is_backup_dir_name(name: &str) -> bool {
    name.split_once(".bak-")
        .map(|(prefix, suffix)| !prefix.is_empty() && !suffix.is_empty())
        .unwrap_or(false)
}

fn classify_repo_status(dir_name: &str, meta: Option<&RepoMeta>) -> RepoStatus {
    if is_backup_dir_name(dir_name) {
        return RepoStatus::Backup;
    }
    let Some(meta) = meta else {
        return RepoStatus::Unknown;
    };
    if meta.worktree_root.exists() {
        return RepoStatus::Active;
    }
    match meta.worktree_root.parent() {
        Some(parent) if parent.exists() => RepoStatus::Orphaned,
        _ => RepoStatus::Unreachable,
    }
}

/// Inspect a single repository data directory and return its [`RepoUsage`].
pub fn inspect_repo_dir(dir: &Path) -> Result<RepoUsage> {
    let repo_id = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = dir_size_bytes(dir);
    let is_backup = is_backup_dir_name(&repo_id);
    let meta = if is_backup {
        None
    } else {
        meta::read_repo_meta_in_dir(dir).ok().flatten()
    };
    let branches = if is_backup {
        Vec::new()
    } else {
        meta::read_branch_registry_in_dir(dir).unwrap_or_default()
    };
    let status = classify_repo_status(&repo_id, meta.as_ref());
    Ok(RepoUsage {
        repo_id,
        dir: dir.to_path_buf(),
        bytes,
        meta,
        status,
        branches,
    })
}

/// Scan every repository store directory under `root`.
///
/// Ignores non-directory entries and the legacy `models` cache directory.
pub fn scan_store_root(root: &Path) -> Result<Vec<RepoUsage>> {
    let read_dir = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(GitCortexError::Io(err)),
    };

    let mut repos = Vec::new();
    for entry in read_dir {
        let entry = entry?;
        let ft = entry.file_type()?;
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == "models" {
            continue;
        }
        repos.push(inspect_repo_dir(&entry.path())?);
    }

    repos.sort_by(|a, b| a.repo_id.cmp(&b.repo_id));
    Ok(repos)
}

// ── Storage pruning (`gcx storage prune`) ────────────────────────────────────

/// Policy controlling which store entries [`plan_prune`] selects for removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrunePolicy {
    /// Remove stores whose `worktree_root` no longer exists while its parent does.
    pub orphaned: bool,
    /// Remove historical `*.bak-*` dirs and leftover `graph.kuzu.pre-compact-*` payloads.
    pub backups: bool,
    /// Remove `Unknown` (metadata-less) stores whose `graph.kuzu` mtime is at
    /// least this many days old. `None` never selects `Unknown` stores.
    pub unknown_older_than_days: Option<u32>,
}

impl Default for PrunePolicy {
    fn default() -> Self {
        Self {
            orphaned: true,
            backups: true,
            unknown_older_than_days: None,
        }
    }
}

/// Dry-run plan produced by [`plan_prune`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePlan {
    pub items: Vec<(RepoUsage, &'static str)>,
    pub bytes: u64,
}

/// Outcome returned by [`apply_prune`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PruneReport {
    pub removed: u64,
    pub skipped_locked: Vec<PathBuf>,
}

fn is_pre_compact_leftover(file_name: &str) -> bool {
    file_name
        .strip_prefix("graph.kuzu.pre-compact-")
        .map(|rest| !rest.is_empty())
        .unwrap_or(false)
}

fn unknown_dir_reference_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let graph = dir.join("graph.kuzu");
    if let Ok(meta) = fs::metadata(&graph) {
        if let Ok(modified) = meta.modified() {
            return Some(modified);
        }
    }
    fs::metadata(dir).ok().and_then(|m| m.modified().ok())
}

fn is_older_than_days(mtime: std::time::SystemTime, days: u32, now: std::time::SystemTime) -> bool {
    let threshold = std::time::Duration::from_secs(u64::from(days).saturating_mul(86_400));
    match now.duration_since(mtime) {
        Ok(age) => age >= threshold,
        Err(_) => false,
    }
}

/// Compute a dry-run [`PrunePlan`] for `root` under `policy`. Never deletes files.
pub fn plan_prune(root: &Path, policy: &PrunePolicy) -> Result<PrunePlan> {
    let now = std::time::SystemTime::now();
    let repos = scan_store_root(root)?;
    let mut items: Vec<(RepoUsage, &'static str)> = Vec::new();
    let mut bytes = 0u64;

    for repo in repos {
        match repo.status {
            RepoStatus::Orphaned if policy.orphaned => {
                bytes = bytes.saturating_add(repo.bytes);
                items.push((repo, "orphaned"));
                continue;
            }
            RepoStatus::Backup if policy.backups => {
                bytes = bytes.saturating_add(repo.bytes);
                items.push((repo, "backup"));
                continue;
            }
            RepoStatus::Unknown => {
                if let Some(days) = policy.unknown_older_than_days {
                    if let Some(mtime) = unknown_dir_reference_mtime(&repo.dir) {
                        if is_older_than_days(mtime, days, now) {
                            bytes = bytes.saturating_add(repo.bytes);
                            items.push((repo, "unknown"));
                            continue;
                        }
                    }
                }
            }
            RepoStatus::Unreachable => {
                // Never prune unreachable stores (e.g., unmounted volumes).
                continue;
            }
            RepoStatus::Active | RepoStatus::Orphaned | RepoStatus::Backup => {}
        }

        if !policy.backups {
            continue;
        }

        // Only prune leftover `graph.kuzu.pre-compact-*` payloads when a live
        // `graph.kuzu` is present so crash-recovery backups are never destroyed.
        if !repo.dir.join("graph.kuzu").exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&repo.dir) else {
            continue;
        };
        let mut leftovers = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if is_pre_compact_leftover(&name_str) {
                leftovers.push(entry.path());
            }
        }
        leftovers.sort();
        for path in leftovers {
            let size = dir_size_bytes(&path);
            bytes = bytes.saturating_add(size);
            items.push((
                RepoUsage {
                    repo_id: repo.repo_id.clone(),
                    dir: path,
                    bytes: size,
                    meta: None,
                    status: RepoStatus::Backup,
                    branches: Vec::new(),
                },
                "pre-compact backup",
            ));
        }
    }

    Ok(PrunePlan { items, bytes })
}

fn try_lock_store_dir(store_dir: &Path) -> Result<Option<Option<fs::File>>> {
    let lock_path = store_dir.join("serve.lock");
    if !lock_path.exists() {
        return Ok(Some(None));
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)?;
    match fs2::FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(Some(Some(file))),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(GitCortexError::Io(err)),
    }
}

/// Execute a [`PrunePlan`], skipping any target whose repository `serve.lock`
/// is currently held by another process.
pub fn apply_prune(plan: &PrunePlan) -> Result<PruneReport> {
    let mut report = PruneReport::default();

    for (usage, _reason) in &plan.items {
        let target = &usage.dir;
        let meta = match fs::symlink_metadata(target) {
            Ok(m) => m,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(GitCortexError::Io(err)),
        };

        let store_dir = if meta.is_dir() {
            target.as_path()
        } else {
            target.parent().unwrap_or(target.as_path())
        };

        let Some(_lock_guard) = try_lock_store_dir(store_dir)? else {
            report.skipped_locked.push(target.clone());
            continue;
        };

        if meta.is_dir() {
            fs::remove_dir_all(target)?;
        } else {
            fs::remove_file(target)?;
        }
        report.removed = report.removed.saturating_add(usage.bytes);
    }

    Ok(report)
}
