use std::{
    io::Write,
    path::{Component, Path, PathBuf},
};

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
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    reject_symlink_components(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)?;
    reject_symlink_components(path)?;
    let permissions = std::fs::metadata(path).ok().map(|meta| meta.permissions());
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(content.as_bytes())?;
    temp.as_file_mut().sync_all()?;
    if let Some(permissions) = permissions {
        temp.as_file().set_permissions(permissions)?;
    }
    temp.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn reject_symlink_components(path: &Path) -> Result<()> {
    if std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        anyhow::bail!("refusing to write through symlink {}", path.display());
    }

    let repo_root = path
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists());
    let Some(repo_root) = repo_root else {
        return Ok(());
    };
    let relative = path.strip_prefix(repo_root).with_context(|| {
        format!(
            "{} is not contained by repository {}",
            path.display(),
            repo_root.display()
        )
    })?;
    let mut current = repo_root.to_owned();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            anyhow::bail!("{} contains unsupported path traversal", path.display());
        };
        current.push(segment);
        if std::fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            anyhow::bail!("refusing to write through symlink {}", current.display());
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn write_atomic_rejects_symlink_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        let external = outside.path().join("config.json");
        std::fs::write(&external, "original").expect("write external file");
        let link = dir.path().join("config.json");
        symlink(&external, &link).expect("create symlink");

        let error = write_atomic(&link, "replacement").expect_err("symlink must be rejected");
        assert!(error.to_string().contains("symlink"));
        assert_eq!(
            std::fs::read_to_string(external).expect("read external file"),
            "original"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_rejects_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let repo = tempfile::tempdir().expect("repo");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir(repo.path().join(".git")).expect("create git marker");
        let external = outside.path().join("config.json");
        std::fs::write(&external, "original").expect("write external file");
        symlink(outside.path(), repo.path().join("config")).expect("create parent symlink");
        let path = repo.path().join("config/config.json");

        let error = write_atomic(&path, "replacement").expect_err("symlinked parent must fail");
        assert!(error.to_string().contains("symlink"));
        assert_eq!(
            std::fs::read_to_string(external).expect("read external file"),
            "original"
        );
    }
}
