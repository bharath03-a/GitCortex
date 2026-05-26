use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_store::branch;

/// Wipe the entire graph store for this repo (all branches).
/// The next `gcx hook` or `gcx init` will perform a full re-index.
pub fn run() -> Result<()> {
    let repo_root = repo_root()?;
    let repo_id = branch::repo_id(&repo_root);
    let data_dir = branch::data_dir(&repo_id);

    if !data_dir.exists() {
        println!("nothing to clean (no data at {})", data_dir.display());
        return Ok(());
    }

    std::fs::remove_dir_all(&data_dir)
        .with_context(|| format!("failed to remove {}", data_dir.display()))?;

    println!("cleaned: {}", data_dir.display());
    println!("run `gcx init` or make a commit to trigger a fresh full index");
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
