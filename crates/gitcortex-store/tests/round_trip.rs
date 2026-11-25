#![cfg(feature = "kuzu-backend")]

use std::path::{Path, PathBuf};

use gitcortex_core::{
    graph::{Edge, GraphDiff, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility, SCHEMA_VERSION},
    store::GraphStore,
};
use gitcortex_store::{branch, kuzu::KuzuGraphStore};

fn make_node(name: &str, kind: NodeKind, file: &str, line: u32) -> Node {
    Node {
        id: NodeId::new(),
        kind,
        name: name.to_owned(),
        qualified_name: format!("crate::{name}"),
        file: PathBuf::from(file),
        span: Span {
            start_line: line,
            end_line: line + 5,
        },
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
fn repository_lock_rejects_a_second_store_owner() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let first = KuzuGraphStore::open(repo.path()).expect("first store owner");
    let error = KuzuGraphStore::open(repo.path())
        .err()
        .expect("second owner must be rejected");
    assert!(error.to_string().contains("repository graph is busy"));
    drop(first);
    let reopened = KuzuGraphStore::open(repo.path()).expect("lock released on drop");
    drop(reopened);
}

#[test]
fn insert_and_lookup_node() {
    let (mut store, _dir) = tmp_store();

    let node = make_node("greet", NodeKind::Function, "src/lib.rs", 1);
    let node_id = node.id.clone();

    let diff = GraphDiff {
        added_nodes: vec![node],
        ..Default::default()
    };
    store.apply_diff("main", &diff).expect("apply_diff");

    let results = store
        .lookup_symbol("main", "greet", false)
        .expect("lookup_symbol");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, node_id);
    assert_eq!(results[0].name, "greet");
    assert_eq!(results[0].kind, NodeKind::Function);
}

#[test]
fn search_nodes_honors_limit_with_stable_ordering() {
    let (mut store, _dir) = tmp_store();
    let nodes = vec![
        make_node("match_z", NodeKind::Function, "z.rs", 1),
        make_node("match_a", NodeKind::Function, "a.rs", 1),
        make_node("match_m", NodeKind::Function, "m.rs", 1),
    ];
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: nodes,
                ..Default::default()
            },
        )
        .expect("insert nodes");

    let first = store
        .search_nodes("main", "match_", 2)
        .expect("first search");
    let second = store
        .search_nodes("main", "match_", 2)
        .expect("second search");
    assert_eq!(first.len(), 2);
    assert_eq!(
        first.iter().map(|node| &node.id).collect::<Vec<_>>(),
        second.iter().map(|node| &node.id).collect::<Vec<_>>()
    );
    assert_eq!(
        first
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        vec!["match_a", "match_m"]
    );

    let structs = vec![
        make_node("match_struct_z", NodeKind::Struct, "z.rs", 1),
        make_node("match_struct_a", NodeKind::Struct, "a.rs", 1),
    ];
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: structs,
                ..Default::default()
            },
        )
        .expect("insert structs");
    let filtered = store
        .search_nodes_filtered("main", "match_", Some("struct"), None, 1)
        .expect("filtered search");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "match_struct_a");
}

#[test]
fn list_definitions_ordered_by_line() {
    let (mut store, _dir) = tmp_store();

    let nodes = vec![
        make_node("baz", NodeKind::Function, "src/lib.rs", 20),
        make_node("foo", NodeKind::Function, "src/lib.rs", 1),
        make_node("bar", NodeKind::Struct, "src/lib.rs", 10),
    ];
    let diff = GraphDiff {
        added_nodes: nodes,
        ..Default::default()
    };
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
    let edge = Edge::call(caller.id.clone(), callee.id.clone(), 12);

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
fn duplicate_replacement_node_id_does_not_drop_preserved_incoming_edge() {
    let (mut store, _dir) = tmp_store();
    let caller = make_node("caller", NodeKind::Function, "src/caller.rs", 1);
    let callee = make_node("callee", NodeKind::Function, "src/callee.rs", 1);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![caller.clone(), callee.clone()],
                added_edges: vec![Edge::call(caller.id.clone(), callee.id.clone(), 3)],
                ..Default::default()
            },
        )
        .expect("apply initial graph");

    store
        .apply_diff(
            "main",
            &GraphDiff {
                removed_files: vec![PathBuf::from("src/callee.rs")],
                added_nodes: vec![callee.clone(), callee],
                ..Default::default()
            },
        )
        .expect("replace callee with duplicate node entries");

    let callers = store.find_callers("main", "callee").expect("find callers");
    assert_eq!(
        callers.len(),
        1,
        "duplicate copies of one replacement node ID are not ambiguity"
    );
    assert_eq!(callers[0].id, caller.id);
}

