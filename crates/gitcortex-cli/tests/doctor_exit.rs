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
