use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn index_branch_preserves_checkout_and_ignores_uncommitted_bytes() {
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "core.hooksPath", "/dev/null"]);
    git(repo.path(), &["config", "user.name", "Test"]);
    git(repo.path(), &["config", "user.email", "test@example.test"]);
    std::fs::write(repo.path().join("lib.rs"), "pub fn committed() {}\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(
        repo.path(),
        &["-c", "commit.gpgsign=false", "commit", "-m", "fixture"],
    );
    git(repo.path(), &["branch", "feat/ü-x"]);
    let head = git(repo.path(), &["rev-parse", "HEAD"]);
    std::fs::write(repo.path().join("lib.rs"), "pub fn dirty() {}\n").unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_gcx"))
            .current_dir(repo.path())
            .env("GCX_STORE_PATH", data.path())
            .args(args)
            .output()
            .unwrap()
    };
    let result = invoke(&["index", "--branch", "feat/ü-x"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("nodes"));
    assert_eq!(
        git(repo.path(), &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("lib.rs")).unwrap(),
        "pub fn dirty() {}\n"
    );
    let result = invoke(&[
        "query",
        "lookup-symbol",
        "committed",
        "--branch",
        "feat/ü-x",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("committed"));
    assert!(!invoke(&["index", "--branch", "unknown"]).status.success());
    assert!(!invoke(&["index"]).status.success());
}

struct Fixture {
    repo: tempfile::TempDir,
    data: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            repo: tempfile::tempdir().unwrap(),
            data: tempfile::tempdir().unwrap(),
        };
        let repo = fixture.repo.path();
        git(repo, &["init", "-b", "main"]);
        git(repo, &["config", "core.hooksPath", "/dev/null"]);
        git(repo, &["config", "user.name", "Test"]);
        git(repo, &["config", "user.email", "test@example.test"]);
        git(repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("lib.rs"), "pub fn one() {}\npub fn two() {}\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", "one"]);
        std::fs::write(repo.join("lib.rs"), "pub fn one() {}\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", "two"]);
        git(repo, &["switch", "-c", "feature"]);
        git(repo, &["switch", "main"]);
        fixture
    }
    fn gcx(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_gcx"))
            .current_dir(self.repo.path())
            .env("GCX_STORE_PATH", self.data.path())
            .args(args)
            .output()
            .unwrap()
    }
    fn store_files(&self) -> Vec<String> {
        fn walk(dir: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                if entry.path().is_dir() {
                    walk(&entry.path(), out);
                } else {
                    out.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        let mut out = Vec::new();
        walk(self.data.path(), &mut out);
        out
    }
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn revision_syntax_and_unknown_branches_are_refused_clearly() {
    let fixture = Fixture::new();
    for input in ["main~1", "main^", "main@{0}", "main^{/one}", "no-such"] {
        let output = fixture.gcx(&["index", "--branch", input]);
        assert_eq!(output.status.code(), Some(1), "{input}");
        let text = stderr(&output);
        assert!(text.contains("No local branch named"), "{input}: {text}");
        assert!(text.contains(input), "{input}: {text}");
        assert!(!text.contains("revspec"), "{input}: {text}");
        let files = fixture.store_files();
        assert!(
            !files
                .iter()
                .any(|f| f.ends_with(".sha") || f.ends_with(".dirty")),
            "{input}: {files:?}"
        );
    }
}

#[test]
fn remote_only_branch_is_explained() {
    let fixture = Fixture::new();
    let head = git(fixture.repo.path(), &["rev-parse", "HEAD"]);
    git(
        fixture.repo.path(),
        &["update-ref", "refs/remotes/origin/foo", &head],
    );
    let output = fixture.gcx(&["index", "--branch", "origin/foo"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("remote-only"), "{text}");
    assert!(!text.contains("revspec"), "{text}");
}

#[test]
fn rerun_reports_unchanged_with_graph_totals() {
    let fixture = Fixture::new();
    let first = fixture.gcx(&["index", "--branch", "feature"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let first = stdout(&first);
    assert!(first.contains("Indexed feature"), "{first}");
    assert!(!first.contains(" 0 nodes"), "{first}");
    let rerun = fixture.gcx(&["index", "--branch", "feature"]);
    assert!(rerun.status.success(), "{}", stderr(&rerun));
    let rerun = stdout(&rerun);
    assert!(rerun.contains("unchanged"), "{rerun}");
    assert!(!rerun.contains(" 0 nodes"), "{rerun}");
    assert!(!rerun.contains(" 0 edges"), "{rerun}");
}

#[test]
fn held_lock_failure_says_mcp_can_index_the_branch() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    git(&root, &["init", "-b", "main"]);
    git(&root, &["config", "core.hooksPath", "/dev/null"]);
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["config", "user.email", "test@example.test"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    std::fs::write(root.join("lib.rs"), "pub fn one() {}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "one"]);
    git(&root, &["branch", "feature"]);
    // Inherits this process's data root so the child contends for this lock.
    let store_dir =
        gitcortex_store::branch::data_dir(&gitcortex_store::branch::storage_repo_id(&root));
    let held = gitcortex_store::branch::RepositoryLock::try_acquire(&root)
        .unwrap()
        .expect("lock is free");
    let output = Command::new(env!("CARGO_BIN_EXE_gcx"))
        .current_dir(&root)
        .args(["index", "--branch", "feature"])
        .output()
        .unwrap();
    drop(held);
    let _ = std::fs::remove_dir_all(store_dir);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("repository graph is active"), "{text}");
    assert!(text.contains("MCP"), "{text}");
    assert!(text.contains("branch"), "{text}");
    assert!(text.contains("feature"), "{text}");
}