#[test]
fn replacement_edge_is_not_duplicated_when_already_present_in_diff() {
    let (mut store, _dir) = tmp_store();
    let caller = make_node("caller", NodeKind::Function, "src/caller.rs", 1);
    let old_callee = make_node("callee", NodeKind::Function, "src/callee.rs", 1);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![caller.clone(), old_callee.clone()],
                added_edges: vec![Edge::call(caller.id.clone(), old_callee.id.clone(), 3)],
                ..Default::default()
            },
        )
        .expect("apply initial graph");

    let replacement = make_node("callee", NodeKind::Function, "src/callee.rs", 1);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                removed_files: vec![PathBuf::from("src/callee.rs")],
                added_nodes: vec![replacement.clone()],
                added_edges: vec![Edge::call(caller.id.clone(), replacement.id.clone(), 3)],
                ..Default::default()
            },
        )
        .expect("replace callee with explicit edge");

    let matching_edges = store
        .list_all_edges("main")
        .expect("list edges")
        .into_iter()
        .filter(|edge| {
            edge.src == caller.id
                && edge.dst == replacement.id
                && edge.kind == EdgeKind::Calls
                && edge.line == Some(3)
        })
        .count();
    assert_eq!(
        matching_edges, 1,
        "an edge emitted by the diff must not also be restored"
    );
}

#[test]
fn delete_file_removes_nodes() {
    let (mut store, _dir) = tmp_store();

    let node = make_node("old_fn", NodeKind::Function, "src/old.rs", 1);
    let add_diff = GraphDiff {
        added_nodes: vec![node],
        ..Default::default()
    };
    store.apply_diff("main", &add_diff).expect("apply_diff");

    assert_eq!(
        store.lookup_symbol("main", "old_fn", false).unwrap().len(),
        1
    );

    let del_diff = GraphDiff {
        removed_files: vec![PathBuf::from("src/old.rs")],
        ..Default::default()
    };
    store
        .apply_diff("main", &del_diff)
        .expect("apply_diff remove");

    assert_eq!(
        store.lookup_symbol("main", "old_fn", false).unwrap().len(),
        0
    );
}

#[test]
fn removed_paths_never_delete_folders_and_ignore_extensions() {
    let (mut store, _dir) = tmp_store();
    let folder = make_node("v1.2", NodeKind::Folder, "v1.2", 1);
    let extensionless = make_node("build_rule", NodeKind::Function, "Makefile", 1);
    let setup = GraphDiff {
        added_nodes: vec![folder.clone(), extensionless],
        ..GraphDiff::default()
    };
    store.apply_diff("main", &setup).expect("seed");
    let removal = GraphDiff {
        removed_files: vec![PathBuf::from("v1.2"), PathBuf::from("Makefile")],
        ..GraphDiff::default()
    };
    store.apply_diff("main", &removal).expect("remove");
    let remaining = store.list_all_nodes("main").expect("list");
    assert_eq!(remaining.len(), 1, "{remaining:?}");
    assert_eq!(remaining[0].id, folder.id);
}

