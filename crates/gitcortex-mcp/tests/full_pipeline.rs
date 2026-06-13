/// End-to-end pipeline tests: git repo → indexer → store → query.
/// Each test creates a real temp git repo, commits a fixture file,
/// runs the incremental indexer, applies the diff to a KuzuDB store,
/// and asserts that nodes and known symbols are present.
///
/// Tests are serialized via KUZU_LOCK because KuzuDB cannot safely open
/// multiple database instances concurrently within the same process.
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

static KUZU_LOCK: Mutex<()> = Mutex::new(());

/// Commit multiple fixture files in a single git commit.
fn commit_files(dir: &Path, fixtures: &[&str]) {
    for name in fixtures {
        let src = Path::new(FIXTURES).join(name);
        std::fs::copy(&src, dir.join(name)).expect("copy fixture");
    }
    let names: Vec<&str> = fixtures.to_vec();
    let mut add_args = vec!["add"];
    add_args.extend_from_slice(&names);
    let status = Command::new("git")
        .args(&add_args)
        .current_dir(dir)
        .status()
        .expect("git add failed");
    assert!(status.success(), "git add failed");
    let status = Command::new("git")
        .args(["commit", "-m", "add fixtures"])
        .current_dir(dir)
        .status()
        .expect("git commit failed");
    assert!(status.success(), "git commit failed");
}

/// Run the full pipeline against multiple fixture files committed together.
fn run_pipeline_multi(
    fixtures: &[&str],
) -> (
    Vec<gitcortex_core::graph::Node>,
    Vec<gitcortex_core::graph::Edge>,
    KuzuGraphStore,
) {
    let _lock = KUZU_LOCK.lock().expect("lock");
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    commit_files(tmp.path(), fixtures);

    let indexer = IncrementalIndexer::new(tmp.path()).expect("indexer");
    let (diff, head_sha) = indexer.run(None).expect("indexer.run");

    let mut store = KuzuGraphStore::open(tmp.path()).expect("store");
    store.apply_diff("main", &diff).expect("apply_diff");
    store
        .set_last_indexed_sha("main", &head_sha)
        .expect("set sha");

    let nodes = store.list_all_nodes("main").expect("list_all_nodes");
    let edges = store.list_all_edges("main").expect("list_all_edges");
    (nodes, edges, store)
}

use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/integration/fixtures"
);

fn init_repo(dir: &Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@test.com"],
        vec!["config", "user.name", "Test"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .expect("git failed");
        assert!(status.success(), "git {args:?} failed");
    }
}

fn commit_file(dir: &Path, src: &Path, dest_name: &str) {
    let dest = dir.join(dest_name);
    std::fs::copy(src, &dest).expect("copy fixture");
    for args in [vec!["add", dest_name], vec!["commit", "-m", "add fixture"]] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .expect("git failed");
        assert!(status.success(), "git {args:?} failed");
    }
}

fn run_pipeline(
    fixture: &str,
) -> (
    Vec<gitcortex_core::graph::Node>,
    Vec<gitcortex_core::graph::Edge>,
) {
    let _lock = KUZU_LOCK.lock().expect("lock");
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    commit_file(
        tmp.path(),
        Path::new(FIXTURES).join(fixture).as_path(),
        fixture,
    );

    let indexer = IncrementalIndexer::new(tmp.path()).expect("indexer");
    let (diff, head_sha) = indexer.run(None).expect("indexer.run");

    let mut store = KuzuGraphStore::open(tmp.path()).expect("store");
    store.apply_diff("main", &diff).expect("apply_diff");
    store
        .set_last_indexed_sha("main", &head_sha)
        .expect("set sha");

    let nodes = store.list_all_nodes("main").expect("list_all_nodes");
    let edges = store.list_all_edges("main").expect("list_all_edges");
    (nodes, edges)
}

#[test]
fn rust_fixture_indexes_nodes_and_edges() {
    let (nodes, edges) = run_pipeline("sample.rs");
    assert!(!nodes.is_empty(), "expected nodes for sample.rs");
    assert!(!edges.is_empty(), "expected edges for sample.rs");
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Hello"), "expected struct Hello");
    assert!(names.contains(&"Greeter"), "expected trait Greeter");
    assert!(
        names.contains(&"make_greeting"),
        "expected fn make_greeting"
    );
}

#[test]
fn python_fixture_indexes_nodes_and_edges() {
    let (nodes, edges) = run_pipeline("sample.py");
    assert!(!nodes.is_empty(), "expected nodes for sample.py");
    assert!(!edges.is_empty(), "expected edges for sample.py");
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Greeter"), "expected class Greeter");
    assert!(
        names.contains(&"make_greeting"),
        "expected fn make_greeting"
    );
}

