use std::{
    fs::{self, File, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use gitcortex_core::error::{GitCortexError, Result};

// ── Branch name sanitization ──────────────────────────────────────────────────

/// Sanitize a branch name so it can be used as a KuzuDB table name prefix.
///
/// Rules applied (in order):
/// - `/`  → `__`  (preserves branch hierarchy visibility)
/// - any remaining non-alphanumeric char → `_`
/// - leading digit → prefix with `b_` (table names can't start with a digit)
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

/// Write the schema version marker.
pub fn write_schema_version(repo_id: &str, version: u32) -> Result<()> {
    let path = schema_version_path(repo_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, version.to_string()).map_err(GitCortexError::Io)
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, "").map_err(GitCortexError::Io)
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, sha).map_err(GitCortexError::Io)
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
        assert_eq!(sanitize("feat/auth"), "feat__auth");
    }

    #[test]
    fn sanitize_dash_and_dot() {
        assert_eq!(sanitize("release/v1.0-rc"), "release__v1_0_rc");
    }

    #[test]
    fn sanitize_leading_digit() {
        assert_eq!(sanitize("1-hotfix"), "b_1_hotfix");
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
}