#[test]
fn last_indexed_sha_round_trip() {
    let (mut store, _dir) = tmp_store();

    assert!(store.last_indexed_sha("main").unwrap().is_none());

    store
        .set_last_indexed_sha("main", "abc123")
        .expect("set sha");
    assert_eq!(
        store.last_indexed_sha("main").unwrap().as_deref(),
        Some("abc123")
    );
    assert!(!store.working_tree_dirty("main").unwrap());
    store.set_working_tree_dirty("main", true).unwrap();
    assert!(store.working_tree_dirty("main").unwrap());
    assert!(!store.working_tree_dirty("feature").unwrap());
    assert_eq!(
        store.last_indexed_sha("main").unwrap().as_deref(),
        Some("abc123")
    );
    store.set_working_tree_dirty("main", false).unwrap();
    assert!(!store.working_tree_dirty("main").unwrap());
}

#[test]
fn paged_graph_reads_are_stable_and_complete() {
    let (mut store, _dir) = tmp_store();
    let nodes = vec![
        make_node("alpha", NodeKind::Function, "src/lib.rs", 1),
        make_node("beta", NodeKind::Function, "src/lib.rs", 10),
        make_node("gamma", NodeKind::Function, "src/lib.rs", 20),
    ];
    let edges = vec![
        Edge::call(nodes[0].id.clone(), nodes[1].id.clone(), 3),
        Edge::call(nodes[1].id.clone(), nodes[2].id.clone(), 12),
    ];
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: nodes,
                added_edges: edges,
                ..Default::default()
            },
        )
        .expect("apply graph");

    let first_nodes = store.list_nodes_page("main", 0, 2).expect("first nodes");
    let second_nodes = store.list_nodes_page("main", 2, 2).expect("second nodes");
    assert_eq!(first_nodes.len(), 2);
    assert_eq!(second_nodes.len(), 1);
    assert!(first_nodes[0].id.as_str() < first_nodes[1].id.as_str());

    let first_edge = store.list_edges_page("main", 0, 1).expect("first edge");
    let second_edge = store.list_edges_page("main", 1, 1).expect("second edge");
    assert_eq!(first_edge.len(), 1);
    assert_eq!(second_edge.len(), 1);
    assert_ne!(first_edge[0].src, second_edge[0].src);
}

#[test]
fn bounded_exact_neighborhood_respects_direction_and_limit() {
    let (mut store, _dir) = tmp_store();
    let seed = make_node("seed", NodeKind::Function, "src/lib.rs", 1);
    let incoming = make_node("incoming", NodeKind::Function, "src/lib.rs", 10);
    let outgoing = make_node("outgoing", NodeKind::Function, "src/lib.rs", 20);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![seed.clone(), incoming.clone(), outgoing.clone()],
                added_edges: vec![
                    Edge::call(incoming.id.clone(), seed.id.clone(), 12),
                    Edge::call(seed.id.clone(), outgoing.id.clone(), 3),
                ],
                ..Default::default()
            },
        )
        .expect("apply graph");

    let inbound = store
        .get_neighborhood_by_id("main", &seed.id.as_str(), "in", 10)
        .expect("inbound neighborhood");
    assert_eq!(inbound.edges.len(), 1);
    assert_eq!(inbound.edges[0].src, incoming.id);

    let limited = store
        .get_neighborhood_by_id("main", &seed.id.as_str(), "both", 1)
        .expect("limited neighborhood");
    assert_eq!(limited.edges.len(), 1);
    assert_eq!(limited.nodes.len(), 2);
}

