use std::{
    fs::{self, File, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use directories::BaseDirs;
use gitcortex_core::error::{GitCortexError, Result};
use serde::{Deserialize, Serialize};

/// Version of the repository storage identity scheme.
///
/// External checkpoint and cache consumers include this value in their cache
/// keys so an identity scheme change cleanly invalidates older checkpoints.
pub const IDENTITY_VERSION: u32 = 1;

// ── Branch key classification ────────────────────────────────────────────────

/// Classification of a branch/snapshot key stored inside a repository graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchKind {
    /// Normal symbolic branch ref (e.g. `main`, `feat/auth`).
    Branch,
    /// Worktree detached-HEAD snapshot key (short SHA or `detached_<wt>_<sha>`).
    Detached,
    /// Immutable commit-tree snapshot keyed by a full 40- or 64-char hex SHA.
    Commit,
}

fn is_lower_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Extract the 8-hex-character worktree id from a `detached_<wt>_<sha>` key,
/// if present.
pub fn detached_worktree_id(name: &str) -> Option<String> {
    let rest = name.strip_prefix("detached_")?;
    let (wt, sha) = rest.split_once('_')?;
    if wt.len() == 8 && is_lower_hex(wt) && !sha.is_empty() && is_lower_hex(sha) {
        Some(wt.to_owned())
    } else {
        None
    }
}

/// Classify a graph key into [`BranchKind`]. This is the single authoritative
/// classifier used by branch registration and garbage collection.
///
/// - Full 40- or 64-char lowercase hex object ids → [`BranchKind::Commit`].
/// - `detached_<8hex>_<short_sha>` or pre-worktree short lowercase hex SHAs
///   (7..40 chars) → [`BranchKind::Detached`].
/// - Everything else → [`BranchKind::Branch`].
pub fn key_kind(name: &str) -> BranchKind {
    if (name.len() == 40 || name.len() == 64) && is_lower_hex(name) {
        return BranchKind::Commit;
    }
    if detached_worktree_id(name).is_some() {
        return BranchKind::Detached;
    }
    if (7..40).contains(&name.len()) && is_lower_hex(name) {
        return BranchKind::Detached;
    }
    BranchKind::Branch
}

static ATOMIC_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `content` to `path` via a same-directory temporary file and atomic
/// rename so concurrent readers never observe a truncated or partial file.
pub fn write_atomic(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let seq = ATOMIC_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "meta".to_owned());
    let tmp = path.with_file_name(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));
    if let Err(error) = fs::write(&tmp, content) {
        let _ = fs::remove_file(&tmp);
        return Err(GitCortexError::Io(error));
    }
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(GitCortexError::Io(error));
    }
    Ok(())
}

// ── Branch name sanitization ──────────────────────────────────────────────────

/// Sanitize a branch name so it can be used as a KuzuDB table name prefix.
///
/// Rules applied (in order):
/// - `/`  → `__`  (preserves branch hierarchy visibility)
/// - any remaining non-alphanumeric char → `_`
/// - leading digit → prefix with `b_` (table names can't start with a digit)
/// - transformed refs receive a short digest suffix to prevent collisions
///
/// Examples:
/// - `main`           → `main`
/// - `feat/auth`      → `feat__auth`
/// - `feat/auth-v2`   → `feat__auth_v2`
/// - `release/v1.0`   → `release__v1_0`
pub fn sanitize(branch: &str) -> String {
    let expanded = branch.replace('/', "__");
    let mut s: String = expanded
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "b_");
    }
    if s != branch {
        let digest = blake3::hash(branch.as_bytes());
        s.push_str("__");
        s.push_str(&digest.to_hex()[..8]);
    }
    s
}

// ── Repository identity ───────────────────────────────────────────────────────

/// Derive a stable 16-hex-character ID from the repo's absolute path.
///
/// BLAKE3 is deliberately specified here instead of `DefaultHasher`, whose
/// algorithm is an implementation detail and may change between Rust releases.
pub fn repo_id(repo_root: &Path) -> String {
    let digest = blake3::hash(repo_root.to_string_lossy().as_bytes());
    digest.to_hex()[..16].to_owned()
}

/// Derive a stable 8-hex-character worktree identifier from a canonicalized
/// worktree root path.
pub fn worktree_id(repo_root: &Path) -> String {
    let canonical = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let digest = blake3::hash(canonical.to_string_lossy().as_bytes());
    digest.to_hex()[..8].to_owned()
}