#[test]
fn typescript_fixture_indexes_nodes_and_edges() {
    let (nodes, edges) = run_pipeline("sample.ts");
    assert!(!nodes.is_empty(), "expected nodes for sample.ts");
    assert!(!edges.is_empty(), "expected edges for sample.ts");
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Greeter"), "expected interface Greeter");
    assert!(names.contains(&"Hello"), "expected class Hello");
    assert!(names.contains(&"makeGreeting"), "expected fn makeGreeting");
}

#[test]
fn go_fixture_indexes_nodes_and_edges() {
    let (nodes, edges) = run_pipeline("sample.go");
    assert!(!nodes.is_empty(), "expected nodes for sample.go");
    assert!(!edges.is_empty(), "expected edges for sample.go");
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Greeter"), "expected interface Greeter");
    assert!(names.contains(&"Hello"), "expected struct Hello");
    assert!(names.contains(&"MakeGreeting"), "expected fn MakeGreeting");
}

#[test]
fn java_fixture_indexes_nodes_and_edges() {
    let (nodes, edges) = run_pipeline("sample.java");
    assert!(!nodes.is_empty(), "expected nodes for sample.java");
    assert!(!edges.is_empty(), "expected edges for sample.java");
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Greeter"), "expected interface Greeter");
    assert!(names.contains(&"Hello"), "expected class Hello");
    assert!(names.contains(&"makeGreeting"), "expected fn makeGreeting");
}

// ── Python comprehensive regression tests ────────────────────────────────────

use gitcortex_core::schema::{EdgeKind, NodeKind};

fn run_python_comprehensive() -> (
    Vec<gitcortex_core::graph::Node>,
    Vec<gitcortex_core::graph::Edge>,
) {
    run_pipeline("python_comprehensive.py")
}

#[test]
fn python_comprehensive_constants_are_indexed() {
    let (nodes, _) = run_python_comprehensive();
    let constants: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Constant)
        .collect();
    let names: Vec<&str> = constants.iter().map(|n| n.name.as_str()).collect();
    assert!(
        names.contains(&"MAX_RETRIES"),
        "expected Constant MAX_RETRIES, got: {names:?}"
    );
    assert!(
        names.contains(&"DEFAULT_TIMEOUT"),
        "expected Constant DEFAULT_TIMEOUT, got: {names:?}"
    );
    assert!(
        names.contains(&"API_VERSION"),
        "expected Constant API_VERSION, got: {names:?}"
    );
}

#[test]
fn python_comprehensive_protocols_become_interfaces() {
    let (nodes, _) = run_python_comprehensive();
    let interfaces: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Interface)
        .collect();
    let names: Vec<&str> = interfaces.iter().map(|n| n.name.as_str()).collect();
    assert!(
        names.contains(&"Serializable"),
        "expected Interface Serializable, got: {names:?}"
    );
    assert!(
        names.contains(&"Repository"),
        "expected Interface Repository, got: {names:?}"
    );
    for iface in &interfaces {
        assert!(
            iface.metadata.is_abstract,
            "Protocol node '{}' should have is_abstract=true",
            iface.name
        );
    }
}

#[test]
fn python_comprehensive_plain_classes_are_structs() {
    let (nodes, _) = run_python_comprehensive();
    let structs: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Struct)
        .collect();
    let names: Vec<&str> = structs.iter().map(|n| n.name.as_str()).collect();
    for expected in &["BaseModel", "User", "AsyncService", "EventSystem"] {
        assert!(
            names.contains(expected),
            "expected Struct {expected}, got: {names:?}"
        );
    }
}

#[test]
fn python_comprehensive_property_decorator() {
    let (nodes, _) = run_python_comprehensive();
    let props: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Property)
        .collect();
    assert!(
        props.iter().any(|n| n.name == "display_name"),
        "expected Property 'display_name', got: {:?}",
        props.iter().map(|n| &n.name).collect::<Vec<_>>()
    );
    let display_name = props.iter().find(|n| n.name == "display_name").unwrap();
    assert!(
        display_name.metadata.is_property,
        "display_name should have is_property=true"
    );
}

