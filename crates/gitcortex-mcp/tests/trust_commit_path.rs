mod support;
use gitcortex_store::kuzu::KuzuGraphStore;
use support::*;

fn scenario(mutate: impl FnOnce(&TestRepo)) {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn target() -> u32 { 1 }\n");
    repo.write("caller.rs", "pub fn caller() { crate::a::target(); }\n");
    repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    mutate(&repo);
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
}
macro_rules! case {
    ($name:ident, $action:expr) => {
        #[test]
        fn $name() {
            scenario(|r| {
                ($action)(r);
                r.commit(stringify!($name));
            });
        }
    };
}
case!(after_create_file, |r: &TestRepo| r
    .write("new.rs", "pub fn new() {}"));
case!(after_modify_body, |r: &TestRepo| r
    .write("a.rs", "pub fn target() -> u32 { 2 }\n"));
case!(after_modify_signature, |r: &TestRepo| r
    .write("a.rs", "pub fn target(x: u32) -> u32 { x }\n"));
case!(after_shift_lines, |r: &TestRepo| r
    .write("a.rs", "\npub fn target() -> u32 { 1 }\n"));
case!(after_delete_file, |r: &TestRepo| r.remove("a.rs"));
case!(after_rename_file, |r: &TestRepo| r
    .rename("a.rs", "renamed.rs"));
case!(after_rename_symbol, |r: &TestRepo| r
    .write("a.rs", "pub fn renamed() -> u32 { 1 }\n"));
case!(after_cross_file_caller_removed, |r: &TestRepo| r
    .remove("caller.rs"));
case!(after_duplicate_name_added, |r: &TestRepo| r.write(
    "a.rs",
    "pub fn target() -> u32 { 1 }\npub fn target() -> u32 { 2 }\n"
));
#[test]
fn after_commit_amend() {
    scenario(|r| {
        r.write("a.rs", "pub fn target() -> u32 { 3 }");
        r.git(&["add", "-A"]);
        r.git(&["commit", "--amend", "-m", "amended"]);
    });
}
#[test]
fn after_reset_hard_to_parent() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn parent() {}");
    repo.commit("parent");
    repo.write("a.rs", "pub fn child() {}");
    repo.commit("child");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    repo.git(&["reset", "--hard", "HEAD^"]);
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
}
#[test]
fn after_fast_forward_merge() {
    scenario(|r| {
        r.git(&["checkout", "-b", "feat/x"]);
        r.write("a.rs", "pub fn merged() {}");
        r.commit("feature");
        r.git(&["checkout", "main"]);
        r.git(&["merge", "--ff-only", "feat/x"]);
    });
}
#[test]
fn after_rebase_onto_main() {
    let repo = TestRepo::new();
    repo.write("a.rs", "pub fn base() {}");
    repo.commit("base");
    repo.git(&["checkout", "-b", "feat/x"]);
    repo.write("a.rs", "pub fn feature() {}");
    repo.commit("feature");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "feat/x");
    repo.git(&["checkout", "main"]);
    repo.write("b.rs", "pub fn main_only() {}");
    repo.commit("main advances");
    repo.git(&["checkout", "feat/x"]);
    repo.git(&["rebase", "main"]);
    index_hook_path(&repo, &mut store, "feat/x");
    assert_matches_rebuild(&repo, &store, "feat/x");
}
