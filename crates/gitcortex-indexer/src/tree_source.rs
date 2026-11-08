use crate::{differ::FileChange, IncrementalIndexer};
use git2::{FileMode, ObjectType, Repository};
use gitcortex_core::error::{GitCortexError, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};

const IGNORE_FILE: &str = ".gitcortex/ignore";

fn git(error: git2::Error) -> GitCortexError {
    GitCortexError::Git(error.to_string())
}

fn is_regular(mode: i32) -> bool {
    mode == i32::from(FileMode::Blob) || mode == i32::from(FileMode::BlobExecutable)
}

/// Bytes of a repository-relative source; `None` means intentionally skipped.
pub(crate) trait SourceProvider: Sync {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>>;
}
pub(crate) struct DiskSource<'a>(pub &'a IncrementalIndexer);
impl SourceProvider for DiskSource<'_> {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        if self.0.should_ignore(path) {
            return Ok(None);
        }
        let absolute = self.0.validated_source_path(path)?;
        fs::read(&absolute)
            .map(Some)
            .map_err(|e| GitCortexError::Parse {
                file: absolute,
                message: e.to_string(),
            })
    }
}
/// Reads one commit's blobs; never consults the checkout, index, or HEAD.
pub(crate) struct TreeSource {
    // git2::Repository is Send but not Sync: parsing stays parallel while
    // blob reads are serialized.
    repo: Mutex<Repository>,
    tree: git2::Oid,
    pub sha: String,
    ignore: Gitignore,
}
impl TreeSource {
    pub fn new(root: &Path, rev: &str) -> Result<Self> {
        let repo = Repository::open(root).map_err(git)?;
        let (tree, sha, ignore) = {
            let commit = repo
                .revparse_single(rev)
                .and_then(|o| o.peel_to_commit())
                .map_err(git)?;
            let tree = commit.tree().map_err(git)?;
            let mut builder = GitignoreBuilder::new(root);
            if let Ok(entry) = tree.get_path(Path::new(IGNORE_FILE)) {
                let blob = repo.find_blob(entry.id()).map_err(git)?;
                for line in String::from_utf8_lossy(blob.content()).lines() {
                    builder
                        .add_line(None, line)
                        .map_err(|e| GitCortexError::Git(e.to_string()))?;
                }
            }
            let ignore = builder
                .build()
                .map_err(|e| GitCortexError::Git(e.to_string()))?;
            (tree.id(), commit.id().to_string(), ignore)
        };
        Ok(Self {
            repo: Mutex::new(repo),
            tree,
            sha,
            ignore,
        })
    }
    fn repo(&self) -> Result<MutexGuard<'_, Repository>> {
        self.repo
            .lock()
            .map_err(|_| GitCortexError::Git("tree source mutex poisoned".to_owned()))
    }
    fn excluded(&self, path: &Path) -> bool {
        self.ignore
            .matched_path_or_any_parents(path, false)
            .is_ignore()
    }
    pub fn changes(&self, from: Option<&str>, extensions: &[&str]) -> Result<Vec<FileChange>> {
        let repo = self.repo()?;
        let tree = repo.find_tree(self.tree).map_err(git)?;
        let old = from
            .map(|sha| repo.revparse_single(sha).and_then(|o| o.peel_to_tree()))
            .transpose()
            .map_err(git)?;
        // Exclusions can change without any source blob changing, so a rule
        // change re-evaluates every path.
        let ignore_id =
            |t: &git2::Tree<'_>| t.get_path(Path::new(IGNORE_FILE)).ok().map(|e| e.id());
        let rules_changed = old
            .as_ref()
            .is_some_and(|t| ignore_id(t) != ignore_id(&tree));
        let base = if rules_changed { None } else { old.as_ref() };
        let diff = repo
            .diff_tree_to_tree(base, Some(&tree), None)
            .map_err(git)?;
        let supported = |path: &Path| {
            path.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| extensions.contains(&ext))
        };
        let mut changes = Vec::new();
        for delta in diff.deltas() {
            let Some(path) = delta.new_file().path().or(delta.old_file().path()) else {
                continue;
            };
            if !supported(path) {
                continue;
            }
            // Symlinks and submodules are never parsed; treat a regular file
            // that became one as removed.
            let gone = delta.status() == git2::Delta::Deleted
                || !is_regular(i32::from(delta.new_file().mode()))
                || self.excluded(path);
            changes.push(if gone {
                FileChange::Deleted(path.to_owned())
            } else {
                FileChange::Modified(path.to_owned())
            });
        }
        if let (true, Some(old)) = (rules_changed, old) {
            old.walk(git2::TreeWalkMode::PreOrder, |prefix, entry| {
                let path = Path::new(prefix).join(entry.name().unwrap_or_default());
                if entry.kind() == Some(ObjectType::Blob)
                    && supported(&path)
                    && !changes.iter().any(|c| c.path() == path)
                {
                    changes.push(FileChange::Deleted(path));
                }
                git2::TreeWalkResult::Ok
            })
            .map_err(git)?;
        }
        Ok(changes)
    }
}
impl SourceProvider for TreeSource {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let repo = self.repo()?;
        let tree = repo.find_tree(self.tree).map_err(git)?;
        let entry = tree.get_path(path).map_err(git)?;
        if !is_regular(entry.filemode()) {
            return Ok(None);
        }
        let blob = repo.find_blob(entry.id()).map_err(git)?;
        Ok(Some(blob.content().to_vec()))
    }
}
#[cfg(test)]
mod tests {
    use crate::IncrementalIndexer;
    use std::process::Command;
    #[test]
    fn tree_source_lists_same_paths_as_disk() {
        compare_disk_and_tree(false);
    }
    #[test]
    fn tree_index_skips_binary_and_oversized() {
        compare_disk_and_tree(true);
    }
    fn compare_disk_and_tree(excluded: bool) {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        std::fs::write(root.path().join("kept.rs"), "pub fn kept() {}\n").unwrap();
        if excluded {
            std::fs::write(root.path().join("binary.rs"), b"\0\xff").unwrap();
            std::fs::write(root.path().join("large.rs"), " ".repeat(600_000)).unwrap();
        }
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.test").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .unwrap();
        let indexer = IncrementalIndexer::new(root.path()).unwrap();
        let (disk, sha) = indexer.run(None).unwrap();
        let (tree, actual) = indexer.run_tree(&sha, None).unwrap();
        assert_eq!(sha, actual);
        assert_eq!(disk, tree);
    }
    #[test]
    fn tree_source_honors_ignore_in_tree() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(root.path())
                .output()
                .unwrap();
            assert!(output.status.success());
        };
        git(&["init"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@example.test"]);
        std::fs::create_dir(root.path().join(".gitcortex")).unwrap();
        std::fs::write(root.path().join(".gitcortex/ignore"), "ignored.rs\n").unwrap();
        std::fs::write(root.path().join("ignored.rs"), "pub fn ignored() {}\n").unwrap();
        std::fs::write(root.path().join("kept.rs"), "pub fn kept() {}\n").unwrap();
        std::fs::write(root.path().join("binary.rs"), b"\0\xff").unwrap();
        std::fs::write(root.path().join("large.rs"), " ".repeat(600_000)).unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "-m", "fixture"]);
        std::fs::write(root.path().join(".gitcortex/ignore"), "kept.rs\n").unwrap();
        std::fs::remove_file(root.path().join("kept.rs")).unwrap();
        let (diff, _) = IncrementalIndexer::new(root.path())
            .unwrap()
            .run_tree("HEAD", None)
            .unwrap();
        assert!(diff.added_nodes.iter().any(|node| node.name == "kept"));
        assert!(!diff.added_nodes.iter().any(|node| node.name == "ignored"));
        assert!(!diff
            .added_nodes
            .iter()
            .any(|node| node.file.to_str() == Some("binary.rs")));
        assert!(!diff
            .added_nodes
            .iter()
            .any(|node| node.file.to_str() == Some("large.rs")));
    }
}
