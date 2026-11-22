mod common;
use common::*;

#[test]
fn documented_blast_head_on_clean_fixture() {
    let f = Fixture::new();
    f.init();
    assert!(f.git(&["status", "--porcelain"]).is_empty());
    for head in ["HEAD", "refs/heads/main", "alias"] {
        if head == "alias" {
            f.git(&["symbolic-ref", "refs/heads/alias", "refs/heads/main"]);
        }
        let o = f.run(&[
            "blast-radius",
            "--base",
            "main",
            "--head",
            head,
            "--format",
            "text",
        ]);
        assert!(o.status.success(), "{}", message(&o));
    }
}

#[test]
fn distinct_same_commit_branch_keeps_its_index() {
    let f = Fixture::new();
    f.init();
    f.git(&["branch", "other"]);
    let o = f.run(&["blast-radius", "--base", "main", "--head", "other"]);
    assert_eq!(o.status.code(), Some(3), "{}", message(&o));
    assert!(message(&o).contains("missing on branch 'other'"));
}

#[test]
fn detached_full_sha_normalization() {
    let f = Fixture::new();
    f.git(&["checkout", "--detach"]);
    f.init();
    let sha = f.git(&["rev-parse", "HEAD"]);
    let o = f.run(&["blast-radius", "--base", &sha, "--head", "HEAD"]);
    assert!(o.status.success(), "{}", message(&o));
}

#[test]
fn unindexed_sha_is_never_certified() {
    let f = Fixture::new();
    f.init();
    let sha = f.git(&["rev-parse", "HEAD"]);
    let o = f.run(&["blast-radius", "--base", "main", "--head", &sha]);
    assert_eq!(o.status.code(), Some(3), "{}", message(&o));
    assert!(
        message(&o).contains(&format!("missing on branch '{sha}'")),
        "{}",
        message(&o)
    );
}

#[test]
fn invalid_blast_ref_exits_2_without_opening_store() {
    let f = Fixture::new();
    let o = f.run(&["blast-radius", "--base", "main", "--head", "no-such-ref"]);
    assert_eq!(o.status.code(), Some(2), "{}", message(&o));
    assert!(message(&o).contains("invalid"), "{}", message(&o));
    assert_eq!(std::fs::read_dir(f.data.path()).unwrap().count(), 0);
}
