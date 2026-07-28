/// Behavioral assertions for the search scoring ladder.
///
/// All tests share a single indexed store (built once via OnceLock).
/// The store is wrapped in a Mutex because KuzuDB only allows one write
/// transaction at a time — ensure_branch() creates tables on first access,
/// so concurrent test threads would race. Mutex serialises all access.
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use gitcortex_core::store::GraphStore;
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_mcp::mcp::{
    agent::{format_search, AgentStatus},
    search::search,
};
use gitcortex_store::kuzu::KuzuGraphStore;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/integration/fixtures"
);

struct SharedStore {
    store: Mutex<KuzuGraphStore>,
    _tmp: tempfile::TempDir,
}

static STORE: OnceLock<SharedStore> = OnceLock::new();

fn with_store<F, R>(f: F) -> R
where
    F: FnOnce(&KuzuGraphStore) -> R,
{
    let shared = STORE.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("tempdir");
        init_repo(tmp.path());
        commit_files(
            tmp.path(),
            &[
                "sample.rs",
                "sample.py",
                "python_comprehensive.py",
                "sample.go",
                "sample.ts",
                "sample.java",
                "xfile_callee.rs",
                "xfile_caller.rs",
                "xfile_trait.rs",
                "xfile_impl.rs",
            ],
        );
        let indexer = IncrementalIndexer::new(tmp.path()).expect("indexer");
        let (diff, head_sha) = indexer.run(None).expect("indexer.run");
        let mut store = KuzuGraphStore::open(tmp.path()).expect("store");
        store.apply_diff("main", &diff).expect("apply_diff");
        store
            .set_last_indexed_sha("main", &head_sha)
            .expect("set sha");
        // Warm up ensure_branch so subsequent read queries don't trigger writes.
        let _ = store.list_all_nodes("main").expect("warmup");
        SharedStore {
            store: Mutex::new(store),
            _tmp: tmp,
        }
    });
    // Recover from poison: a previous test panic shouldn't break all others.
    let guard = shared.store.lock().unwrap_or_else(|e| e.into_inner());
    f(&guard)
}

fn init_repo(dir: &Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@test.com"],
        vec!["config", "user.name", "Test"],
    ] {
        assert!(Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }
}

fn commit_files(dir: &Path, fixtures: &[&str]) {
    for name in fixtures {
        std::fs::copy(Path::new(FIXTURES).join(name), dir.join(name)).expect("copy fixture");
    }
    let mut add_args = vec!["add"];
    add_args.extend_from_slice(fixtures);
    assert!(Command::new("git")
        .args(&add_args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "add fixtures"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
}

fn assert_within_top(query: &str, expected_name: &str, k: usize) {
    with_store(|s| {
        let hits = search(s, "main", query, Some(k)).expect("search");
        if !hits.iter().any(|h| h.name == expected_name) {
            let top: Vec<_> = hits
                .iter()
                .map(|h| format!("{}({})", h.name, h.score))
                .collect();
            panic!("query={query:?} expected {expected_name:?} in top-{k}\n  got: {top:?}");
        }
    });
}

// ── Exact ─────────────────────────────────────────────────────────────────────

#[test]
fn exact_snake_case() {
    assert_within_top("make_greeting", "make_greeting", 1);
}

#[test]
fn exact_struct() {
    assert_within_top("DataStore", "DataStore", 1);
}

#[test]
fn exact_cross_language_greeter() {
    with_store(|s| {
        let hits = search(s, "main", "Greeter", Some(10)).expect("search");
        assert!(
            hits.iter().any(|h| h.name == "Greeter"),
            "expected Greeter in results, got: {:?}",
            hits.iter().map(|h| &h.name).collect::<Vec<_>>()
        );
    });
}

// ── Prefix ────────────────────────────────────────────────────────────────────

#[test]
fn prefix_snake() {
    assert_within_top("make_greet", "make_greeting", 3);
}

#[test]
fn prefix_struct() {
    assert_within_top("DataSt", "DataStore", 3);
}

#[test]
fn snake_prefix_query() {
    assert_within_top("run_with", "run_with_branch", 3);
}

// ── Token splitting ───────────────────────────────────────────────────────────

#[test]
fn camel_token_split() {
    assert_within_top("makeGreeting", "makeGreeting", 3);
}

#[test]
fn space_separated_finds_snake() {
    // "make greeting" → tokens [make, greeting] → finds make_greeting
    assert_within_top("make greeting", "make_greeting", 5);
}

#[test]
fn space_separated_finds_camel() {
    assert_within_top("make greeting", "makeGreeting", 5);
}

#[test]
fn multi_token_finds_eventsystem() {
    // "event system" → tokens [event, system] → EventSystem (Python class)
    assert_within_top("event system", "EventSystem", 5);
}

// ── Typo tolerance ────────────────────────────────────────────────────────────

#[test]
fn typo_one_edit_snake() {
    // "make_greting" (missing 'e') → make_greeting
    assert_within_top("make_greting", "make_greeting", 5);
}

#[test]
fn typo_double_letter() {
    // "Greetter" (extra 't') → Greeter
    assert_within_top("Greetter", "Greeter", 5);
}

// ── Edge cases ────────────────────────────────────────────────────────────────

#[test]
fn empty_query_returns_empty() {
    with_store(|s| {
        let hits = search(s, "main", "", Some(10)).expect("search");
        assert!(hits.is_empty(), "expected empty, got {hits:?}");
    });
}

#[test]
fn whitespace_query_returns_empty() {
    with_store(|s| {
        let hits = search(s, "main", "   ", Some(10)).expect("search");
        assert!(hits.is_empty(), "expected empty, got {hits:?}");
    });
}

#[test]
fn agent_search_contract_adds_source_evidence_within_budget() {
    with_store(|store| {
        let hits = search(store, "main", "Greeter", Some(10)).expect("search");
        let response =
            format_search(store, "main", "Greeter", hits, false, 400).expect("format search");
        assert_eq!(response.status, AgentStatus::Ok);
        assert!(!response.evidence.is_empty());
        assert!(response
            .evidence
            .iter()
            .any(|item| !item.signature.is_empty()));
        assert!(serde_json::to_vec(&response).unwrap().len() <= 1_600);
    });
}
