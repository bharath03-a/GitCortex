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
