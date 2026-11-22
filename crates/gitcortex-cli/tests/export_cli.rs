use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use serde_json::Value;

const GCX: &str = env!("CARGO_BIN_EXE_gcx");

struct Fixture {
    repo: tempfile::TempDir,
    data: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let repo = tempfile::tempdir().expect("repo tempdir");
        let data = tempfile::tempdir().expect("data tempdir");
        let home = tempfile::tempdir().expect("home tempdir");

        let f = Self { repo, data, home };
        f.git(&["init", "-b", "main"]);
        f.write_file(
            "src/chain.rs",
            r#"
pub fn entry() {
    middle();
}

pub fn middle() {
    leaf();
}

pub fn leaf() {}
"#,
        );
        f.write_file("src/mod_a.rs", "pub fn dup() {}\n");
        f.write_file("src/mod_b.rs", "pub fn dup() {}\n");
        f.git(&["add", "."]);
        f.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "initial",
        ]);
        f
    }

    fn write_file(&self, rel: &str, contents: &str) {
        let full = self.repo.path().join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(full, contents).expect("write file");
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.repo.path())
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(GCX)
            .args(args)
            .current_dir(self.repo.path())
            .env("GCX_STORE_PATH", self.data.path())
            .env("HOME", self.home.path())
            .output()
            .expect("run gcx")
    }

    fn init(&self) {
        let out = self.run(&["init", "--editor", "none"]);
        assert!(out.status.success(), "init failed: {}", stderr(&out));
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn node_names(doc: &Value) -> Vec<String> {
    doc["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .map(|n| n["name"].as_str().expect("name").to_owned())
        .collect()
}

#[test]
fn graph_json_full_export() {
    let f = Fixture::new();
    f.init();

    let out = f.run(&["export", "--format", "graph-json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let doc: Value = serde_json::from_str(&stdout(&out)).expect("valid json");
    assert_eq!(doc["format"], "gitcortex.graph");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["meta"]["branch"], "main");
    assert_eq!(doc["meta"]["readiness"], "ready");
    assert_eq!(doc["meta"]["truncated"], false);

    let names = node_names(&doc);
    for expected in ["entry", "middle", "leaf", "dup"] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing {expected}: {names:?}"
        );
    }
}

#[test]
fn seed_down_depth1_only_callees() {
    let f = Fixture::new();
    f.init();

    let out = f.run(&[
        "export",
        "--format",
        "graph-json",
        "--seed",
        "entry",
        "--direction",
        "down",
        "--depth",
        "1",
        "--edge-kinds",
        "calls",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));

    let doc: Value = serde_json::from_str(&stdout(&out)).expect("valid json");
    let names = node_names(&doc);
    assert!(names.contains(&"entry".to_owned()), "{names:?}");
    assert!(names.contains(&"middle".to_owned()), "{names:?}");
    assert!(!names.contains(&"leaf".to_owned()), "{names:?}");
}

#[test]
fn seed_up_lists_what_leads_to_it() {
    let f = Fixture::new();
    f.init();

    let out = f.run(&[
        "export",
        "--format",
        "graph-json",
        "--seed",
        "leaf",
        "--direction",
        "up",
        "--depth",
        "2",
        "--edge-kinds",
        "calls",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));

    let doc: Value = serde_json::from_str(&stdout(&out)).expect("valid json");
    let names = node_names(&doc);
    assert!(names.contains(&"leaf".to_owned()), "{names:?}");
    assert!(names.contains(&"middle".to_owned()), "{names:?}");
    assert!(names.contains(&"entry".to_owned()), "{names:?}");
}

#[test]
fn ambiguous_seed_lists_candidates() {
    let f = Fixture::new();
    f.init();

    let out = f.run(&["export", "--format", "graph-json", "--seed", "dup"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));

    let err = stderr(&out);
    assert!(err.contains("ambiguous seed"), "{err}");
    let candidate_lines = err
        .lines()
        .filter(|line| line.contains("dup") && line.contains('('))
        .count();
    assert!(
        candidate_lines >= 2,
        "expected >= 2 candidate lines, got:\n{err}"
    );
}

#[test]
fn unknown_seed_exit_2() {
    let f = Fixture::new();
    f.init();

    let out = f.run(&[
        "export",
        "--format",
        "graph-json",
        "--seed",
        "definitely_not_a_symbol",
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("seed not found"), "{}", stderr(&out));
}

#[test]
fn invalid_branch_ref_exit_2() {
    let f = Fixture::new();
    let out = f.run(&[
        "export",
        "--format",
        "graph-json",
        "--branch",
        "no-such-branch",
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert_eq!(
        fs::read_dir(f.data.path()).expect("read data dir").count(),
        0,
        "invalid branch ref should not touch or create store"
    );
}

#[test]
fn missing_index_exits_3() {
    let f = Fixture::new();
    let out = f.run(&["export", "--format", "graph-json"]);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert!(stderr(&out).contains("missing"), "{}", stderr(&out));
    assert_eq!(
        fs::read_dir(f.data.path()).expect("read data dir").count(),
        0,
        "missing index check should not create an empty store"
    );
}

#[test]
fn unrelated_error_exits_1() {
    let f = Fixture::new();
    f.init();

    // Corrupt the repo's graph.kuzu file so opening the store fails.
    let entries: Vec<_> = fs::read_dir(f.data.path())
        .expect("read data dir")
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "expected initialized store dir");
    let db = entries[0].path().join("graph.kuzu");
    assert!(db.exists(), "expected graph.kuzu at {}", db.display());
    fs::write(&db, b"corrupted-not-a-kuzu-db").expect("corrupt graph.kuzu");

    let out = f.run(&["export", "--format", "graph-json"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
}

#[test]
fn allow_stale_exports_with_meta_state() {
    let f = Fixture::new();
    f.init();

    // Commit a change without running gcx hook so the index becomes stale.
    f.write_file("src/extra.rs", "pub fn newer() {}\n");
    f.git(&["add", "src/extra.rs"]);
    f.git(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "-m",
        "advance head without reindex",
    ]);

    let blocked = f.run(&["export", "--format", "graph-json"]);
    assert_eq!(blocked.status.code(), Some(3), "{}", stderr(&blocked));
    assert!(stderr(&blocked).contains("stale"), "{}", stderr(&blocked));

    let allowed = f.run(&["export", "--format", "graph-json", "--allow-stale"]);
    assert!(allowed.status.success(), "{}", stderr(&allowed));
    let doc: Value = serde_json::from_str(&stdout(&allowed)).expect("valid json");
    assert_eq!(doc["meta"]["readiness"], "stale");
    assert!(stderr(&allowed).contains("stale"), "{}", stderr(&allowed));
}

#[test]
fn legacy_json_unchanged() {
    let f = Fixture::new();
    f.init();

    let out = f.run(&["export", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let doc: Value = serde_json::from_str(&stdout(&out)).expect("valid json");
    let obj = doc.as_object().expect("json object");
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "branch",
            "edge_count",
            "edges",
            "sha",
            "symbol_count",
            "symbols"
        ]
    );
    assert_eq!(doc["branch"], "main");
    assert!(doc["symbol_count"].as_u64().unwrap() >= 5);
    assert!(doc["edges"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| { e.get("src").is_some() && e.get("dst").is_some() && e.get("kind").is_some() }));
}

#[test]
fn out_flag_writes_file_to_disk() {
    let f = Fixture::new();
    f.init();

    let out_dir = tempfile::tempdir().expect("out tempdir");
    let target = out_dir.path().join("nested/graph.json");
    let target_str = target.to_str().expect("utf8 path");

    let out = f.run(&["export", "--format", "graph-json", "--out", target_str]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).trim().is_empty(),
        "expected empty stdout with --out"
    );
    assert!(Path::new(&target).exists());
    let doc: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(doc["format"], "gitcortex.graph");
}