#[test]
fn python_comprehensive_staticmethod_and_classmethod() {
    let (nodes, _) = run_python_comprehensive();
    let methods: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Method)
        .collect();
    let from_dict = methods.iter().find(|n| n.name == "from_dict");
    let anonymous = methods.iter().find(|n| n.name == "anonymous");
    assert!(from_dict.is_some(), "expected method 'from_dict'");
    assert!(anonymous.is_some(), "expected method 'anonymous'");
    assert!(
        from_dict.unwrap().metadata.is_static,
        "from_dict (@staticmethod) should have is_static=true"
    );
    assert!(
        anonymous.unwrap().metadata.is_static,
        "anonymous (@classmethod) should have is_static=true"
    );
}

#[test]
fn python_comprehensive_async_methods_flagged() {
    let (nodes, _) = run_python_comprehensive();
    let methods: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Method)
        .collect();
    let fetch = methods.iter().find(|n| n.name == "fetch_user");
    let save = methods.iter().find(|n| n.name == "save_user");
    assert!(fetch.is_some(), "expected method 'fetch_user'");
    assert!(save.is_some(), "expected method 'save_user'");
    assert!(
        fetch.unwrap().metadata.is_async,
        "fetch_user should have is_async=true"
    );
    assert!(
        save.unwrap().metadata.is_async,
        "save_user should have is_async=true"
    );
}

#[test]
fn python_comprehensive_generator_function_flagged() {
    let (nodes, _) = run_python_comprehensive();
    let fns: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Function)
        .collect();
    let user_stream = fns.iter().find(|n| n.name == "user_stream");
    assert!(user_stream.is_some(), "expected function 'user_stream'");
    assert!(
        user_stream.unwrap().metadata.is_generator,
        "user_stream should have is_generator=true"
    );
}

#[test]
fn python_comprehensive_async_generator_flagged() {
    let (nodes, _) = run_python_comprehensive();
    let fns: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Function)
        .collect();
    let async_stream = fns.iter().find(|n| n.name == "async_user_stream");
    assert!(
        async_stream.is_some(),
        "expected function 'async_user_stream'"
    );
    let f = async_stream.unwrap();
    assert!(
        f.metadata.is_async,
        "async_user_stream should have is_async=true"
    );
    assert!(
        f.metadata.is_generator,
        "async_user_stream should have is_generator=true"
    );
}

#[test]
fn python_comprehensive_nested_classes_indexed() {
    let (nodes, edges) = run_python_comprehensive();
    let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Event"), "expected nested class 'Event'");
    assert!(
        names.contains(&"Handler"),
        "expected nested class 'Handler'"
    );
    // EventSystem must contain Event and Handler via Contains edges
    let event_sys = nodes.iter().find(|n| n.name == "EventSystem").unwrap();
    let event_cls = nodes.iter().find(|n| n.name == "Event").unwrap();
    let handler_cls = nodes.iter().find(|n| n.name == "Handler").unwrap();
    let contains: Vec<_> = edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains)
        .collect();
    assert!(
        contains
            .iter()
            .any(|e| e.src == event_sys.id && e.dst == event_cls.id),
        "expected Contains edge EventSystem → Event"
    );
    assert!(
        contains
            .iter()
            .any(|e| e.src == event_sys.id && e.dst == handler_cls.id),
        "expected Contains edge EventSystem → Handler"
    );
}

#[test]
fn python_comprehensive_call_edges_recorded() {
    let (nodes, edges) = run_python_comprehensive();
    let process = nodes.iter().find(|n| n.name == "process_pipeline").unwrap();
    let user_stream = nodes.iter().find(|n| n.name == "user_stream").unwrap();
    let create_user = nodes.iter().find(|n| n.name == "create_user").unwrap();
    let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
    assert!(
        calls
            .iter()
            .any(|e| e.src == process.id && e.dst == user_stream.id),
        "expected Calls edge process_pipeline → user_stream"
    );
    assert!(
        calls
            .iter()
            .any(|e| e.src == process.id && e.dst == create_user.id),
        "expected Calls edge process_pipeline → create_user"
    );
}

#[test]
fn python_comprehensive_inheritance_edges_present() {
    let (_, edges) = run_python_comprehensive();
    let implements: Vec<_> = edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Implements)
        .collect();
    assert!(
        !implements.is_empty(),
        "expected at least one Implements edge (User extends BaseModel)"
    );
}

#[test]
fn python_comprehensive_private_method_visibility() {
    use gitcortex_core::schema::Visibility;
    let (nodes, _) = run_python_comprehensive();
    let internal = nodes.iter().find(|n| n.name == "_internal_check");
    assert!(internal.is_some(), "expected method '_internal_check'");
    assert_eq!(
        internal.unwrap().metadata.visibility,
        Visibility::Private,
        "_internal_check should be Private"
    );
}

