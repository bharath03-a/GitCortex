use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use gitcortex_core::error::{GitCortexError, Result};
use serde::{Deserialize, Serialize};

use crate::branch::{self, BranchKind};

/// Current on-disk format version for `repo.json`.
pub const META_FORMAT_VERSION: u32 = 1;

/// Minimum interval (in seconds) between `last_opened_at` rewrites in `repo.json`.
pub const LAST_OPENED_THROTTLE_SECS: u64 = 3600;

/// Ownership metadata persisted at `{data_dir}/repo.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoMeta {
    pub format: u32,
    pub identity_version: u32,
    pub worktree_root: PathBuf,
    pub git_common_dir: PathBuf,
    pub created_at: u64,
    pub last_opened_at: u64,
}

/// Registered branch/snapshot entry persisted in `{data_dir}/branches.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchEntry {
    pub name: String,
    pub key: String,
    pub kind: BranchKind,
    pub worktree_id: Option<String>,
    pub last_indexed_at: u64,
}

pub(crate) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| GitCortexError::Store(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("meta.json");
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));

    let payload = serde_json::to_vec_pretty(value)
        .map_err(|e| GitCortexError::Store(format!("serialize {}: {e}", path.display())))?;

    let write_result = (|| -> std::io::Result<()> {
        let mut file = File::create(&tmp_path)?;
        file.write_all(&payload)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&tmp_path, path)?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(GitCortexError::Io(err));
    }
    Ok(())
}

// ── Repo metadata (`repo.json`) ──────────────────────────────────────────────

/// Path to `{data_dir}/repo.json`.
pub fn repo_meta_path(repo_id: &str) -> PathBuf {
    branch::data_dir(repo_id).join("repo.json")
}

/// Read `repo.json` from an explicit store directory.
pub fn read_repo_meta_in_dir(dir: &Path) -> Result<Option<RepoMeta>> {
    let path = dir.join("repo.json");
    match fs::read_to_string(&path) {
        Ok(raw) => {
            let meta: RepoMeta = serde_json::from_str(&raw)
                .map_err(|e| GitCortexError::Store(format!("parse {}: {e}", path.display())))?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(GitCortexError::Io(e)),
    }
}

/// Read `{data_dir}/repo.json` for `repo_id`.
pub fn read_repo_meta(repo_id: &str) -> Result<Option<RepoMeta>> {
    read_repo_meta_in_dir(&branch::data_dir(repo_id))
}

/// Atomically write `repo.json` inside an explicit store directory.
pub fn write_repo_meta_in_dir(dir: &Path, meta: &RepoMeta) -> Result<()> {
    write_json_atomic(&dir.join("repo.json"), meta)
}

/// Atomically write `{data_dir}/repo.json` for `repo_id`.
pub fn write_repo_meta(repo_id: &str, meta: &RepoMeta) -> Result<()> {
    write_repo_meta_in_dir(&branch::data_dir(repo_id), meta)
}

/// Record a store open, creating `repo.json` on first open and throttling
/// subsequent `last_opened_at` rewrites to at most once per
/// [`LAST_OPENED_THROTTLE_SECS`] (3600 s).
pub fn record_repo_open(repo_id: &str, repo_root: &Path) -> Result<RepoMeta> {
    record_repo_open_at(repo_id, repo_root, now_unix_secs())
}

/// Like [`record_repo_open`] with an explicit Unix timestamp (for testing).
pub fn record_repo_open_at(repo_id: &str, repo_root: &Path, now: u64) -> Result<RepoMeta> {
    let worktree_root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let git_common_dir = branch::resolve_git_common_dir(&worktree_root);

    if let Some(existing) = read_repo_meta(repo_id)? {
        if existing.format == META_FORMAT_VERSION
            && existing.identity_version == branch::IDENTITY_VERSION
            && now.saturating_sub(existing.last_opened_at) < LAST_OPENED_THROTTLE_SECS
        {
            return Ok(existing);
        }
        let updated = RepoMeta {
            format: META_FORMAT_VERSION,
            identity_version: branch::IDENTITY_VERSION,
            worktree_root,
            git_common_dir,
            created_at: existing.created_at,
            last_opened_at: now,
        };
        write_repo_meta(repo_id, &updated)?;
        return Ok(updated);
    }

    let meta = RepoMeta {
        format: META_FORMAT_VERSION,
        identity_version: branch::IDENTITY_VERSION,
        worktree_root,
        git_common_dir,
        created_at: now,
        last_opened_at: now,
    };
    write_repo_meta(repo_id, &meta)?;
    Ok(meta)
}

// ── Branch registry (`branches.json`) ────────────────────────────────────────

/// Path to `{data_dir}/branches.json`.
pub fn branch_registry_path(repo_id: &str) -> PathBuf {
    branch::data_dir(repo_id).join("branches.json")
}

/// Read `branches.json` from an explicit store directory.
pub fn read_branch_registry_in_dir(dir: &Path) -> Result<Vec<BranchEntry>> {
    let path = dir.join("branches.json");
    match fs::read_to_string(&path) {
        Ok(raw) => {
            let entries: Vec<BranchEntry> = serde_json::from_str(&raw)
                .map_err(|e| GitCortexError::Store(format!("parse {}: {e}", path.display())))?;
            Ok(entries)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(GitCortexError::Io(e)),
    }
}

/// Read `{data_dir}/branches.json` for `repo_id`.
pub fn read_branch_registry(repo_id: &str) -> Result<Vec<BranchEntry>> {
    read_branch_registry_in_dir(&branch::data_dir(repo_id))
}

/// Atomically write `{data_dir}/branches.json` for `repo_id`.
pub fn write_branch_registry(repo_id: &str, entries: &[BranchEntry]) -> Result<()> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.key.cmp(&b.key).then_with(|| a.name.cmp(&b.name)));
    write_json_atomic(&branch_registry_path(repo_id), &sorted)
}

