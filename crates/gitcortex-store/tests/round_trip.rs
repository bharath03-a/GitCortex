use std::path::{Path, PathBuf};

use gitcortex_core::{
    graph::{Edge, GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility},
    store::GraphStore,
};
use gitcortex_store::kuzu::KuzuGraphStore;

fn make_node(name: &str, kind: NodeKind, file: &str, line: u32) -> Node {
    Node {
        id: NodeId::new(),
        kind,
        name: name.to_owned(),
        qualified_name: format!("crate::{name}"),
        file: PathBuf::from(file),
        span: Span { start_line: line, end_line: line + 5 },
        metadata: NodeMetadata {
            loc: 6,
            visibility: Visibility::Pub,
            is_async: false,
            is_unsafe: false,
            ..Default::default()
        },
    }
}

/// Opens a KuzuGraphStore backed by a fresh temp directory.
fn tmp_store() -> (KuzuGraphStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = KuzuGraphStore::open(dir.path()).expect("open store");
    (store, dir)
}

#[test]
fn insert_and_lookup_node() {
    let (mut store, _dir) = tmp_store();

    let node = make_node("greet", NodeKind::Function, "src/lib.rs", 1);
    let node_id = node.id.clone();

    let diff = GraphDiff { added_nodes: vec![node], ..Default::default() };
    store.apply_diff("main", &diff).expect("apply_diff");

    let results = store.lookup_symbol("main", "greet").expect("lookup_symbol");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, node_id);
    assert_eq!(results[0].name, "greet");
    assert_eq!(results[0].kind, NodeKind::Function);
}

#[test]
fn list_definitions_ordered_by_line() {
    let (mut store, _dir) = tmp_store();

    let nodes = vec![
        make_node("baz", NodeKind::Function, "src/lib.rs", 20),
        make_node("foo", NodeKind::Function, "src/lib.rs", 1),
        make_node("bar", NodeKind::Struct, "src/lib.rs", 10),
    ];
    let diff = GraphDiff { added_nodes: nodes, ..Default::default() };
    store.apply_diff("main", &diff).expect("apply_diff");

    let defs = store
        .list_definitions("main", Path::new("src/lib.rs"))
        .expect("list_definitions");
    assert_eq!(defs.len(), 3);
    assert_eq!(defs[0].name, "foo");
    assert_eq!(defs[1].name, "bar");
    assert_eq!(defs[2].name, "baz");
}

#[test]
fn find_callers_via_calls_edge() {
    let (mut store, _dir) = tmp_store();

    let caller = make_node("announce", NodeKind::Function, "src/lib.rs", 10);
    let callee = make_node("greet", NodeKind::Method, "src/lib.rs", 1);
    let edge = Edge { src: caller.id.clone(), dst: callee.id.clone(), kind: EdgeKind::Calls };

    let diff = GraphDiff {
        added_nodes: vec![caller.clone(), callee],
        added_edges: vec![edge],
        ..Default::default()
    };
    store.apply_diff("main", &diff).expect("apply_diff");

    let callers = store.find_callers("main", "greet").expect("find_callers");
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].name, "announce");
}

#[test]
fn delete_file_removes_nodes() {
    let (mut store, _dir) = tmp_store();

    let node = make_node("old_fn", NodeKind::Function, "src/old.rs", 1);
    let add_diff = GraphDiff { added_nodes: vec![node], ..Default::default() };
    store.apply_diff("main", &add_diff).expect("apply_diff");

    assert_eq!(store.lookup_symbol("main", "old_fn").unwrap().len(), 1);

    let del_diff = GraphDiff {
        removed_files: vec![PathBuf::from("src/old.rs")],
        ..Default::default()
    };
    store.apply_diff("main", &del_diff).expect("apply_diff remove");

    assert_eq!(store.lookup_symbol("main", "old_fn").unwrap().len(), 0);
}

#[test]
fn last_indexed_sha_round_trip() {
    let (mut store, _dir) = tmp_store();

    assert!(store.last_indexed_sha("main").unwrap().is_none());

    store.set_last_indexed_sha("main", "abc123").expect("set sha");
    assert_eq!(store.last_indexed_sha("main").unwrap().as_deref(), Some("abc123"));
}

#[test]
fn branch_diff_detects_added_and_removed_nodes() {
    let (mut store, _dir) = tmp_store();

    let node_a = make_node("shared", NodeKind::Function, "src/lib.rs", 1);
    let node_b = make_node("only_main", NodeKind::Function, "src/lib.rs", 10);
    let node_c = make_node("only_feat", NodeKind::Function, "src/lib.rs", 20);

    // main has shared + only_main
    let main_diff = GraphDiff {
        added_nodes: vec![node_a.clone(), node_b.clone()],
        ..Default::default()
    };
    store.apply_diff("main", &main_diff).expect("apply main");

    // feat has shared + only_feat
    let feat_diff =
        GraphDiff { added_nodes: vec![node_a, node_c.clone()], ..Default::default() };
    store.apply_diff("feat/new", &feat_diff).expect("apply feat");

    let diff = store.branch_diff("main", "feat/new").expect("branch_diff");

    // only_feat is in feat but not main → added
    assert!(diff.added_nodes.iter().any(|n| n.name == "only_feat"));
    // only_main is in main but not feat → removed
    assert!(!diff.removed_node_ids.is_empty());
}
