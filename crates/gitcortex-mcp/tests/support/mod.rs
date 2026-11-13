//! Store-backed lifecycle fixtures. Every repository owns a distinct store/lock.
#![allow(dead_code)]
use gitcortex_core::{graph::Node, store::GraphStore};
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::{branch::storage_repo_id, kuzu::KuzuGraphStore};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    path::Path,
    process::Command,
};

// Every open Kuzu DB reserves a large virtual address-space range, so parallel
// fixtures exhaust it. A TestRepo holds this lock until dropped. Not reentrant:
// never call TestRepo::new while one is alive on the same thread (deadlock);
// use snapshot_copy, which takes no lock.
static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub struct TestRepo(
    tempfile::TempDir,
    Option<std::sync::MutexGuard<'static, ()>>,
);
// Git reports canonical paths (/private/var on macOS) and the store id hashes
// the path string, so fixtures must live at their canonical path for an
// external `gcx` process to resolve the same store.
fn canonical_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}
impl Default for TestRepo {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for TestRepo {
    fn drop(&mut self) {
        // Best effort: a leftover store only wastes disk under target/.
        let _ = std::fs::remove_dir_all(gitcortex_store::branch::data_dir(&storage_repo_id(
            self.root(),
        )));
    }
}
impl TestRepo {
    pub fn new() -> Self {
        let repo = Self(
            canonical_tempdir(),
            Some(FIXTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner())),
        );
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Trust Test"]);
        repo.git(&["config", "user.email", "trust@example.test"]);
        repo.git(&["config", "core.hooksPath", "/dev/null"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }
    pub fn root(&self) -> &Path {
        self.0.path()
    }
    pub fn write(&self, rel: &str, contents: &str) {
        let path = self.root().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    pub fn remove(&self, rel: &str) {
        std::fs::remove_file(self.root().join(rel)).unwrap();
    }
    pub fn rename(&self, from: &str, to: &str) {
        std::fs::rename(self.root().join(from), self.root().join(to)).unwrap();
    }
    pub fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.root())
            .env(
                "GCX_STORE_PATH",
                std::env::var_os("GCX_STORE_PATH").unwrap(),
            )
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
    pub fn commit(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "--allow-empty", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }
    pub fn snapshot_copy(&self) -> Self {
        fn copy(from: &Path, to: &Path) {
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let dst = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    std::fs::create_dir(&dst).unwrap();
                    copy(&entry.path(), &dst);
                } else if entry.file_type().unwrap().is_symlink() {
                    std::os::unix::fs::symlink(std::fs::read_link(entry.path()).unwrap(), dst)
                        .unwrap();
                } else {
                    std::fs::copy(entry.path(), dst).unwrap();
                }
            }
        }
        let repo = Self(canonical_tempdir(), None);
        copy(self.root(), repo.root());
        repo
    }
}
pub fn index_hook_path(repo: &TestRepo, store: &mut KuzuGraphStore, branch: &str) {
    let sha = store.last_indexed_sha(branch).unwrap();
    assert!(!store.working_tree_dirty(branch).unwrap());
    let (diff, head) = IncrementalIndexer::new(repo.root())
        .unwrap()
        .run(sha.as_deref())
        .unwrap();
    if !diff.is_empty() {
        let snapshots = diff.source_hashes.iter().cloned().collect();
        let verified =
            gitcortex_mcp::mcp::readiness::snapshots_match_commit(repo.root(), &head, &snapshots)
                .unwrap();
        store.set_working_tree_dirty(branch, true).unwrap();
        store.apply_diff(branch, &diff).unwrap();
        store.set_last_indexed_sha(branch, &head).unwrap();
        store.set_working_tree_dirty(branch, !verified).unwrap();
    } else {
        store.set_last_indexed_sha(branch, &head).unwrap();
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeKey {
    pub file: String,
    pub kind: String,
    pub qualified_name: String,
    pub start_line: u32,
    pub end_line: u32,
}
impl From<&Node> for NodeKey {
    fn from(n: &Node) -> Self {
        Self {
            file: n.file.to_string_lossy().into_owned(),
            kind: n.kind.to_string(),
            qualified_name: n.qualified_name.clone(),
            start_line: n.span.start_line,
            end_line: n.span.end_line,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeRecord {
    pub key: NodeKey,
    pub name: String,
    pub span: String,
    pub metadata: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Endpoint {
    Node(NodeKey),
    Unresolved(String),
    Ambiguous(String),
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeRecord {
    pub src: Endpoint,
    pub kind: String,
    pub dst: Endpoint,
    pub line: Option<u32>,
    pub confidence: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FactRecord {
    pub src: Endpoint,
    pub src_file: String,
    pub target_name: String,
    pub kind: String,
    pub line: i64,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Fingerprint {
    pub nodes: BTreeMap<NodeRecord, usize>,
    pub edges: BTreeMap<EdgeRecord, usize>,
    pub facts: BTreeMap<FactRecord, usize>,
}
impl Fingerprint {
    /// Symmetric per-record difference; `None` when equal.
    pub fn diff(&self, rebuilt: &Fingerprint) -> Option<String> {
        let mut out = Vec::new();
        diff_counts("nodes", &self.nodes, &rebuilt.nodes, &mut out);
        diff_counts("edges", &self.edges, &rebuilt.edges, &mut out);
        diff_counts("facts", &self.facts, &rebuilt.facts, &mut out);
        (!out.is_empty()).then(|| out.join("\n"))
    }
}
fn diff_counts<K: Ord + Debug>(
    label: &str,
    incremental: &BTreeMap<K, usize>,
    rebuilt: &BTreeMap<K, usize>,
    out: &mut Vec<String>,
) {
    for (key, n) in incremental {
        let m = rebuilt.get(key).copied().unwrap_or(0);
        if *n != m {
            out.push(format!(
                "{label}: only in incremental (x{n} vs x{m} rebuilt): {key:?}"
            ));
        }
    }
    for (key, m) in rebuilt {
        if !incremental.contains_key(key) {
            out.push(format!(
                "{label}: only in rebuild (x{m} vs x0 incremental): {key:?}"
            ));
        }
    }
}
pub fn fingerprint(store: &KuzuGraphStore, branch: &str) -> Fingerprint {
    let nodes = store.list_all_nodes(branch).unwrap();
    let mut ids: HashMap<String, Vec<NodeKey>> = HashMap::new();
    let mut keys: BTreeMap<NodeKey, usize> = BTreeMap::new();
    for n in &nodes {
        let key = NodeKey::from(n);
        *keys.entry(key.clone()).or_default() += 1;
        ids.entry(n.id.as_str()).or_default().push(key);
    }
    let endpoint = |id: String| match ids.get(&id) {
        None => Endpoint::Unresolved(id),
        Some(v) if v.len() == 1 && keys[&v[0]] == 1 => Endpoint::Node(v[0].clone()),
        Some(v) => Endpoint::Ambiguous(format!("{v:?}")),
    };
    let mut fp = Fingerprint {
        nodes: BTreeMap::new(),
        edges: BTreeMap::new(),
        facts: BTreeMap::new(),
    };
    for n in &nodes {
        // serde_json::Value maps sort object keys recursively by default.
        let metadata = serde_json::to_string(&serde_json::to_value(&n.metadata).unwrap()).unwrap();
        *fp.nodes
            .entry(NodeRecord {
                key: NodeKey::from(n),
                name: n.name.clone(),
                span: format!("{:?}", n.span),
                metadata,
            })
            .or_default() += 1;
    }
    for e in store.list_all_edges(branch).unwrap() {
        *fp.edges
            .entry(EdgeRecord {
                src: endpoint(e.src.as_str()),
                dst: endpoint(e.dst.as_str()),
                kind: e.kind.to_string(),
                line: e.line,
                confidence: format!("{:?}", e.confidence),
            })
            .or_default() += 1;
    }
    for (src, src_file, target_name, kind, line) in store.list_reference_facts(branch).unwrap() {
        *fp.facts
            .entry(FactRecord {
                src: endpoint(src),
                src_file,
                target_name,
                kind,
                line,
            })
            .or_default() += 1;
    }
    fp
}
pub fn assert_matches_rebuild(repo: &TestRepo, store: &KuzuGraphStore, branch: &str) {
    let copy = repo.snapshot_copy();
    let mut rebuilt = KuzuGraphStore::open(copy.root()).unwrap();
    let (diff, _) = IncrementalIndexer::new(copy.root())
        .unwrap()
        .run(None)
        .unwrap();
    rebuilt.apply_diff(branch, &diff).unwrap();
    if let Some(delta) = fingerprint(store, branch).diff(&fingerprint(&rebuilt, branch)) {
        panic!("rebuild mismatch\n{delta}");
    }
}
