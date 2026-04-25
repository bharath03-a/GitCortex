use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

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
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "b_");
    }
    s
}

// ── Repository identity ───────────────────────────────────────────────────────

/// Derive a stable 16-hex-char ID from the absolute repo root path.
/// Used to namespace per-repo data directories without path encoding issues.
pub fn repo_id(repo_root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    repo_root.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ── XDG data paths ────────────────────────────────────────────────────────────

/// Root data directory for a repo: `$XDG_DATA_HOME/gitcortex/{repo_id}/`
pub fn data_dir(repo_id: &str) -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".local/share"));
    base.join("gitcortex").join(repo_id)
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Path to the single KuzuDB file for a repo (all branches, namespaced by table prefix).
pub fn db_path(repo_id: &str) -> PathBuf {
    data_dir(repo_id).join("graph.kuzu")
}

/// Path to the last-indexed SHA file for a specific branch.
pub fn last_sha_path(repo_id: &str, branch: &str) -> PathBuf {
    data_dir(repo_id).join(format!("{}.sha", sanitize(branch)))
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
        assert_eq!(repo_id(path), repo_id(path));
    }

    #[test]
    fn repo_id_differs_across_paths() {
        let a = repo_id(Path::new("/home/user/proj-a"));
        let b = repo_id(Path::new("/home/user/proj-b"));
        assert_ne!(a, b);
    }
}