/// Upsert a [`BranchEntry`] by its sanitized `key` in `{data_dir}/branches.json`.
pub fn upsert_branch(repo_id: &str, entry: BranchEntry) -> Result<()> {
    let mut entries = read_branch_registry(repo_id)?;
    if let Some(existing) = entries.iter_mut().find(|e| e.key == entry.key) {
        *existing = entry;
    } else {
        entries.push(entry);
    }
    write_branch_registry(repo_id, &entries)
}

fn build_branch_entry(branch_name: &str, last_indexed_at: u64) -> BranchEntry {
    let kind = branch::key_kind(branch_name);
    let worktree_id = match kind {
        BranchKind::Detached => branch::detached_worktree_id(branch_name),
        BranchKind::Branch | BranchKind::Commit => None,
    };
    BranchEntry {
        name: branch_name.to_owned(),
        key: branch::sanitize(branch_name),
        kind,
        worktree_id,
        last_indexed_at,
    }
}

/// Ensure `branch_name` has an entry in `branches.json` without rewriting the
/// registry if its key is already present.
pub fn ensure_branch_registered(repo_id: &str, branch_name: &str) -> Result<()> {
    let key = branch::sanitize(branch_name);
    let entries = read_branch_registry(repo_id)?;
    if entries.iter().any(|e| e.key == key) {
        return Ok(());
    }
    upsert_branch(repo_id, build_branch_entry(branch_name, now_unix_secs()))
}

/// Record (or refresh) `branch_name` in `branches.json` with the current timestamp.
pub fn record_branch_indexed(repo_id: &str, branch_name: &str) -> Result<()> {
    upsert_branch(repo_id, build_branch_entry(branch_name, now_unix_secs()))
}

// ── Automatic branch GC timestamp (`last_branch_gc`) ─────────────────────────

/// Minimum interval (in seconds) between automatic hook branch-GC runs (24 hours).
pub const BRANCH_GC_INTERVAL_SECS: u64 = 86_400;

/// Path to `{data_dir}/last_branch_gc`.
pub fn last_branch_gc_path(repo_id: &str) -> PathBuf {
    branch::data_dir(repo_id).join("last_branch_gc")
}

/// Read the Unix timestamp of the last automatic branch GC run, if any.
pub fn read_last_branch_gc(repo_id: &str) -> Option<u64> {
    fs::read_to_string(last_branch_gc_path(repo_id))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Write the Unix timestamp of the last automatic branch GC run.
pub fn write_last_branch_gc(repo_id: &str, now: u64) -> Result<()> {
    let path = last_branch_gc_path(repo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, format!("{now}\n")).map_err(GitCortexError::Io)
}

/// Whether automatic hook branch GC is due at Unix time `now`.
pub fn branch_gc_due(repo_id: &str, now: u64) -> bool {
    match read_last_branch_gc(repo_id) {
        None => true,
        Some(last) => now.saturating_sub(last) >= BRANCH_GC_INTERVAL_SECS,
    }
}

/// Whether automatic hook branch GC is due right now.
pub fn branch_gc_due_now(repo_id: &str) -> bool {
    branch_gc_due(repo_id, now_unix_secs())
}

/// Record the current time as the last automatic branch GC run.
pub fn mark_branch_gc_now(repo_id: &str) -> Result<()> {
    write_last_branch_gc(repo_id, now_unix_secs())
}