fn legacy_repo_id(repo_root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    repo_root.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Resolve the ID used on disk, retaining access to stores created before the
/// stable BLAKE3 ID was introduced. New repositories always use [`repo_id`].
pub fn storage_repo_id(repo_root: &Path) -> String {
    let stable = repo_id(repo_root);
    if data_dir(&stable).exists() {
        return stable;
    }

    let legacy = legacy_repo_id(repo_root);
    if data_dir(&legacy).exists() {
        legacy
    } else {
        stable
    }
}

/// Resolve the Git common directory for `repo_root` using filesystem inspection
/// (without `git2` or spawning a subprocess).
///
/// - Normal checkout: `<root>/.git` directory.
/// - Linked worktree / submodule: `<root>/.git` file pointing to `gitdir:`,
///   following `<gitdir>/commondir` when present.
/// - Bare repo or non-git directory: falls back to canonicalized `repo_root`.
pub fn resolve_git_common_dir(repo_root: &Path) -> PathBuf {
    let canonical_root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let dot_git = canonical_root.join(".git");
    if dot_git.is_dir() {
        return fs::canonicalize(&dot_git).unwrap_or(dot_git);
    }
    if dot_git.is_file() {
        if let Ok(contents) = fs::read_to_string(&dot_git) {
            if let Some(raw_gitdir) = contents
                .lines()
                .find_map(|line| line.trim().strip_prefix("gitdir:"))
            {
                let gitdir_rel = PathBuf::from(raw_gitdir.trim());
                let gitdir = if gitdir_rel.is_absolute() {
                    gitdir_rel
                } else {
                    canonical_root.join(gitdir_rel)
                };
                let canonical_gitdir = fs::canonicalize(&gitdir).unwrap_or(gitdir);
                let commondir_file = canonical_gitdir.join("commondir");
                if let Ok(common_raw) = fs::read_to_string(&commondir_file) {
                    let common_trimmed = common_raw.trim();
                    if !common_trimmed.is_empty() {
                        let common_rel = PathBuf::from(common_trimmed);
                        let common = if common_rel.is_absolute() {
                            common_rel
                        } else {
                            canonical_gitdir.join(common_rel)
                        };
                        return fs::canonicalize(&common).unwrap_or(common);
                    }
                }
                return canonical_gitdir;
            }
        }
    }
    canonical_root
}

// ── Platform data and cache paths ─────────────────────────────────────────────

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| BaseDirs::new().map(|dirs| dirs.home_dir().to_owned()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Machine-local durable data root.
///
/// `GCX_STORE_PATH` is the explicit application override. Otherwise the native
/// platform data directory is used (`$XDG_DATA_HOME` on Linux and
/// `~/Library/Application Support` on macOS). Existing macOS installations in
/// `~/.local/share/gitcortex` continue using that location until moved.
pub fn data_root() -> PathBuf {
    if let Some(path) = std::env::var_os("GCX_STORE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("gitcortex");
    }

    let native = BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().join("gitcortex"))
        .unwrap_or_else(|| home_dir().join(".local/share/gitcortex"));
    let legacy = home_dir().join(".local/share/gitcortex");
    if cfg!(target_os = "macos") && legacy.exists() && !native.exists() {
        legacy
    } else {
        native
    }
}

/// Root data directory for a repository.
pub fn data_dir(repo_id: &str) -> PathBuf {
    data_root().join(repo_id)
}

/// Cross-process ownership guard for operations that may open or mutate one
/// repository's embedded graph. The operating system releases it on crashes.
pub struct RepositoryLock {
    file: File,
}

impl RepositoryLock {
    /// Try to claim repository ownership without waiting.
    pub fn try_acquire(repo_root: &Path) -> Result<Option<Self>> {
        let repo_id = storage_repo_id(repo_root);
        let dir = data_dir(&repo_id);
        fs::create_dir_all(&dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("serve.lock"))?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(Self { file })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(GitCortexError::Io(error)),
        }
    }

    /// Read the diagnostic owner text left by the current or previous owner.
    pub fn owner(&mut self) -> String {
        let mut owner = String::new();
        if self.file.rewind().is_ok() {
            let _ = self.file.read_to_string(&mut owner);
        }
        owner.trim().to_owned()
    }

    /// Replace diagnostic owner text after ownership has been acquired.
    pub fn set_owner(&mut self, owner: &str) -> Result<()> {
        self.file.set_len(0)?;
        self.file.rewind()?;
        write!(self.file, "{owner}")?;
        self.file.sync_data()?;
        Ok(())
    }
}

