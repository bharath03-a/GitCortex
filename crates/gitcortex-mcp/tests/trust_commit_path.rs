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
#[test]
fn after_modify_in_dotted_directory() {
    let repo = TestRepo::new();
    repo.write("v1.2/changed.rs", "pub fn changed() {}\n");
    repo.write("v1.2/sibling.rs", "pub fn sibling() {}\n");
    repo.write("v1.2/pkg.d/nested.rs", "pub fn nested() {}\n");
    repo.write("plain/inner/other.rs", "pub fn other() {}\n");
    repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    repo.write("v1.2/changed.rs", "pub fn changed_again() {}\n");
    repo.commit("modify beside siblings");
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
    repo.write("v1.2/pkg.d/nested.rs", "pub fn nested_again() {}\n");
    repo.commit("modify in nested dotted directory");
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
    repo.write("plain/inner/other.rs", "pub fn other_again() {}\n");
    repo.commit("modify in nested plain directory");
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
}
#[test]
fn after_callee_added_later() {
    let repo = TestRepo::new();
    repo.write("caller.rs", "pub fn caller() { later(); }\n");
    repo.commit("caller first");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    repo.write("later.rs", "pub fn later() {}\n");
    repo.commit("callee arrives");
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
}
#[test]
fn after_delete_last_file_in_directory() {
    let repo = TestRepo::new();
    repo.write("keep/surviving.rs", "pub fn surviving() {}\n");
    repo.write("parent/sibling/stay.rs", "pub fn stay() {}\n");
    repo.write("parent/vanish/deep/inner/gone.rs", "pub fn gone() {}\n");
    repo.write("standalone_dir/last.rs", "pub fn last() {}\n");
    repo.commit("initial");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    repo.remove("standalone_dir/last.rs");
    repo.remove("parent/vanish/deep/inner/gone.rs");
    repo.commit("delete last files in directories");
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");
}

#[test]
fn python_class_instantiation_uses_edges_match_rebuild_and_rank_in_subgraph() {
    use gitcortex_core::{
        schema::{EdgeConfidence, EdgeKind, NodeKind},
        store::GraphStore,
    };
    use gitcortex_mcp::mcp::agent::{self, AgentQueryOptions, AgentStatus};

    let repo = TestRepo::new();
    // Commit 1: caller arrives BEFORE Session is defined (exercises incremental
    // reconciliation when the target class arrives in a later commit).
    repo.write(
        "src/requests/api.py",
        "\
from . import sessions

def request(method, url, **kwargs):
    with sessions.Session() as session:
        return session.request(method=method, url=url, **kwargs)
",
    );
    repo.commit("add api.py before sessions.py");
    let mut store = KuzuGraphStore::open(repo.root()).unwrap();
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");

    // Commit 2: Session class with 20 methods + same-file factory + direct import caller
    // + a noisy test suite importing Session explicitly (resolved confidence, is_test=true).
    let mut sessions_py = String::from("class Session:\n");
    for i in 0..20 {
        sessions_py.push_str(&format!("    def method_{i:02}(self):\n        pass\n"));
    }
    sessions_py.push_str("\ndef session():\n    return Session()\n");
    repo.write("src/requests/sessions.py", &sessions_py);
    repo.write(
        "src/requests/direct.py",
        "\
from .sessions import Session

def build_client():
    return Session()
",
    );
    let mut tests_py = String::from("from requests.sessions import Session\n\n");
    for i in 0..25 {
        tests_py.push_str(&format!(
            "def test_session_{i:02}():\n    return Session()\n"
        ));
    }
    repo.write("tests/test_sessions.py", &tests_py);
    repo.commit("add Session class, direct import caller, and tests");
    index_hook_path(&repo, &mut store, "main");
    assert_matches_rebuild(&repo, &store, "main");

    // Verify Uses edges from all three production call patterns to Session (Struct).
    let nodes = store.list_all_nodes("main").unwrap();
    let edges = store.list_all_edges("main").unwrap();
    let session_node = nodes
        .iter()
        .find(|n| n.kind == NodeKind::Struct && n.name == "Session")
        .expect("Session struct node");
    let fn_id = |file: &str, name: &str| {
        nodes
            .iter()
            .find(|n| {
                n.kind == NodeKind::Function && n.name == name && n.file.to_string_lossy() == file
            })
            .unwrap_or_else(|| panic!("missing function {file}::{name}"))
            .id
            .clone()
    };
    let api_request = fn_id("src/requests/api.py", "request");
    let direct_build = fn_id("src/requests/direct.py", "build_client");
    let same_file_session = fn_id("src/requests/sessions.py", "session");

    assert!(
        edges.iter().any(|e| {
            e.kind == EdgeKind::Uses && e.src == api_request && e.dst == session_node.id
        }),
        "expected Uses edge from src/requests/api.py::request -> Session"
    );
    assert!(
        edges.iter().any(|e| {
            e.kind == EdgeKind::Uses
                && e.src == direct_build
                && e.dst == session_node.id
                && e.confidence == EdgeConfidence::Resolved
        }),
        "expected Resolved Uses edge from src/requests/direct.py::build_client -> Session"
    );
    assert!(
        edges.iter().any(|e| {
            e.kind == EdgeKind::Uses && e.src == same_file_session && e.dst == session_node.id
        }),
        "expected Uses edge from src/requests/sessions.py::session -> Session"
    );

    // Verify get_subgraph ranks production used_by ahead of test used_by and contains methods.
    let sub = agent::get_subgraph(
        &store,
        "main",
        "Session",
        2,
        "both",
        AgentQueryOptions {
            limit: 20,
            budget_tokens: 1200,
        },
    )
    .expect("get_subgraph");
    assert_eq!(sub.status, AgentStatus::Ok);
    let api_pos = sub
        .evidence
        .iter()
        .position(|e| e.relation == "used_by" && e.file == "src/requests/api.py")
        .expect("src/requests/api.py must appear in Session subgraph evidence");
    let direct_pos = sub
        .evidence
        .iter()
        .position(|e| e.relation == "used_by" && e.file == "src/requests/direct.py")
        .expect("src/requests/direct.py must appear in Session subgraph evidence");
    if let Some(first_test_pos) = sub.evidence.iter().position(|e| e.is_test) {
        assert!(
            api_pos < first_test_pos && direct_pos < first_test_pos,
            "production used_by ({api_pos}, {direct_pos}) must rank ahead of test evidence ({first_test_pos})"
        );
    }
    if let Some(first_contains_pos) = sub.evidence.iter().position(|e| e.relation == "contains") {
        assert!(
            api_pos < first_contains_pos && direct_pos < first_contains_pos,
            "used_by ({api_pos}, {direct_pos}) must rank ahead of contains ({first_contains_pos})"
        );
    }
}