#[test]
fn apply_diff_bumps_mutation_epoch_on_empty_and_incremental_paths() {
    let (mut store, _dir) = tmp_store();
    assert_eq!(store.mutation_epoch("main").expect("initial epoch"), 0);

    // First apply hits the empty-table bulk_apply fast path.
    let first = make_node("alpha", NodeKind::Function, "src/lib.rs", 1);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![first],
                ..Default::default()
            },
        )
        .expect("bulk apply");
    let after_bulk = store.mutation_epoch("main").expect("epoch after bulk");
    assert!(
        after_bulk > 0,
        "empty-table apply_diff fast path must bump mutation_epoch"
    );

    // Second apply hits the incremental path.
    let second = make_node("beta", NodeKind::Function, "src/other.rs", 1);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![second],
                ..Default::default()
            },
        )
        .expect("incremental apply");
    let after_inc = store
        .mutation_epoch("main")
        .expect("epoch after incremental");
    assert!(
        after_inc > after_bulk,
        "incremental apply_diff must advance mutation_epoch ({after_inc} <= {after_bulk})"
    );
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
        added_edges: vec![Edge::call(node_a.id.clone(), node_b.id.clone(), 3)],
        ..Default::default()
    };
    store.apply_diff("main", &main_diff).expect("apply main");

    // feat has shared + only_feat
    let feat_diff = GraphDiff {
        added_nodes: vec![node_a.clone(), node_c.clone()],
        added_edges: vec![Edge::call(node_a.id.clone(), node_c.id.clone(), 4)],
        ..Default::default()
    };
    store
        .apply_diff("feat/new", &feat_diff)
        .expect("apply feat");

    let diff = store.branch_diff("main", "feat/new").expect("branch_diff");

    // only_feat is in feat but not main → added
    assert!(diff.added_nodes.iter().any(|n| n.name == "only_feat"));
    // only_main is in main but not feat → removed
    assert!(!diff.removed_node_ids.is_empty());
    assert_eq!(diff.added_edges.len(), 1);
    assert_eq!(diff.added_edges[0].dst, node_c.id);
    assert_eq!(diff.removed_edges.len(), 1);
    assert_eq!(diff.removed_edges[0].1, node_b.id);
}

#[test]
fn schema_mismatch_requires_explicit_initialization_before_rebuild() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let repo_id = branch::repo_id(repo.path());
    let db_path = branch::db_path(&repo_id);
    std::fs::create_dir_all(db_path.parent().expect("data parent")).expect("create data dir");
    std::fs::write(&db_path, b"existing graph data").expect("fake existing graph");
    branch::write_schema_version(&repo_id, SCHEMA_VERSION.saturating_sub(1))
        .expect("old schema marker");

    let error = KuzuGraphStore::open(repo.path())
        .err()
        .expect("ordinary open must reject incompatible schema");
    assert!(error.to_string().contains("run `gcx init`"));
    assert_eq!(
        std::fs::read(&db_path).expect("preserved graph"),
        b"existing graph data"
    );

    let store = KuzuGraphStore::open_for_init(repo.path()).expect("explicit rebuild");
    drop(store);
    assert_eq!(branch::read_schema_version(&repo_id), SCHEMA_VERSION);
    branch::wipe_repo_data(&repo_id).expect("clean test data");
}

