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

use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/integration/fixtures");

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
    for args in [
        vec!["add", dest_name],
        vec!["commit", "-m", "add fixture"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .expect("git failed");
        assert!(status.success(), "git {args:?} failed");
    }
}

fn run_pipeline(fixture: &str) -> (Vec<gitcortex_core::graph::Node>, Vec<gitcortex_core::graph::Edge>) {
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
    store.set_last_indexed_sha("main", &head_sha).expect("set sha");

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
    assert!(names.contains(&"make_greeting"), "expected fn make_greeting");
}

#[test]
fn python_fixture_indexes_nodes_and_edges() {
    let (nodes, edges) = run_pipeline("sample.py");
    assert!(!nodes.is_empty(), "expected nodes for sample.py");
    assert!(!edges.is_empty(), "expected edges for sample.py");
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Greeter"), "expected class Greeter");
    assert!(names.contains(&"make_greeting"), "expected fn make_greeting");
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
