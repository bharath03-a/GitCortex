//! `index_hook_path` in the trust harness re-implements `gcx hook`; this test
//! keeps the copy honest by running both on identical repositories.
#[path = "../../gitcortex-mcp/tests/support/mod.rs"]
mod support;

use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;
use std::process::Command;
use support::*;

const GCX: &str = env!("CARGO_BIN_EXE_gcx");

fn run_gcx_hook(repo: &TestRepo) {
    let out = Command::new(GCX)
        .arg("hook")
        .current_dir(repo.root())
        .env(
            "GCX_STORE_PATH",
            std::env::var_os("GCX_STORE_PATH").expect("GCX_STORE_PATH"),
        )
        .output()
        .expect("spawn gcx hook");
    assert!(
        out.status.success(),
        "gcx hook failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_same_state(hooked: &TestRepo, harnessed: &TestRepo, harness_store: &KuzuGraphStore) {
    // The hook child has exited, so its process-exclusive store lock is free.
    let hook_store = KuzuGraphStore::open(hooked.root()).unwrap();
    let delta = fingerprint(&hook_store, "main").diff(&fingerprint(harness_store, "main"));
    assert!(delta.is_none(), "hook vs harness: {}", delta.unwrap());
    assert_eq!(
        hook_store.last_indexed_sha("main").unwrap(),
        Some(hooked.git(&["rev-parse", "HEAD"]))
    );
    assert_eq!(
        harness_store.last_indexed_sha("main").unwrap(),
        Some(harnessed.git(&["rev-parse", "HEAD"]))
    );
    assert_eq!(
        hook_store.working_tree_dirty("main").unwrap(),
        harness_store.working_tree_dirty("main").unwrap(),
    );
    assert!(!hook_store.working_tree_dirty("main").unwrap());
}

#[test]
fn gcx_hook_equals_harness_hook_path() {
    let hooked = TestRepo::new();
    hooked.write("a.rs", "pub fn target() -> u32 { 1 }\n");
    hooked.write("caller.rs", "pub fn caller() { crate::a::target(); }\n");
    hooked.commit("initial");
    let harnessed = hooked.snapshot_copy();
    let mut harness_store = KuzuGraphStore::open(harnessed.root()).unwrap();

    type Step = (&'static str, &'static dyn Fn(&TestRepo));
    let steps: [Step; 4] = [
        ("initial", &|_| {}),
        ("modify and add", &|r| {
            r.write("a.rs", "pub fn target() -> u32 { 2 }\n");
            r.write("new.rs", "pub fn added() { crate::a::target(); }\n");
        }),
        ("delete", &|r| r.remove("caller.rs")),
        ("empty commit", &|_| {}),
    ];
    for (name, mutate) in steps {
        if name != "initial" {
            for repo in [&hooked, &harnessed] {
                mutate(repo);
                repo.commit(name);
            }
        }
        run_gcx_hook(&hooked);
        index_hook_path(&harnessed, &mut harness_store, "main");
        assert_same_state(&hooked, &harnessed, &harness_store);
    }
}