#[test]
fn python_comprehensive_dataclass_is_struct() {
    let (nodes, _) = run_python_comprehensive();
    let user = nodes.iter().find(|n| n.name == "User");
    assert!(user.is_some(), "expected class 'User'");
    assert_eq!(
        user.unwrap().kind,
        NodeKind::Struct,
        "@dataclass User should be Struct kind"
    );
}

// ── Cross-file deferred edge tests ────────────────────────────────────────────
// These tests verify that deferred edge resolution works across file boundaries:
// callers in one file resolve to callees defined in a separate file.

#[test]
fn cross_file_calls_edge_resolved() {
    let (nodes, _edges, store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);

    // Both files' nodes should be indexed.
    assert!(
        nodes.iter().any(|n| n.name == "compute_value"),
        "expected 'compute_value' node from xfile_callee.rs"
    );
    assert!(
        nodes.iter().any(|n| n.name == "run"),
        "expected 'run' node from xfile_caller.rs"
    );

    // Deferred call from `run` in xfile_caller.rs → `compute_value` in xfile_callee.rs
    // should have been resolved as a Calls edge.
    let callers = store
        .find_callers("main", "compute_value")
        .expect("find_callers");
    assert!(
        callers.iter().any(|n| n.name == "run"),
        "expected 'run' as a caller of 'compute_value' across files; got: {:?}",
        callers.iter().map(|n| &n.name).collect::<Vec<_>>()
    );
}

#[test]
fn cross_file_calls_edge_multiple_callers() {
    let (_nodes, _, store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);

    // Both `run` and `run_with_branch` call `compute_value`.
    let callers = store
        .find_callers("main", "compute_value")
        .expect("find_callers");
    let caller_names: Vec<&str> = callers.iter().map(|n| n.name.as_str()).collect();
    assert!(
        caller_names.contains(&"run"),
        "expected 'run' in callers; got {caller_names:?}"
    );
    assert!(
        caller_names.contains(&"run_with_branch"),
        "expected 'run_with_branch' in callers; got {caller_names:?}"
    );
}

#[test]
fn cross_file_implements_edge_resolved() {
    let (nodes, edges, _store) = run_pipeline_multi(&["xfile_trait.rs", "xfile_impl.rs"]);

    assert!(
        nodes.iter().any(|n| n.name == "Processor"),
        "expected 'Processor' trait from xfile_trait.rs"
    );
    assert!(
        nodes.iter().any(|n| n.name == "Worker"),
        "expected 'Worker' struct from xfile_impl.rs"
    );

    // Worker implements Processor — deferred Implements edge should be present.
    let impl_edges: Vec<_> = edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Implements)
        .collect();
    assert!(
        !impl_edges.is_empty(),
        "expected at least one Implements edge (Worker → Processor)"
    );
}

#[test]
fn ast_search_async_methods_only() {
    use gitcortex_core::store::AttributeFilter;
    let (_, _, store) = run_pipeline_multi(&["python_comprehensive.py"]);
    let filter = AttributeFilter {
        kind: Some(NodeKind::Method),
        is_async: Some(true),
        ..Default::default()
    };
    let hits = store
        .search_by_attributes("main", &filter, 50)
        .expect("search_by_attributes");
    let names: Vec<&str> = hits.iter().map(|n| n.name.as_str()).collect();
    // AsyncService.fetch_user / save_user are async methods.
    assert!(
        names.contains(&"fetch_user"),
        "expected async method fetch_user, got: {names:?}"
    );
    // Every result must actually be an async method.
    for n in &hits {
        assert_eq!(n.kind, NodeKind::Method);
        assert!(n.metadata.is_async, "{} not async", n.name);
    }
}

#[test]
fn ast_search_kind_filter_excludes_others() {
    use gitcortex_core::store::AttributeFilter;
    let (_, _, store) = run_pipeline_multi(&["sample.rs"]);
    let filter = AttributeFilter {
        kind: Some(NodeKind::Trait),
        ..Default::default()
    };
    let hits = store
        .search_by_attributes("main", &filter, 50)
        .expect("search_by_attributes");
    assert!(!hits.is_empty(), "expected at least the Greeter trait");
    for n in &hits {
        assert_eq!(n.kind, NodeKind::Trait, "{} is not a trait", n.name);
    }
}

