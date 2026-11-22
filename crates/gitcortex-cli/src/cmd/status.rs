use std::path::Path;

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_store::{branch as store_branch, kuzu::KuzuGraphStore};

use crate::{
    cmd::init::helpers::{current_branch, repo_root},
    exit::CliExit,
    style::{header_style, hint_style, kind_style_from_str, name_style, paint},
};

pub fn run(branch: Option<String>) -> Result<()> {
    let repo_root = repo_root()?;
    let checkout_rev = git_rev_parse_head(&repo_root)?;

    let branch = match branch {
        Some(requested) => {
            if requested == "HEAD" && checkout_rev.is_none() {
                anyhow::bail!(
                    "no checkout commit found at HEAD (unborn branch?); create a commit and retry"
                );
            }
            verify_commit_ref(&repo_root, &requested)?;
            requested
        }
        None => {
            if checkout_rev.is_none() {
                anyhow::bail!(
                    "no checkout commit found at HEAD (unborn branch?); create a commit and retry"
                );
            }
            current_branch(&repo_root)?
        }
    };

    let repo_id = store_branch::storage_repo_id(&repo_root);
    if !store_branch::has_repo_data(&repo_id).context("failed to inspect graph store directory")? {
        return Err(anyhow::Error::new(CliExit::NotReady).context(format!(
            "index missing on branch '{branch}': Run gcx init on the requested checkout. An empty index does not mean no dependencies."
        )));
    }

    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;
    report_status(&repo_root, &store, &branch)
}

fn report_status(repo_root: &Path, store: &dyn GraphStore, branch: &str) -> Result<()> {
    let readiness = gitcortex_mcp::mcp::readiness::inspect(repo_root, store, branch);

    match readiness.state {
        "ready" => {
            let last_sha = readiness
                .indexed_revision
                .as_deref()
                .unwrap_or("unknown")
                .to_owned();
            print_counts(
                store,
                &readiness.branch,
                "ready",
                &last_sha,
                readiness.next_action,
            )?;
            Ok(())
        }
        "unverified" => {
            let last_sha = readiness
                .indexed_revision
                .as_deref()
                .unwrap_or("unknown")
                .to_owned();
            print_counts(
                store,
                &readiness.branch,
                "unverified (observed counts only; working-tree coverage is unverified)",
                &last_sha,
                readiness.next_action,
            )?;
            Ok(())
        }
        "missing" => Err(anyhow::Error::new(CliExit::NotReady).context(format!(
            "index missing on branch '{}': {}",
            readiness.branch, readiness.next_action
        ))),
        "stale" => {
            let indexed = readiness.indexed_revision.as_deref().unwrap_or("none");
            let target = readiness.target_revision.as_deref().unwrap_or("unknown");
            Err(anyhow::Error::new(CliExit::NotReady).context(format!(
                "index stale on branch '{}' (indexed {indexed}, target {target}): {}",
                readiness.branch, readiness.next_action
            )))
        }
        "indexing" | "degraded" => Err(anyhow::Error::new(CliExit::NotReady).context(format!(
            "index {} on branch '{}': {}",
            readiness.state, readiness.branch, readiness.next_action
        ))),
        _ => diagnose_unavailable(repo_root, store, &readiness),
    }
}

fn diagnose_unavailable(
    repo_root: &Path,
    store: &dyn GraphStore,
    readiness: &gitcortex_mcp::mcp::readiness::IndexReadiness,
) -> Result<()> {
    if readiness.checkout_revision.is_none() {
        anyhow::bail!(
            "no checkout commit found at HEAD (unborn branch?); create a commit and retry"
        );
    }

    store
        .last_indexed_sha(&readiness.branch)
        .context("graph store read error while checking indexed revision")?;
    store
        .working_tree_dirty(&readiness.branch)
        .context("graph store read error while checking working-tree state")?;

    if readiness.target_revision.is_none() {
        verify_commit_ref(repo_root, &readiness.branch)?;
    }

    gitcortex_mcp::mcp::readiness::changed_sources(repo_root)
        .context("working-tree status probe failed; inspect git repository health and retry")?;

    anyhow::bail!(
        "index {} on branch '{}': {}",
        readiness.state,
        readiness.branch,
        readiness.next_action
    )
}

