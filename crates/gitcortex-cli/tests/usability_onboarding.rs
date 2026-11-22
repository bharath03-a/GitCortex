mod common;
use common::*;

#[test]
fn help_has_three_query_examples() {
    let f = Fixture::new();
    for args in [vec!["--help"], vec!["query", "--help"]] {
        let o = f.run(&args);
        let m = message(&o);
        for example in [
            "gcx query search <fragment>",
            "gcx query symbol-context <symbol>",
            "gcx query find-callers <symbol>",
        ] {
            assert!(m.contains(example), "{m}");
        }
    }
}

#[test]
fn help_does_not_open_store() {
    let f = Fixture::new();
    let o = f.run(&["--help"]);
    assert!(o.status.success());
    assert_eq!(std::fs::read_dir(f.data.path()).unwrap().count(), 0);
}

#[test]
fn init_non_tty_stays_cli_only() {
    let f = Fixture::new();
    let o = f.run(&["init"]);
    assert!(o.status.success());
    assert!(message(&o).contains("none"));
    assert!(!f.repo.path().join(".mcp.json").exists());
}

#[test]
fn init_prints_next_query_and_dirty_explanation() {
    let f = Fixture::new();
    let start = std::time::Instant::now();
    let o = f.run(&["init", "--editor", "codex"]);
    let m = message(&o);
    assert!(o.status.success(), "{m}");
    for text in [
        "gcx query search <fragment>",
        "Replace",
        "gcx doctor",
        "gcx deinit",
        "unverified",
        "review",
        "reload",
    ] {
        assert!(m.contains(text), "{m}");
    }
    let o = f.run(&["query", "symbol-context", "greet"]);
    assert!(o.status.success(), "{}", message(&o));
    assert!(message(&o).contains("sample.rs"));
    eprintln!("init to first evidence: {:?}", start.elapsed());
}

#[test]
fn repeat_init_reports_existing_hooks() {
    let f = Fixture::new();
    f.init();
    let o = f.run(&["init", "--editor", "none"]);
    assert!(
        message(&o).contains("0 added; existing hooks retained"),
        "{}",
        message(&o)
    );
}

#[test]
fn blast_help_explains_current_base() {
    let f = Fixture::new();
    let o = f.run(&["blast-radius", "--help"]);
    assert!(message(&o).contains("current checkout"));
}
