use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::style::{header_style, hint_style, kind_style_from_str, name_style, paint};

pub fn run(branch: Option<String>) -> Result<()> {
    let repo_root = repo_root()?;
    let branch = match branch {
        Some(b) => b,
        None => current_branch(&repo_root)?,
    };

    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    let last_sha = store
        .last_indexed_sha(&branch)?
        .unwrap_or_else(|| "none".into());
    let nodes = store.list_all_nodes(&branch).unwrap_or_default();
    let edges = store.list_all_edges(&branch).unwrap_or_default();

    // Count by kind and edge kind.
    let mut by_kind: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for n in &nodes {
        *by_kind.entry(n.kind.to_string()).or_default() += 1;
    }
    let mut by_edge: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &edges {
        *by_edge.entry(e.kind.to_string()).or_default() += 1;
    }

    println!(
        "{}     {}",
        paint(header_style(), "branch:"),
        paint(name_style(), &branch)
    );
    println!(
        "{}   {}",
        paint(header_style(), "last sha:"),
        paint(hint_style(), &last_sha)
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

    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    Ok(PathBuf::from(
        String::from_utf8(output.stdout)?.trim().to_owned(),
    ))
}

fn current_branch(repo_root: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        .output()
        .context("git symbolic-ref failed")?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    } else {
        let sha = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(repo_root)
            .output()?;
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}
