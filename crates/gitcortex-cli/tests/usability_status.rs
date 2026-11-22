mod common;
use common::*;

fn exit(o: &std::process::Output, code: i32, needle: &str) {
    assert_eq!(o.status.code(), Some(code), "{}", message(o));
    assert!(message(o).contains(needle), "{}", message(o));
}

#[test]
fn status_missing_store_is_not_empty_success() {
    let f = Fixture::new();
    exit(&f.run(&["status"]), 3, "missing");
    assert_eq!(std::fs::read_dir(f.data.path()).unwrap().count(), 0);
}

#[test]
fn status_valid_unindexed_branch_exits_3() {
    let f = Fixture::new();
    f.init();
    f.git(&["branch", "other"]);
    exit(&f.run(&["status", "--branch", "other"]), 3, "missing");
}

#[test]
fn status_invalid_ref_exits_2() {
    let f = Fixture::new();
    exit(&f.run(&["status", "--branch", "no-such-ref"]), 2, "invalid");
    assert_eq!(std::fs::read_dir(f.data.path()).unwrap().count(), 0);
}

#[test]
fn status_detached_head_uses_sha() {
    let f = Fixture::new();
    f.git(&["checkout", "--detach"]);
    f.init();
    exit(&f.run(&["status"]), 0, &f.git(&["rev-parse", "HEAD"]));
}

#[test]
fn outside_repo_reports_cd_before_store_access() {
    let f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    for args in [
        vec!["status"],
        vec!["query", "tour"],
        vec!["hook"],
        vec!["viz", "--format", "dot"],
        vec!["export"],
        vec!["blast-radius"],
        vec!["init"],
    ] {
        exit(&f.run_at(outside.path(), &args), 1, "cd <repository>");
    }
    assert_eq!(std::fs::read_dir(f.data.path()).unwrap().count(), 0);
}

#[test]
fn clean_outside_repo_touches_no_store() {
    let f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = f.data.path().join("sentinel");
    std::fs::write(&sentinel, b"preserve").unwrap();
    exit(&f.run_at(outside.path(), &["clean"]), 1, "cd <repository>");
    assert_eq!(std::fs::read(sentinel).unwrap(), b"preserve");
    assert_eq!(std::fs::read_dir(f.data.path()).unwrap().count(), 1);
}

#[test]
fn status_unverified_warns_and_exits_0() {
    let f = Fixture::new();
    f.init();
    std::fs::write(f.repo.path().join("sample.rs"), "pub fn changed() {}\n").unwrap();
    exit(&f.run(&["status"]), 0, "unverified");
}

#[test]
fn status_stale_exits_3() {
    let f = Fixture::new();
    f.init();
    // Bypass installed hooks to leave the graph behind the checkout.
    f.git(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "user.name=Test",
        "-c",
        "user.email=t@t",
        "commit",
        "--allow-empty",
        "-m",
        "later",
    ]);
    exit(&f.run(&["status"]), 3, "stale");
}

#[test]
fn status_missing_head_exits_1() {
    let f = Fixture::new();
    f.git(&["checkout", "--orphan", "unborn"]);
    exit(&f.run(&["status"]), 1, "checkout commit");
}

#[test]
fn status_store_error_is_not_zero_counts() {
    let f = Fixture::new();
    f.init();
    fn corrupt(root: &std::path::Path) {
        for e in std::fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                corrupt(&p);
            } else if p.file_name().unwrap() == "graph.kuzu" {
                std::fs::write(p, b"invalid database").unwrap();
            }
        }
    }
    corrupt(f.data.path());
    let o = f.run(&["status"]);
    exit(&o, 1, "store");
    assert!(!message(&o).contains("nodes: 0"));
}

#[test]
fn ready_empty_repository_has_valid_zero_counts() {
    let f = Fixture::new();
    f.git(&["rm", "sample.rs"]);
    f.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=t@t",
        "commit",
        "-m",
        "empty",
    ]);
    f.init();
    exit(&f.run(&["status"]), 0, "ready");
}

#[test]
fn status_probe_failure_exits_1() {
    let f = Fixture::new();
    f.init();
    std::fs::write(f.repo.path().join(".git/index"), b"broken index").unwrap();
    exit(&f.run(&["status"]), 1, "probe");
}
