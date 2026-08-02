use std::{fs::OpenOptions, path::Path};

use anyhow::Result;
use gitcortex_store::branch;

/// Whether a `gcx serve` process currently owns this repository's graph.
/// Advisory file locks are released automatically if the server crashes.
pub fn is_active(repo_root: &Path) -> Result<bool> {
    let repo_id = branch::storage_repo_id(repo_root);
    let path = branch::data_dir(&repo_id).join("serve.lock");
    if !path.exists() {
        return Ok(false);
    }
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    match fs2::FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            fs2::FileExt::unlock(&file)?;
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(error.into()),
    }
}