pub fn repository_lock_owner(repo_root: &Path) -> String {
    let repo_id = storage_repo_id(repo_root);
    fs::read_to_string(data_dir(&repo_id).join("serve.lock"))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Machine-local cache root. Downloadable model weights are cache data, not
/// durable application state.
pub fn cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("GCX_CACHE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path).join("gitcortex");
    }
    BaseDirs::new()
        .map(|dirs| dirs.cache_dir().join("gitcortex"))
        .unwrap_or_else(|| home_dir().join(".cache/gitcortex"))
}

/// Shared model cache directory. On first use, migrate the legacy model cache
/// out of the durable data directory when a same-filesystem rename is possible.
pub fn models_dir() -> PathBuf {
    let target = cache_root().join("models");
    let legacy = data_root().join("models");
    if !target.exists() && legacy.exists() {
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::rename(&legacy, &target).is_err() {
            return legacy;
        }
    }
    target
}

/// Path to the single KuzuDB file for a repo (all branches, namespaced by table prefix).
pub fn db_path(repo_id: &str) -> PathBuf {
    data_dir(repo_id).join("graph.kuzu")
}

/// KuzuDB payload files inside an explicit store directory (`graph.kuzu` and
/// its write-ahead log `graph.kuzu.wal`). Single source of truth shared by
/// data-dir inventory and compaction.
pub fn payload_files_in_dir(dir: &Path) -> [PathBuf; 2] {
    [dir.join("graph.kuzu"), dir.join("graph.kuzu.wal")]
}

/// KuzuDB payload files for `repo_id` (`graph.kuzu` and `graph.kuzu.wal`).
pub fn payload_files(repo_id: &str) -> [PathBuf; 2] {
    payload_files_in_dir(&data_dir(repo_id))
}

/// Path to the last-indexed SHA file for a specific branch.
pub fn last_sha_path(repo_id: &str, branch: &str) -> PathBuf {
    data_dir(repo_id).join(format!("{}.sha", sanitize(branch)))
}

/// Path to the persisted schema version marker for a repo.
pub fn schema_version_path(repo_id: &str) -> PathBuf {
    data_dir(repo_id).join("schema_version")
}