fn print_counts(
    store: &dyn GraphStore,
    branch: &str,
    state_label: &str,
    last_sha: &str,
    next_action: &str,
) -> Result<()> {
    let nodes = store
        .list_all_nodes(branch)
        .context("graph store read error while listing nodes")?;
    let edges = store
        .list_all_edges(branch)
        .context("graph store read error while listing edges")?;

    let mut by_kind: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for n in &nodes {
        *by_kind.entry(n.kind.to_string()).or_default() += 1;
    }
    let mut by_edge: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &edges {
        *by_edge.entry(e.kind.to_string()).or_default() += 1;
    }

    println!(
        "{}      {}",
        paint(header_style(), "state:"),
        paint(name_style(), state_label)
    );
    println!(
        "{}     {}",
        paint(header_style(), "branch:"),
        paint(name_style(), branch)
    );
    println!(
        "{}   {}",
        paint(header_style(), "last sha:"),
        paint(hint_style(), last_sha)
    );
    println!(
        "{}      {}",
        paint(header_style(), "nodes:"),
        paint(name_style(), &nodes.len().to_string())
    );
    let mut kinds: Vec<_> = by_kind.iter().collect();
    kinds.sort_by_key(|(k, _)| k.as_str());
    for (k, c) in &kinds {
        // Pad raw text to a fixed width BEFORE colouring — ANSI escape
        // sequences are non-printing but still count toward `{:<12}`, so
        // colouring first breaks column alignment.
        let padded = format!("{k:<12}");
        println!(
            "  {} {}",
            paint(kind_style_from_str(k), &padded),
            paint(hint_style(), &c.to_string())
        );
    }
    println!(
        "{}      {}",
        paint(header_style(), "edges:"),
        paint(name_style(), &edges.len().to_string())
    );
    let mut ekinds: Vec<_> = by_edge.iter().collect();
    ekinds.sort_by_key(|(k, _)| k.as_str());
    for (k, c) in &ekinds {
        let padded = format!("{k:<12}");
        println!(
            "  {} {}",
            paint(hint_style(), &padded),
            paint(hint_style(), &c.to_string())
        );
    }
    println!(
        "{}       {}",
        paint(header_style(), "note:"),
        paint(hint_style(), next_action)
    );

    Ok(())
}

fn git_rev_parse_head(repo_root: &Path) -> Result<Option<String>> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repo_root)
        .output()
        .context("git rev-parse HEAD failed")?;
    if !out.status.success() {
        return Ok(None);
    }
    let sha = String::from_utf8(out.stdout)?.trim().to_owned();
    Ok((!sha.is_empty()).then_some(sha))
}