#[test]
fn full_index_bulk_reconciliation_matches_incremental_cypher_path() {
    let dir_bulk = tempfile::tempdir().expect("bulk tempdir");
    let dir_incr = tempfile::tempdir().expect("incr tempdir");
    let bulk_repo_id = branch::repo_id(dir_bulk.path());
    let incr_repo_id = branch::repo_id(dir_incr.path());
    let mut store_bulk = KuzuGraphStore::open_for_init(dir_bulk.path()).expect("open bulk");
    let mut store_incr = KuzuGraphStore::open_for_init(dir_incr.path()).expect("open incr");

    let mod_caller = make_node("mod_a", NodeKind::Module, "src/a.rs", 1);
    let caller = make_node("caller", NodeKind::Function, "src/a.rs", 5);
    let target_rs = make_node("helper", NodeKind::Function, "src/b.rs", 1);
    let target_py = make_node("helper", NodeKind::Function, "scripts/b.py", 1);
    let unimported = make_node("local_only", NodeKind::Function, "src/c.rs", 1);
    let widget_type = make_node("Widget", NodeKind::Struct, "src/types.rs", 1);

    let full_diff = GraphDiff {
        added_nodes: vec![
            mod_caller.clone(),
            caller.clone(),
            target_rs.clone(),
            target_py.clone(),
            unimported.clone(),
            widget_type.clone(),
        ],
        // Include a pre-existing Inferred edge that reconciliation must upgrade
        // to Resolved once the Imports fact is reconciled.
        added_edges: vec![Edge {
            src: caller.id.clone(),
            dst: target_rs.id.clone(),
            kind: EdgeKind::Calls,
            line: Some(10),
            confidence: gitcortex_core::schema::EdgeConfidence::Inferred,
        }],
        deferred_imports: vec![(mod_caller.id.clone(), "helper".into())],
        deferred_calls: vec![
            (caller.id.clone(), "helper".into(), 10),
            (caller.id.clone(), "helper".into(), 25),
            (caller.id.clone(), "local_only".into(), 30),
            (caller.id.clone(), "future_fn".into(), 40),
        ],
        deferred_uses: vec![(caller.id.clone(), "Widget".into())],
        ..Default::default()
    };

    // Store A: fresh empty branch takes the in-memory + CSV COPY bulk path.
    store_bulk
        .apply_diff("main", &full_diff)
        .expect("bulk apply_diff");

    // Store B: seed with an unrelated sentinel node first so the second diff
    // runs through the non-empty Cypher persist_reference_facts +
    // reconcile_reference_names path.
    let sentinel = make_node("sentinel", NodeKind::Function, "src/sentinel.rs", 1);
    store_incr
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![sentinel.clone()],
                ..Default::default()
            },
        )
        .expect("seed sentinel");
    let mut incr_diff = full_diff.clone();
    incr_diff.removed_node_ids.push(sentinel.id.clone());
    store_incr
        .apply_diff("main", &incr_diff)
        .expect("incremental apply_diff");

    let collect_edges = |store: &KuzuGraphStore| {
        let mut edges = store
            .list_edges_page("main", 0, 500)
            .expect("list_edges_page")
            .into_iter()
            .map(|e| {
                (
                    e.src.as_str(),
                    e.dst.as_str(),
                    e.kind.to_string(),
                    e.line,
                    e.confidence.to_string(),
                )
            })
            .collect::<Vec<_>>();
        edges.sort();
        edges
    };

    let bulk_edges = collect_edges(&store_bulk);
    let incr_edges = collect_edges(&store_incr);
    assert_eq!(bulk_edges, incr_edges);

    // Sanity-check key properties:
    // - Both call-site lines (10, 25) to Rust `helper` are Resolved.
    // - Python `helper` was excluded by language scoping.
    // - Unimported `local_only` is Inferred.
    // - `Widget` Uses edge exists.
    assert!(bulk_edges.iter().any(|(_, dst, k, ln, cf)| {
        dst == &target_rs.id.as_str() && k == "calls" && *ln == Some(10) && cf == "resolved"
    }));
    assert!(bulk_edges.iter().any(|(_, dst, k, ln, cf)| {
        dst == &target_rs.id.as_str() && k == "calls" && *ln == Some(25) && cf == "resolved"
    }));
    assert!(!bulk_edges
        .iter()
        .any(|(_, dst, _, _, _)| dst == &target_py.id.as_str()));
    assert!(bulk_edges.iter().any(|(_, dst, k, ln, cf)| {
        dst == &unimported.id.as_str() && k == "calls" && *ln == Some(30) && cf == "inferred"
    }));
    assert!(bulk_edges
        .iter()
        .any(|(_, dst, k, _, _)| { dst == &widget_type.id.as_str() && k == "uses" }));

    // Verify raw reference facts loaded via COPY refs.csv reconcile on a later
    // incremental edit when `future_fn` is introduced in a new file.
    let future_fn = make_node("future_fn", NodeKind::Function, "src/future.rs", 1);
    store_bulk
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![future_fn.clone()],
                ..Default::default()
            },
        )
        .expect("incremental add future_fn");
    let callers = store_bulk
        .find_callers("main", "future_fn")
        .expect("find_callers");
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].id, caller.id);
    let updated_edges = collect_edges(&store_bulk);
    assert!(updated_edges.iter().any(|(src, dst, k, ln, _)| {
        src == &caller.id.as_str()
            && dst == &future_fn.id.as_str()
            && k == "calls"
            && *ln == Some(40)
    }));

    drop(store_bulk);
    drop(store_incr);
    branch::wipe_repo_data(&bulk_repo_id).expect("clean bulk test data");
    branch::wipe_repo_data(&incr_repo_id).expect("clean incr test data");
}

