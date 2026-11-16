use std::process::Command;

const GCX: &str = env!("CARGO_BIN_EXE_gcx");

#[test]
fn doctor_exits_nonzero_when_setup_is_broken() {
    let outside_repo = tempfile::tempdir().expect("tempdir");
    let output = Command::new(GCX)
        .arg("doctor")
        .current_dir(outside_repo.path())
        .output()
        .expect("run gcx doctor");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not inside a git repository"));
    assert!(stderr.contains("issues found"));
}

#[test]
fn missing_graph_cannot_report_an_empty_successful_tour() {
    let root = tempfile::tempdir().expect("repo");
    let data = tempfile::tempdir().expect("data");
    assert!(Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(root.path())
        .output()
        .expect("git")
        .status
        .success());
    assert!(Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@t",
            "commit",
            "--allow-empty",
            "-m",
            "start"
        ])
        .current_dir(root.path())
        .output()
        .expect("git")
        .status
        .success());
    let output = Command::new(GCX)
        .args(["query", "tour"])
        .env("GCX_STORE_PATH", data.path())
        .current_dir(root.path())
        .output()
        .expect("query");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains("index") && message.contains("missing"),
        "{message}"
    );
}

#[test]
fn doctor_reports_unreachable_server() {
    let root = tempfile::tempdir().expect("repo");
    let data = tempfile::tempdir().expect("data");
    assert!(Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root.path())
        .status()
        .expect("git")
        .success());
    // A daemon that holds the graph lock but no longer answers its socket.
    std::env::set_var("GCX_STORE_PATH", data.path());
    let repo = root.path().canonicalize().expect("canonical repo");
    let mut lock = gitcortex_store::branch::RepositoryLock::try_acquire(&repo)
        .expect("lock")
        .expect("lock is free");
    lock.set_owner("pid 999999 (repository daemon)")
        .expect("owner");

    let output = Command::new(GCX)
        .arg("doctor")
        .env("GCX_STORE_PATH", data.path())
        .env("GCX_DISABLE_SEMANTIC", "1")
        .current_dir(root.path())
        .output()
        .expect("doctor");
    drop(lock);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("MCP server unreachable"), "{stderr}");
    assert!(stderr.contains("kill 999999"), "{stderr}");
    assert!(stderr.contains("/mcp"), "{stderr}");
}