fn verify_commit_ref(repo_root: &Path, reference: &str) -> Result<()> {
    let out = std::process::Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ])
        .current_dir(repo_root)
        .output()
        .context("git rev-parse failed")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("not a git repository") || stderr.contains("unable to read") {
            anyhow::bail!("git ref probe failed for '{reference}': {}", stderr.trim());
        }
        return Err(anyhow::Error::new(CliExit::InvalidInput)
            .context(format!("invalid git ref '{reference}'")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::exit_code;
    use gitcortex_core::{
        error::GitCortexError,
        graph::{Edge, GraphDiff, Node},
        schema::NodeKind,
        store::{CallersDeep, SubGraph, SymbolContext},
    };

    struct FailingReadStore {
        fail_sha: bool,
        fail_dirty: bool,
        sha: Option<String>,
    }

    impl GraphStore for FailingReadStore {
        fn apply_diff(
            &mut self,
            _branch: &str,
            _diff: &GraphDiff,
        ) -> gitcortex_core::error::Result<()> {
            Ok(())
        }
        fn lookup_symbol(
            &self,
            _branch: &str,
            _name: &str,
            _fuzzy: bool,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn find_callers(
            &self,
            _branch: &str,
            _function_name: &str,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn find_callers_deep(
            &self,
            _branch: &str,
            _function_name: &str,
            _depth: u8,
        ) -> gitcortex_core::error::Result<CallersDeep> {
            Ok(CallersDeep {
                hops: Vec::new(),
                risk_level: "LOW",
            })
        }
        fn symbol_context(
            &self,
            _branch: &str,
            _name: &str,
        ) -> gitcortex_core::error::Result<SymbolContext> {
            Err(GitCortexError::Store("not implemented".into()))
        }
        fn list_definitions(
            &self,
            _branch: &str,
            _file: &Path,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn list_all_nodes(&self, _branch: &str) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn list_all_edges(&self, _branch: &str) -> gitcortex_core::error::Result<Vec<Edge>> {
            Ok(Vec::new())
        }
        fn branch_diff(&self, _from: &str, _to: &str) -> gitcortex_core::error::Result<GraphDiff> {
            Ok(GraphDiff::default())
        }
        fn find_callees(
            &self,
            _branch: &str,
            _function_name: &str,
            _depth: u8,
        ) -> gitcortex_core::error::Result<CallersDeep> {
            Ok(CallersDeep {
                hops: Vec::new(),
                risk_level: "LOW",
            })
        }
        fn find_implementors(
            &self,
            _branch: &str,
            _trait_or_interface_name: &str,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn trace_path(
            &self,
            _branch: &str,
            _from: &str,
            _to: &str,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn list_symbols_in_range(
            &self,
            _branch: &str,
            _file: &Path,
            _start_line: u32,
            _end_line: u32,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn find_unused_symbols(
            &self,
            _branch: &str,
            _kind: Option<NodeKind>,
        ) -> gitcortex_core::error::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        fn get_subgraph(
            &self,
            _branch: &str,
            _seed_name: &str,
            _depth: u8,
            _direction: &str,
        ) -> gitcortex_core::error::Result<SubGraph> {
            Ok(SubGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }
        fn last_indexed_sha(&self, _branch: &str) -> gitcortex_core::error::Result<Option<String>> {
            if self.fail_sha {
                Err(GitCortexError::Store("simulated sha read failure".into()))
            } else {
                Ok(self.sha.clone())
            }
        }
        fn set_last_indexed_sha(
            &mut self,
            _branch: &str,
            _sha: &str,
        ) -> gitcortex_core::error::Result<()> {
            Ok(())
        }
        fn working_tree_dirty(&self, _branch: &str) -> gitcortex_core::error::Result<bool> {
            if self.fail_dirty {
                Err(GitCortexError::Store("simulated dirty read failure".into()))
            } else {
                Ok(false)
            }
        }
    }

    fn init_temp_repo() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap();
            assert!(o.status.success(), "{o:?}");
            String::from_utf8(o.stdout).unwrap().trim().to_owned()
        };
        run(&["init", "-b", "main"]);
        std::fs::write(dir.path().join("sample.rs"), "pub fn greet() {}\n").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@t",
            "commit",
            "-m",
            "init",
        ]);
        let sha = run(&["rev-parse", "HEAD"]);
        (dir, sha)
    }

    #[test]
    fn failing_read_store_sha_error_exits_1_as_store_error() {
        let (dir, _sha) = init_temp_repo();
        let store = FailingReadStore {
            fail_sha: true,
            fail_dirty: false,
            sha: None,
        };
        let err = report_status(dir.path(), &store, "main").expect_err("must fail");
        assert_eq!(exit_code(&err), 1, "{err:#}");
        assert!(format!("{err:#}").contains("store"), "{err:#}");
    }

    #[test]
    fn failing_read_store_dirty_error_exits_1_as_store_error() {
        let (dir, sha) = init_temp_repo();
        let store = FailingReadStore {
            fail_sha: false,
            fail_dirty: true,
            sha: Some(sha),
        };
        let err = report_status(dir.path(), &store, "main").expect_err("must fail");
        assert_eq!(exit_code(&err), 1, "{err:#}");
        assert!(format!("{err:#}").contains("store"), "{err:#}");
    }
}