/// Read the persisted schema version, returning 0 if not present.
pub fn read_schema_version(repo_id: &str) -> u32 {
    let path = schema_version_path(repo_id);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Write the schema version marker atomically.
pub fn write_schema_version(repo_id: &str, version: u32) -> Result<()> {
    let path = schema_version_path(repo_id);
    write_atomic(&path, version.to_string().as_bytes())
}

/// Path to the marker recording that the interactive `gcx init` editor
/// prompt has already run for this repository on this machine.
pub fn editor_prompt_marker_path(repo_id: &str) -> PathBuf {
    data_dir(repo_id).join("editor_prompted")
}

/// Whether the interactive editor prompt has already run for this repo.
/// Machine-local (not repo-committed): editor choice is a per-developer
/// preference, not a team-wide setting.
pub fn has_prompted_for_editor(repo_id: &str) -> bool {
    editor_prompt_marker_path(repo_id).exists()
}

/// Record that the interactive editor prompt has run, regardless of what
/// (if anything) the user picked, so `gcx init` never asks again — use
/// `gcx init --editor <name>` to add an assistant later.
pub fn mark_editor_prompted(repo_id: &str) -> Result<()> {
    let path = editor_prompt_marker_path(repo_id);
    write_atomic(&path, b"")
}

/// Whether a repository data directory contains anything other than its
/// reusable ownership lock file.
pub fn has_repo_data(repo_id: &str) -> Result<bool> {
    let entries = match fs::read_dir(data_dir(repo_id)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(GitCortexError::Io(error)),
    };
    for entry in entries {
        if entry?.file_name() != "serve.lock" {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Wipe per-repository graph data while preserving the advisory ownership
/// lock inode. Keeping the lock file in place prevents a new daemon from
/// slipping through while an explicit clean or schema rebuild is in progress.
pub fn wipe_repo_data(repo_id: &str) -> Result<()> {
    let dir = data_dir(repo_id);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(GitCortexError::Io(error)),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_name() == "serve.lock" {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

// ── last_sha persistence ──────────────────────────────────────────────────────

pub fn read_last_sha(repo_id: &str, branch: &str) -> Result<Option<String>> {
    let path = last_sha_path(repo_id, branch);
    match fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s.trim().to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(GitCortexError::Io(e)),
    }
}

pub fn write_last_sha(repo_id: &str, branch: &str, sha: &str) -> Result<()> {
    let path = last_sha_path(repo_id, branch);
    write_atomic(&path, sha.as_bytes())
}

/// Path to the persisted `.gitcortex/ignore` rules hash for a specific branch.
pub fn indexed_ignore_path(repo_id: &str, branch: &str) -> PathBuf {
    data_dir(repo_id).join(format!("{}.ignore", sanitize(branch)))
}

/// Read the persisted `.gitcortex/ignore` blob hash under which `branch` was
/// last indexed:
/// - `Ok(None)`: no `.ignore` marker file exists (legacy store or direct SHA write).
/// - `Ok(Some(None))`: indexed with no `.gitcortex/ignore` file present.
/// - `Ok(Some(Some(oid)))`: indexed with `.gitcortex/ignore` matching `oid`.
pub fn read_indexed_ignore_hash(repo_id: &str, branch: &str) -> Result<Option<Option<String>>> {
    let path = indexed_ignore_path(repo_id, branch);
    match fs::read_to_string(&path) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed == "none" {
                Ok(Some(None))
            } else {
                Ok(Some(Some(trimmed.to_owned())))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(GitCortexError::Io(e)),
    }
}

/// Atomically persist the `.gitcortex/ignore` blob hash under which `branch`
/// was indexed (`None` when no `.gitcortex/ignore` file was present).
pub fn write_indexed_ignore_hash(repo_id: &str, branch: &str, hash: Option<&str>) -> Result<()> {
    let path = indexed_ignore_path(repo_id, branch);
    let value = hash.unwrap_or("none");
    write_atomic(&path, value.as_bytes())
}

/// Path to the monotonic mutation epoch file for a specific branch.
pub fn mutation_epoch_path(repo_id: &str, branch: &str) -> PathBuf {
    data_dir(repo_id).join(format!("{}.epoch", sanitize(branch)))
}

/// Read the monotonic mutation epoch for `branch`, returning `0` when absent.
pub fn read_mutation_epoch(repo_id: &str, branch: &str) -> Result<u64> {
    let path = mutation_epoch_path(repo_id, branch);
    match fs::read_to_string(&path) {
        Ok(s) => s.trim().parse::<u64>().map_err(|e| {
            GitCortexError::Store(format!("corrupt mutation epoch in {}: {e}", path.display()))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(GitCortexError::Io(e)),
    }
}

/// Increment and persist the monotonic mutation epoch for `branch` atomically.
pub fn bump_mutation_epoch(repo_id: &str, branch: &str) -> Result<u64> {
    let next = read_mutation_epoch(repo_id, branch)?.saturating_add(1);
    let path = mutation_epoch_path(repo_id, branch);
    write_atomic(&path, next.to_string().as_bytes())?;
    Ok(next)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_plain() {
        assert_eq!(sanitize("main"), "main");
    }

    #[test]
    fn sanitize_slash_becomes_double_underscore() {
        assert!(sanitize("feat/auth").starts_with("feat__auth__"));
    }

    #[test]
    fn sanitize_dash_and_dot() {
        assert!(sanitize("release/v1.0-rc").starts_with("release__v1_0_rc__"));
    }

    #[test]
    fn sanitize_leading_digit() {
        assert!(sanitize("1-hotfix").starts_with("b_1_hotfix__"));
    }

    #[test]
    fn sanitized_storage_keys_do_not_collide() {
        assert_ne!(sanitize("feat/a-b"), sanitize("feat/a.b"));
    }

    #[test]
    fn repo_id_is_stable() {
        let path = Path::new("/home/user/myproject");
        assert_eq!(repo_id(path), "b6dd9f32aba035a6");
    }

    #[test]
    fn repo_id_differs_across_paths() {
        let a = repo_id(Path::new("/home/user/proj-a"));
        let b = repo_id(Path::new("/home/user/proj-b"));
        assert_ne!(a, b);
    }

    #[test]
    fn write_atomic_replaces_content_without_leaving_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("main.sha");
        write_atomic(&target, b"1111111").expect("first write");
        assert_eq!(fs::read_to_string(&target).expect("read"), "1111111");
        write_atomic(&target, b"2222222").expect("second write");
        assert_eq!(fs::read_to_string(&target).expect("read"), "2222222");
        let entries: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("main.sha")]);
    }
}