#[test]
fn ast_search_complexity_lower_bound() {
    use gitcortex_core::store::AttributeFilter;
    // run_with_branch has complexity 2; a min of 2 must include it, min of 3 must not.
    let (_, _, store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);

    let at_least_2 = store
        .search_by_attributes(
            "main",
            &AttributeFilter {
                min_complexity: Some(2),
                ..Default::default()
            },
            50,
        )
        .expect("search");
    assert!(
        at_least_2.iter().any(|n| n.name == "run_with_branch"),
        "complexity≥2 should include run_with_branch"
    );

    let at_least_3 = store
        .search_by_attributes(
            "main",
            &AttributeFilter {
                min_complexity: Some(3),
                ..Default::default()
            },
            50,
        )
        .expect("search");
    assert!(
        !at_least_3.iter().any(|n| n.name == "run_with_branch"),
        "complexity≥3 should exclude run_with_branch (complexity 2)"
    );
}

#[test]
fn graph_stats_totals_match_kind_sums() {
    let (nodes, edges, store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);
    let stats = store.graph_stats("main").expect("graph_stats");

    // Totals must equal the raw node/edge counts.
    assert_eq!(stats.total_nodes as usize, nodes.len());
    assert_eq!(stats.total_edges as usize, edges.len());

    // Per-kind tallies must sum to the totals.
    let node_sum: u64 = stats.nodes_by_kind.iter().map(|(_, c)| c).sum();
    let edge_sum: u64 = stats.edges_by_kind.iter().map(|(_, c)| c).sum();
    assert_eq!(node_sum, stats.total_nodes);
    assert_eq!(edge_sum, stats.total_edges);

    // Known fixtures: function nodes and at least one calls edge exist.
    assert!(
        stats
            .nodes_by_kind
            .iter()
            .any(|(k, c)| k == "function" && *c >= 2),
        "expected ≥2 function nodes, got: {:?}",
        stats.nodes_by_kind
    );
    assert!(
        stats
            .edges_by_kind
            .iter()
            .any(|(k, c)| k == "calls" && *c >= 1),
        "expected ≥1 calls edge, got: {:?}",
        stats.edges_by_kind
    );
}

#[test]
fn graph_stats_kind_counts_sorted_descending() {
    let (_, _, store) = run_pipeline_multi(&["sample.py", "python_comprehensive.py"]);
    let stats = store.graph_stats("main").expect("graph_stats");
    // Output must be sorted by count descending for deterministic display.
    for w in stats.nodes_by_kind.windows(2) {
        assert!(
            w[0].1 >= w[1].1,
            "nodes_by_kind not sorted desc: {:?}",
            stats.nodes_by_kind
        );
    }
}

#[test]
fn find_unused_symbols_returns_uncalled_nodes() {
    // compute_value is CALLED by run() and run_with_branch(), so it should NOT
    // appear as unused. DataStore is defined but never called or used as a type.
    let (_, _, store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);
    let unused = store
        .find_unused_symbols("main", None)
        .expect("find_unused_symbols");
    let unused_names: Vec<&str> = unused.iter().map(|n| n.name.as_str()).collect();

    assert!(
        !unused_names.contains(&"compute_value"),
        "compute_value is called by run() — must not appear in unused: {unused_names:?}"
    );
    assert!(
        unused_names.contains(&"DataStore"),
        "DataStore has no callers or uses — must appear in unused: {unused_names:?}"
    );
}

#[test]
fn find_unused_symbols_kind_filter() {
    // With kind=function, only functions/methods should be returned.
    let (_, _, store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);
    let unused = store
        .find_unused_symbols("main", Some(NodeKind::Function))
        .expect("find_unused_symbols kind=function");
    for node in &unused {
        assert!(
            matches!(node.kind, NodeKind::Function | NodeKind::Method),
            "kind filter returned non-function: {node:?}"
        );
    }
}

#[test]
fn cyclomatic_complexity_simple_function() {
    let (nodes, _) = run_pipeline("sample.rs");
    // make_greeting calls h.greet() — linear function, complexity should be 1.
    let f = nodes.iter().find(|n| n.name == "make_greeting");
    assert!(f.is_some(), "expected 'make_greeting'");
    let complexity = f.unwrap().metadata.lld.complexity;
    assert_eq!(
        complexity,
        Some(1),
        "linear function should have complexity 1, got {complexity:?}"
    );
}

#[test]
fn cyclomatic_complexity_branching_function() {
    let (nodes, _, _store) = run_pipeline_multi(&["xfile_callee.rs", "xfile_caller.rs"]);
    // run_with_branch has one `if` → complexity 2.
    let f = nodes.iter().find(|n| n.name == "run_with_branch");
    assert!(f.is_some(), "expected 'run_with_branch'");
    let complexity = f.unwrap().metadata.lld.complexity;
    assert_eq!(
        complexity,
        Some(2),
        "function with one `if` should have complexity 2, got {complexity:?}"
    );
}