#[test]
fn incremental_file_edit_preserves_repeated_incoming_calls_and_import_verification() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo_id = branch::repo_id(dir.path());
    let mut store = KuzuGraphStore::open(dir.path()).expect("open store");

    let caller = make_node("run_all", NodeKind::Function, "src/caller.rs", 1);
    let target = make_node("target_fn", NodeKind::Function, "src/target.rs", 1);
    let util = make_node("util_fn", NodeKind::Function, "src/util.rs", 1);
    let util_inferred = make_node("other_fn", NodeKind::Function, "src/util.rs", 10);

    // Seed graph so `caller` imports and calls `target_fn` on two lines.
    store
        .apply_diff(
            "main",
            &GraphDiff {
                added_nodes: vec![
                    caller.clone(),
                    target.clone(),
                    util.clone(),
                    util_inferred.clone(),
                ],
                deferred_imports: vec![(caller.id.clone(), "target_fn".to_owned())],
                deferred_calls: vec![
                    (caller.id.clone(), "target_fn".to_owned(), 10),
                    (caller.id.clone(), "target_fn".to_owned(), 20),
                ],
                ..Default::default()
            },
        )
        .expect("initial index");

    // Incrementally replace `src/target.rs`: keep `target_fn`, add `helper_fn`
    // which imports `util_fn` and calls both `util_fn` (import-verified) and
    // `other_fn` (inferred).
    let helper = make_node("helper_fn", NodeKind::Function, "src/target.rs", 25);
    store
        .apply_diff(
            "main",
            &GraphDiff {
                removed_files: vec![PathBuf::from("src/target.rs")],
                added_nodes: vec![target.clone(), helper.clone()],
                deferred_imports: vec![(helper.id.clone(), "util_fn".to_owned())],
                deferred_calls: vec![
                    (helper.id.clone(), "util_fn".to_owned(), 28),
                    (helper.id.clone(), "other_fn".to_owned(), 30),
                ],
                ..Default::default()
            },
        )
        .expect("incremental update");

    let edges = store
        .list_edges_page("main", 0, 500)
        .expect("list_edges_page");
    let incoming_call_lines: Vec<(Option<u32>, String)> = edges
        .iter()
        .filter(|e| e.src == caller.id && e.dst == target.id && e.kind == EdgeKind::Calls)
        .map(|e| (e.line, e.confidence.to_string()))
        .collect();
    assert_eq!(incoming_call_lines.len(), 2);
    assert!(incoming_call_lines.contains(&(Some(10), "resolved".to_owned())));
    assert!(incoming_call_lines.contains(&(Some(20), "resolved".to_owned())));

    let util_call = edges
        .iter()
        .find(|e| e.src == helper.id && e.dst == util.id && e.kind == EdgeKind::Calls)
        .expect("helper -> util_fn call edge");
    assert_eq!(util_call.line, Some(28));
    assert_eq!(
        util_call.confidence.to_string(),
        "resolved",
        "edges={edges:?}"
    );

    let other_call = edges
        .iter()
        .find(|e| e.src == helper.id && e.dst == util_inferred.id && e.kind == EdgeKind::Calls)
        .expect("helper -> other_fn call edge");
    assert_eq!(other_call.line, Some(30));
    assert_eq!(other_call.confidence.to_string(), "inferred");

    drop(store);
    branch::wipe_repo_data(&repo_id).expect("clean test data");
}
