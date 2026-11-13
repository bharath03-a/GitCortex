use crate::{differ::FileChange, IncrementalIndexer};
use git2::{FileMode, Repository};
use gitcortex_core::error::{GitCortexError, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

const IGNORE_FILE: &str = ".gitcortex/ignore";

fn git(error: git2::Error) -> GitCortexError {
    GitCortexError::Git(error.to_string())
}

fn is_regular(mode: i32) -> bool {
    mode == i32::from(FileMode::Blob) || mode == i32::from(FileMode::BlobExecutable)
}

/// Parses ignore bytes as `GitignoreBuilder::add` does for the checkout: strips a
/// BOM, skips invalid globs, and stops at the first non-UTF-8 line.
fn add_ignore_lines(builder: &mut GitignoreBuilder, content: &[u8]) {
    for (index, line) in content.split(|byte| *byte == b'\n').enumerate() {
        let Ok(line) = std::str::from_utf8(line) else {
            break;
        };
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line = if index == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        let _ = builder.add_line(None, line);
    }
}

fn tree_ignore(repo: &Repository, tree: &git2::Tree<'_>, root: &Path) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    match tree.get_path(Path::new(IGNORE_FILE)) {
        Ok(entry) if is_regular(entry.filemode()) => {
            let blob = repo.find_blob(entry.id()).map_err(git)?;
            add_ignore_lines(&mut builder, blob.content());
        }
        Ok(_) => {}
        Err(error) if error.code() == git2::ErrorCode::NotFound => {}
        Err(error) => return Err(git(error)),
    }
    Ok(builder.build().unwrap_or_else(|_| Gitignore::empty()))
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
        // A symlink fails the whole checkout index: following one could read
        // outside the repository. Trees differ by design, see `TreeSource::read`.
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
            let ignore = tree_ignore(&repo, &tree, root)?;
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
            // `read` skips symlinks and gitlinks; diff splits a typechange into
            // delete + add, so the replaced file's symbols are still removed.
            let gone = delta.status() == git2::Delta::Deleted || self.excluded(path);
            changes.push(if gone {
                FileChange::Deleted(path.to_owned())
            } else {
                FileChange::Modified(path.to_owned())
            });
        }
        if let (true, Some(old)) = (rules_changed, old) {
            // Diff rather than `Tree::walk`, whose callback rejects non-UTF-8 names.
            let reported: HashSet<PathBuf> = changes.iter().map(|c| c.path().to_owned()).collect();
            let previous = repo
                .diff_tree_to_tree(Some(&old), None, None)
                .map_err(git)?;
            for delta in previous.deltas() {
                let Some(path) = delta.old_file().path() else {
                    continue;
                };
                if supported(path) && !reported.contains(path) {
                    changes.push(FileChange::Deleted(path.to_owned()));
                }
            }
        }
        Ok(changes)
    }
}
impl SourceProvider for TreeSource {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let repo = self.repo()?;
        let tree = repo.find_tree(self.tree).map_err(git)?;
        let entry = tree.get_path(path).map_err(git)?;
        // A symlink's blob is only its target path and nothing is followed, so
        // skipping is safe here, unlike on disk where a symlink fails the index.
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

