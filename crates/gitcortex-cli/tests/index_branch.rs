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
