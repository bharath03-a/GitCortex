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

/// Blob hash of `.gitcortex/ignore` committed in `rev`'s tree, or `None` when
/// absent from that commit.
pub fn committed_rules_hash(root: &Path, rev: &str) -> anyhow::Result<Option<String>> {
    let repo = git2::Repository::open(root)?;
    let tree = repo.find_commit(git2::Oid::from_str(rev)?)?.tree()?;
    match tree.get_path(Path::new(IGNORE_FILE)) {
        Ok(entry) => Ok(Some(entry.id().to_string())),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Whether committed rules at `head` were edited or deleted in the working tree.
pub fn rules_modified(root: &Path, head: &str) -> anyhow::Result<bool> {
    let Some(committed) = committed_rules_hash(root, head)? else {
        return Ok(false);
    };
    Ok(rules_hash(root)?.as_deref() != Some(committed.as_str()))
}

/// Persist the `.gitcortex/ignore` rules hash under which `branch` was indexed.
pub fn persist_indexed_rules(root: &Path, branch: &str, hash: Option<&str>) -> anyhow::Result<()> {
    let repo_id = gitcortex_store::branch::storage_repo_id(root);
    gitcortex_store::branch::write_indexed_ignore_hash(&repo_id, branch, hash)?;
    Ok(())
}

/// Whether `branch`'s persisted `.ignore` rules marker agrees with `current`.
///
/// Legacy stores that predate `.ignore` markers (marker absent) are treated as
/// matching only when no `.gitcortex/ignore` file is present on disk (`None`),
/// so an untracked `.gitcortex/ignore` created or edited offline always fails
/// closed to `unverified`.
pub fn indexed_rules_match(
    root: &Path,
    branch: &str,
    current: &Option<String>,
) -> anyhow::Result<bool> {
    let repo_id = gitcortex_store::branch::storage_repo_id(root);
    match gitcortex_store::branch::read_indexed_ignore_hash(&repo_id, branch)? {
        Some(recorded) => Ok(recorded == *current),
        None => Ok(current.is_none()),
    }
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
