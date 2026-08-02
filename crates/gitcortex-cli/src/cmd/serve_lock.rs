use std::path::Path;

use anyhow::Result;
use gitcortex_store::branch::{self, RepositoryLock};

/// Whether a process currently owns this repository's embedded graph.
/// Advisory file locks are released automatically if the owner crashes.
pub fn is_active(repo_root: &Path) -> Result<bool> {
    let repo_id = branch::storage_repo_id(repo_root);
    if !branch::data_dir(&repo_id).join("serve.lock").exists() {
        return Ok(false);
    }
    Ok(RepositoryLock::try_acquire(repo_root)?.is_none())
}

/// Hold exclusive repository ownership across a destructive filesystem
/// operation, eliminating the check-then-delete race with daemon startup.
pub fn acquire_mutation(repo_root: &Path) -> Result<RepositoryLock> {
    let mut lock = RepositoryLock::try_acquire(repo_root)?.ok_or_else(|| {
        let owner = branch::repository_lock_owner(repo_root);
        anyhow::anyhow!(
            "repository graph is active{}; close editor MCP sessions and stop `gcx viz`, then retry",
            if owner.is_empty() {
                String::new()
            } else {
                format!(" ({owner})")
            }
        )
    })?;
    lock.set_owner(&format!("pid {} (CLI mutation)", std::process::id()))?;
    Ok(lock)
}
