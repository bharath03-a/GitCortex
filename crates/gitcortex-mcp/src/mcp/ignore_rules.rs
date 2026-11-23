//! The `.gitcortex/ignore` rules a working-tree graph was indexed under.
use std::{io::Read, path::Path};

const IGNORE_FILE: &str = ".gitcortex/ignore";
const MAX_IGNORE_BYTES: u64 = 256 * 1024;

/// Blob hash of the working-tree rules file, `None` when it is absent. Errors
/// when the file cannot be hashed safely (a symlink, directory, or oversized).
pub fn rules_hash(root: &Path) -> anyhow::Result<Option<String>> {
    let path = root.join(IGNORE_FILE);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= MAX_IGNORE_BYTES,
        "{IGNORE_FILE} is not a regular file within {MAX_IGNORE_BYTES} bytes"
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(&path)?
        .take(MAX_IGNORE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_IGNORE_BYTES,
        "{IGNORE_FILE} grew while hashing"
    );
    Ok(Some(
        git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)?.to_string(),
    ))
}

/// Whether committed rules at `head` were edited or deleted in the working tree.
/// Untracked rules (as `gcx init` writes them) are covered by the overlay
/// checkpoint's recorded hash and the watcher's reconcile instead.
pub fn rules_modified(root: &Path, head: &str) -> anyhow::Result<bool> {
    let repo = git2::Repository::open(root)?;
    let tree = repo.find_commit(git2::Oid::from_str(head)?)?.tree()?;
    let committed = match tree.get_path(Path::new(IGNORE_FILE)) {
        Ok(entry) => entry.id().to_string(),
        Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(rules_hash(root)?.as_deref() != Some(committed.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::test_support::{commit_file, TempRepo};

    #[test]
    fn untracked_rules_are_not_modified_but_edited_tracked_rules_are() {
        let repo = TempRepo::init();
        let head = commit_file(repo.path(), "a.rs", "pub fn a() {}\n", "a");
        assert_eq!(rules_hash(repo.path()).expect("hash"), None);
        std::fs::create_dir_all(repo.path().join(".gitcortex")).expect("mkdir");
        std::fs::write(repo.path().join(IGNORE_FILE), "x.rs\n").expect("rules");
        assert!(!rules_modified(repo.path(), &head).expect("untracked"));
        let head = commit_file(repo.path(), IGNORE_FILE, "x.rs\n", "rules");
        assert!(!rules_modified(repo.path(), &head).expect("committed"));
        std::fs::write(repo.path().join(IGNORE_FILE), "y.rs\n").expect("edit");
        assert!(rules_modified(repo.path(), &head).expect("edited"));
        std::fs::remove_file(repo.path().join(IGNORE_FILE)).expect("delete");
        assert!(rules_modified(repo.path(), &head).expect("deleted"));
    }

    #[test]
    fn symlinked_rules_cannot_be_hashed() {
        let repo = TempRepo::init();
        std::fs::create_dir_all(repo.path().join(".gitcortex")).expect("mkdir");
        std::fs::write(repo.path().join("rules"), "x.rs\n").expect("target");
        std::os::unix::fs::symlink(repo.path().join("rules"), repo.path().join(IGNORE_FILE))
            .expect("symlink");
        assert!(rules_hash(repo.path()).is_err());
    }
}
