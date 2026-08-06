use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result};

pub fn repo_root() -> Result<PathBuf> {
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

/// Replace a text file atomically while retaining its existing permissions.
pub fn write_atomic(path: &std::path::Path, content: &str) -> Result<()> {
    let target = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(path)
            .with_context(|| format!("resolve symlink {}", path.display()))?,
        _ => path.to_owned(),
    };
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let permissions = std::fs::metadata(&target)
        .ok()
        .map(|meta| meta.permissions());
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(content.as_bytes())?;
    temp.as_file_mut().sync_all()?;
    if let Some(permissions) = permissions {
        temp.as_file().set_permissions(permissions)?;
    }
    temp.persist(&target)
        .map_err(|error| error.error)
        .with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

pub fn require_json_object(value: &serde_json::Value, path: &std::path::Path) -> Result<()> {
    if !value.is_object() {
        anyhow::bail!("{} must contain a JSON object", path.display());
    }
    Ok(())
}

pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn current_branch(repo_root: &std::path::Path) -> Result<String> {
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
            .output()
            .context("git rev-parse HEAD failed")?;
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}
