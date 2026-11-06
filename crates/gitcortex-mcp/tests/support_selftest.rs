mod support;
use gitcortex_core::{graph::GraphDiff, schema::EdgeKind, store::GraphStore};
use gitcortex_store::{branch::repo_id, kuzu::KuzuGraphStore};
use support::*;

fn fixture() -> (TestRepo, KuzuGraphStore) {
    let repo = TestRepo::new();
    repo.write(
        "a.rs",
        "pub fn target() {}\npub fn caller() { target(); }\n",
    );
    repo.write("b.rs", "pub fn target() {}\n");
    repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    (repo, store)
}
#[test]
fn store_path_is_isolated() {
    assert!(std::env::var("GCX_STORE_PATH")
        .unwrap()
        .ends_with("target/gcx-test-store"));
}
#[test]
fn rebuild_copy_has_own_repo_id_and_lock() {
    let (repo, _store) = fixture();
    repo.write("dirty.rs", "pub fn dirty() {}");
    let copy = repo.snapshot_copy();
    assert_ne!(repo_id(repo.root()), repo_id(copy.root()));
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        copy.git(&["rev-parse", "HEAD"])
    );
    assert!(copy.root().join("dirty.rs").exists());
    let _second = KuzuGraphStore::open(copy.root()).unwrap();
}
#[test]
fn fingerprint_equal_after_full_reindex() {
    let (repo, store) = fixture();
    assert_matches_rebuild(&repo, &store, "main");
}
macro_rules! stale {
    ($name:ident, $text:expr) => {
        #[test]
        #[should_panic(expected = "rebuild mismatch")]
        fn $name() {
            let (repo, store) = fixture();
            repo.write("a.rs", $text);
            assert_matches_rebuild(&repo, &store, "main");
        }
    };
}
stale!(
    fingerprint_detects_added_symbol,
    "pub fn extra() {}\npub fn target() {}\npub fn caller() { target(); }"
);
stale!(
    fingerprint_detects_stale_body,
    "pub fn target() { let x = 2; }\npub fn caller() { target(); }"
);
stale!(
    fingerprint_detects_stale_span,
    "\npub fn target() {}\npub fn caller() { target(); }"
);
#[test]
#[should_panic(expected = "rebuild mismatch")]
fn fingerprint_detects_misresolved_edge() {
    let (repo, mut store) = fixture();
    let nodes = store.list_all_nodes("main").unwrap();
    let caller = nodes.iter().find(|n| n.name == "caller").unwrap();
    let wrong = nodes
        .iter()
        .find(|n| n.name == "target" && n.file.to_str() == Some("b.rs"))
        .unwrap();
    let mut edge = store
        .list_all_edges("main")
        .unwrap()
        .into_iter()
        .find(|e| e.kind == EdgeKind::Calls && e.src == caller.id && e.dst != wrong.id)
        .unwrap();
    let mut diff = GraphDiff::default();
    diff.removed_edges
        .push((edge.src.clone(), edge.dst.clone(), edge.kind.clone()));
    edge.dst = wrong.id.clone();
    diff.added_edges.push(edge);
    store.apply_diff("main", &diff).unwrap();
    assert_matches_rebuild(&repo, &store, "main");
}
#[test]
#[should_panic(expected = "rebuild mismatch")]
fn fingerprint_counts_duplicate_qualified_names() {
    let repo = TestRepo::new();
    repo.write(
        "a.rs",
        "pub fn same() {}
pub fn same() {}
",
    );
    repo.commit("duplicates");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    let nodes = store.lookup_symbol("main", "same", false).unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].qualified_name, nodes[1].qualified_name);
    let mut diff = GraphDiff::default();
    diff.removed_node_ids.push(nodes[0].id.clone());
    store.apply_diff("main", &diff).unwrap();
    assert_matches_rebuild(&repo, &store, "main");
}