    #[test]
    fn checkout_incremental_reevaluates_paths_when_rules_change() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(root.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{args:?}");
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        git(&["init"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@example.test"]);
        git(&["config", "commit.gpgsign", "false"]);
        std::fs::create_dir(root.path().join(".gitcortex")).unwrap();
        std::fs::write(root.path().join(".gitcortex/ignore"), "hidden.rs\n").unwrap();
        std::fs::write(root.path().join("hidden.rs"), "pub fn hidden() {}\n").unwrap();
        std::fs::write(root.path().join("kept.rs"), "pub fn kept() {}\n").unwrap();
        std::fs::write(root.path().join("dropped.rs"), "pub fn dropped() {}\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "base"]);
        let base = git(&["rev-parse", "HEAD"]);
        std::fs::write(root.path().join(".gitcortex/ignore"), "dropped.rs\n").unwrap();
        git(&["commit", "-am", "swap rules"]);
        let (diff, _) = IncrementalIndexer::new(root.path())
            .unwrap()
            .run(Some(&base))
            .unwrap();
        assert!(diff
            .removed_files
            .iter()
            .any(|p| p.as_os_str() == "dropped.rs"));
        assert!(!names(&diff).contains(&"dropped"));
        assert!(
            names(&diff).contains(&"hidden"),
            "unignored file not indexed"
        );

        // A commit touching only an ignored file still drops its nodes.
        let base = git(&["rev-parse", "HEAD"]);
        std::fs::write(root.path().join("dropped.rs"), "pub fn dropped2() {}\n").unwrap();
        git(&["commit", "-am", "edit ignored"]);
        let (diff, _) = IncrementalIndexer::new(root.path())
            .unwrap()
            .run(Some(&base))
            .unwrap();
        assert!(diff
            .removed_files
            .iter()
            .any(|p| p.as_os_str() == "dropped.rs"));
        assert!(!names(&diff).contains(&"dropped2"));
        assert!(diff
            .source_hashes
            .iter()
            .all(|(p, _)| p.as_os_str() != "dropped.rs"));
    }

    enum Entry<'a> {
        File(&'a str),
        Tree(Vec<(&'a [u8], u32, Entry<'a>)>),
        Commit,
    }

    /// Serializes a tree by hand: `TreeBuilder` refuses legacy modes, which real
    /// histories still contain.
    fn raw_tree(repo: &git2::Repository, entries: &[(&[u8], u32, Entry<'_>)]) -> git2::Oid {
        let mut named: Vec<(Vec<u8>, u32, git2::Oid)> = entries
            .iter()
            .map(|(name, mode, entry)| {
                let (sort_name, id) = match entry {
                    Entry::File(text) => (name.to_vec(), repo.blob(text.as_bytes()).unwrap()),
                    Entry::Commit => (name.to_vec(), git2::Oid::from_bytes(&[7; 20]).unwrap()),
                    Entry::Tree(children) => {
                        let mut sort_name = name.to_vec();
                        sort_name.push(b'/');
                        (sort_name, raw_tree(repo, children))
                    }
                };
                (sort_name, *mode, id)
            })
            .collect();
        named.sort_by(|a, b| a.0.cmp(&b.0));
        let mut bytes = Vec::new();
        for (name, mode, id) in &named {
            let name = name.strip_suffix(b"/").unwrap_or(name);
            bytes.extend_from_slice(format!("{mode:o} ").as_bytes());
            bytes.extend_from_slice(name);
            bytes.push(0);
            bytes.extend_from_slice(id.as_bytes());
        }
        repo.odb()
            .unwrap()
            .write(git2::ObjectType::Tree, &bytes)
            .unwrap()
    }

    fn raw_commit(
        repo: &git2::Repository,
        parent: Option<&str>,
        entries: &[(&[u8], u32, Entry<'_>)],
    ) -> String {
        let tree = repo.find_tree(raw_tree(repo, entries)).unwrap();
        let signature = git2::Signature::now("Test", "test@example.test").unwrap();
        let parents: Vec<git2::Commit<'_>> = parent
            .map(|sha| repo.revparse_single(sha).unwrap().peel_to_commit().unwrap())
            .into_iter()
            .collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        repo.commit(None, &signature, &signature, "fixture", &tree, &parents)
            .unwrap()
            .to_string()
    }

    fn names(diff: &gitcortex_core::graph::GraphDiff) -> Vec<&str> {
        diff.added_nodes.iter().map(|n| n.name.as_str()).collect()
    }

    /// Structural nodes come out of a hash map, so compare without order.
    fn graph_of(diff: &gitcortex_core::graph::GraphDiff) -> (Vec<String>, Vec<String>) {
        let mut nodes: Vec<_> = diff.added_nodes.iter().map(|n| format!("{n:?}")).collect();
        let mut edges: Vec<_> = diff.added_edges.iter().map(|e| format!("{e:?}")).collect();
        nodes.sort();
        edges.sort();
        (nodes, edges)
    }

    const FN_A: &str = "pub fn alpha() {}\n";
    const FN_B: &str = "pub fn beta() {}\n";
    const LINK: u32 = 0o120000;
    const GITLINK: u32 = 0o160000;
    const REGULAR: u32 = 0o100644;

    #[test]
    fn legacy_blob_modes_index_as_regular_files() {
        for mode in [0o100664, 0o100600, 0o100640] {
            let root = tempfile::tempdir().unwrap();
            let repo = git2::Repository::init(root.path()).unwrap();
            let first = raw_commit(&repo, None, &[(b"other.rs", REGULAR, Entry::File(FN_B))]);
            let second = raw_commit(
                &repo,
                Some(&first),
                &[
                    (b"legacy.rs", mode, Entry::File(FN_A)),
                    (b"other.rs", REGULAR, Entry::File(FN_B)),
                ],
            );
            let indexer = IncrementalIndexer::new(root.path()).unwrap();
            let (full, _) = indexer.run_tree(&second, None).unwrap();
            assert!(names(&full).contains(&"alpha"), "{mode:o}: {full:?}");
            let (incremental, _) = indexer.run_tree(&second, Some(&first)).unwrap();
            assert!(names(&incremental).contains(&"alpha"), "{mode:o}");
            assert!(
                incremental
                    .source_hashes
                    .iter()
                    .any(|(p, hash)| p.as_os_str() == "legacy.rs" && hash.is_some()),
                "{mode:o}: legacy file reported deleted"
            );
        }
    }

    #[test]
    fn symlink_and_gitlink_entries_are_never_parsed() {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        let sha = raw_commit(
            &repo,
            None,
            &[
                (b"kept.rs", REGULAR, Entry::File(FN_A)),
                (b"link.rs", LINK, Entry::File("pub fn from_link() {}\n")),
                (b"module.rs", GITLINK, Entry::Commit),
            ],
        );
        let (diff, _) = IncrementalIndexer::new(root.path())
            .unwrap()
            .run_tree(&sha, None)
            .unwrap();
        let found = names(&diff);
        assert!(found.contains(&"alpha"), "{found:?}");
        assert!(!found.contains(&"from_link"), "{found:?}");
        assert!(!diff
            .added_nodes
            .iter()
            .any(|n| n.file.as_os_str() == "link.rs" || n.file.as_os_str() == "module.rs"));
    }

    #[test]
    fn typechange_to_symlink_removes_old_symbols_and_back_restores_them() {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        let regular = raw_commit(&repo, None, &[(b"a.rs", REGULAR, Entry::File(FN_A))]);
        let linked = raw_commit(
            &repo,
            Some(&regular),
            &[(b"a.rs", LINK, Entry::File("target.rs"))],
        );
        let gitlinked = raw_commit(&repo, Some(&linked), &[(b"a.rs", GITLINK, Entry::Commit)]);
        let restored = raw_commit(
            &repo,
            Some(&gitlinked),
            &[(b"a.rs", REGULAR, Entry::File(FN_B))],
        );
        let indexer = IncrementalIndexer::new(root.path()).unwrap();
        for (from, to) in [(&regular, &linked), (&regular, &gitlinked)] {
            let (diff, _) = indexer.run_tree(to, Some(from)).unwrap();
            assert!(diff.removed_files.iter().any(|p| p.as_os_str() == "a.rs"));
            assert!(diff
                .added_nodes
                .iter()
                .all(|n| n.file.as_os_str() != "a.rs"));
        }
        for from in [&linked, &gitlinked] {
            let (diff, _) = indexer.run_tree(&restored, Some(from)).unwrap();
            assert!(names(&diff).contains(&"beta"), "{diff:?}");
        }
    }

    #[test]
    fn non_utf8_paths_index_and_delete_by_bytes() {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        let first = raw_commit(
            &repo,
            None,
            &[
                (b"caf\xff.rs", REGULAR, Entry::File(FN_A)),
                (
                    b"dir\xff",
                    0o040000,
                    Entry::Tree(vec![(b"inner.rs", REGULAR, Entry::File(FN_B))]),
                ),
                (b"kept.rs", REGULAR, Entry::File("pub fn kept() {}\n")),
            ],
        );
        let indexer = IncrementalIndexer::new(root.path()).unwrap();
        let (full, _) = indexer.run_tree(&first, None).unwrap();
        let found = names(&full);
        assert!(
            found.contains(&"alpha") && found.contains(&"beta"),
            "{found:?}"
        );

        // A rules change must still report byte paths that vanished.
        let second = raw_commit(
            &repo,
            Some(&first),
            &[
                (
                    b".gitcortex",
                    0o040000,
                    Entry::Tree(vec![(b"ignore", REGULAR, Entry::File("nothing.rs\n"))]),
                ),
                (b"kept.rs", REGULAR, Entry::File("pub fn kept() {}\n")),
            ],
        );
        let (diff, _) = indexer.run_tree(&second, Some(&first)).unwrap();
        use std::os::unix::ffi::OsStrExt;
        for gone in [&b"caf\xff.rs"[..], &b"dir\xff/inner.rs"[..]] {
            assert!(
                diff.removed_files
                    .iter()
                    .any(|p| p.as_os_str().as_bytes() == gone),
                "{:?} missing from {:?}",
                String::from_utf8_lossy(gone),
                diff.removed_files
            );
        }
    }

    #[test]
    fn rules_change_reports_each_vanished_path_once() {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        let first = raw_commit(
            &repo,
            None,
            &[
                (b"gone.rs", REGULAR, Entry::File(FN_A)),
                (b"kept.rs", REGULAR, Entry::File(FN_B)),
            ],
        );
        let second = raw_commit(
            &repo,
            Some(&first),
            &[
                (
                    b".gitcortex",
                    0o040000,
                    Entry::Tree(vec![(b"ignore", REGULAR, Entry::File("kept.rs\n"))]),
                ),
                (b"kept.rs", REGULAR, Entry::File(FN_B)),
            ],
        );
        let (diff, _) = IncrementalIndexer::new(root.path())
            .unwrap()
            .run_tree(&second, Some(&first))
            .unwrap();
        for path in ["gone.rs", "kept.rs"] {
            let hashes = diff
                .source_hashes
                .iter()
                .filter(|(p, hash)| p.as_os_str() == path && hash.is_none())
                .count();
            assert_eq!(hashes, 1, "{path}: {:?}", diff.source_hashes);
        }
    }

    /// Same `.gitcortex/ignore` bytes on disk and in the tree must scope alike.
    fn assert_ignore_parity(ignore: &[u8], excluded: &[&str], kept: &[&str]) {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        std::fs::create_dir(root.path().join(".gitcortex")).unwrap();
        std::fs::write(root.path().join(".gitcortex/ignore"), ignore).unwrap();
        for name in excluded.iter().chain(kept) {
            let stem = name.trim_end_matches(".rs");
            std::fs::write(root.path().join(name), format!("pub fn {stem}() {{}}\n")).unwrap();
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
        let (tree, _) = indexer.run_tree(&sha, None).unwrap();
        // Removal lists differ by design: the tree path reports excluded files
        // as deleted so a rules change can evict them.
        assert_eq!(graph_of(&disk), graph_of(&tree));
        for name in excluded {
            let stem = name.trim_end_matches(".rs");
            assert!(!names(&tree).contains(&stem), "{stem} must be excluded");
        }
        for name in kept {
            let stem = name.trim_end_matches(".rs");
            assert!(names(&tree).contains(&stem), "{stem} must be kept");
        }
    }

    #[test]
    fn tree_ignore_strips_bom_like_disk() {
        assert_ignore_parity(b"\xef\xbb\xbfskipped.rs\n", &["skipped.rs"], &["kept.rs"]);
    }

    #[test]
    fn tree_ignore_skips_invalid_globs_like_disk() {
        assert_ignore_parity(
            b"a[z-b].rs\nskipped.rs\n",
            &["skipped.rs"],
            &["kept.rs", "other.rs"],
        );
    }

    #[test]
    fn tree_ignore_stops_at_non_utf8_like_disk() {
        assert_ignore_parity(
            b"skipped.rs\n\xff\xfe\nafter.rs\n",
            &["skipped.rs"],
            &["after.rs", "kept.rs"],
        );
    }

    #[test]
    fn unreadable_tree_ignore_is_reported_not_ignored() {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        std::fs::write(root.path().join("a.rs"), FN_A).unwrap();
        std::fs::create_dir(root.path().join(".gitcortex")).unwrap();
        std::fs::write(
            root.path().join(".gitcortex/ignore"),
            "unique-ignore-bytes\n",
        )
        .unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test", "test@example.test").unwrap();
        let sha = repo
            .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .unwrap()
            .to_string();
        let ignore_tree = tree
            .get_path(std::path::Path::new(".gitcortex"))
            .unwrap()
            .id()
            .to_string();
        std::fs::remove_file(
            repo.path()
                .join("objects")
                .join(&ignore_tree[..2])
                .join(&ignore_tree[2..]),
        )
        .unwrap();
        let error = IncrementalIndexer::new(root.path())
            .unwrap()
            .run_tree(&sha, None)
            .expect_err("a corrupt ignore must not read as no rules");
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn source_symlink_is_skipped_in_tree_but_refused_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(root.path()).unwrap();
        std::fs::write(root.path().join("real.rs"), FN_A).unwrap();
        std::os::unix::fs::symlink("real.rs", root.path().join("link.rs")).unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.test").unwrap();
        let sha = repo
            .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .unwrap()
            .to_string();
        let indexer = IncrementalIndexer::new(root.path()).unwrap();
        let refused = indexer.run(None).expect_err("checkout index refuses");
        assert!(refused.to_string().contains("symlink"), "{refused}");
        let refused = indexer
            .index_files_from_disk(&[root.path().join("link.rs")])
            .expect_err("live update refuses");
        assert!(refused.to_string().contains("symlink"), "{refused}");
        let (tree, _) = indexer.run_tree(&sha, None).unwrap();
        assert!(names(&tree).contains(&"alpha"));
        assert!(tree
            .added_nodes
            .iter()
            .all(|n| n.file.as_os_str() != "link.rs"));
    }
}
