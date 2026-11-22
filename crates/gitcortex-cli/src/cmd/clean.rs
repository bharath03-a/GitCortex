use anyhow::{Context, Result};
use gitcortex_store::branch;

use super::{init::helpers::repo_root, serve_lock};

/// Wipe the entire graph store for this repo (all branches).
/// The next `gcx hook` or `gcx init` will perform a full re-index.
pub fn run() -> Result<()> {
    let repo_root = repo_root()?;
    let _repository_lock = serve_lock::acquire_mutation(&repo_root)?;
    let repo_id = branch::storage_repo_id(&repo_root);
    let data_dir = branch::data_dir(&repo_id);
    if !branch::has_repo_data(&repo_id)? {
        println!("nothing to clean (no graph data at {})", data_dir.display());
        return Ok(());
    }

    branch::wipe_repo_data(&repo_id)
        .with_context(|| format!("failed to clean {}", data_dir.display()))?;

    println!("cleaned: {}", data_dir.display());
    println!("run `gcx init` or make a commit to trigger a fresh full index");
    Ok(())
}
