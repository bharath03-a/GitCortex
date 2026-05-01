use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use gitcortex_core::{
    graph::{Edge, Node},
    schema::{EdgeKind, NodeKind, Visibility},
    store::GraphStore,
};
use gitcortex_store::kuzu::KuzuGraphStore;

const DEFAULT_OUTPUT: &str = ".gitcortex/context.md";

pub fn run(branch: Option<String>) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    let branch = match branch {
        Some(b) => b,
        None => current_branch(&repo_root)?,
    };

    let (path, count) = write_context(&repo_root, &store, &branch)?;
    println!("wrote {} ({count} definitions)", path.display());
    Ok(())
}

/// Regenerate context.md only if it already exists (opt-in refresh from hook).
pub fn refresh_if_exists(repo_root: &Path, store: &KuzuGraphStore, branch: &str) {
    let path = repo_root.join(DEFAULT_OUTPUT);
    if path.exists() {
        if let Err(e) = write_context(repo_root, store, branch) {
            tracing::warn!("context.md refresh failed: {e}");
        }
    }
}

fn write_context(
    repo_root: &Path,
    store: &KuzuGraphStore,
    branch: &str,
) -> Result<(PathBuf, usize)> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let sha = store
        .last_indexed_sha(branch)?
        .unwrap_or_else(|| "unknown".into());

    let def_count = nodes.iter().filter(|n| n.kind != NodeKind::File).count();
    let content = build_context_md(&nodes, &edges, branch, &sha);

    let out_path = repo_root.join(DEFAULT_OUTPUT);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, content).context("write context.md")?;

    Ok((PathBuf::from(DEFAULT_OUTPUT), def_count))
}

// ── Markdown generation ───────────────────────────────────────────────────────

fn build_context_md(nodes: &[Node], edges: &[Edge], branch: &str, sha: &str) -> String {
    // String-keyed maps to avoid NodeId lifetime complexity.
    let node_map: HashMap<String, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut child_set: HashSet<String> = HashSet::new();
    for edge in edges {
        if edge.kind == EdgeKind::Contains {
            let src = edge.src.as_str();
            let dst = edge.dst.as_str();
            children.entry(src).or_default().push(dst.clone());
            child_set.insert(dst);
        }
    }

    // Group non-File nodes by file path.
    let mut by_file: HashMap<PathBuf, Vec<&Node>> = HashMap::new();
    for node in nodes {
        if node.kind != NodeKind::File {
            by_file.entry(node.file.clone()).or_default().push(node);
        }
    }

    let mut files: Vec<PathBuf> = by_file.keys().cloned().collect();
    files.sort();

    let def_count = nodes.iter().filter(|n| n.kind != NodeKind::File).count();

    let mut out = String::new();
    out.push_str("# Codebase Map\n\n");
    out.push_str(&format!(
        "> Branch: `{branch}` · {def_count} definitions · SHA: `{sha}`\n\n"
    ));

    for file in &files {
        let file_nodes = match by_file.get(file) {
            Some(ns) => ns,
            None => continue,
        };

        out.push_str(&format!("## {}\n\n", file.display()));

        // Roots: nodes in this file that are not children of another node.
        let mut roots: Vec<&Node> = file_nodes
            .iter()
            .copied()
            .filter(|n| !child_set.contains(&n.id.as_str()))
            .collect();
        roots.sort_by_key(|n| n.span.start_line);

        for root in &roots {
            render_node(&mut out, root, &children, &node_map, 0);
        }

        out.push('\n');
    }

    out
}

fn render_node(
    out: &mut String,
    node: &Node,
    children: &HashMap<String, Vec<String>>,
    node_map: &HashMap<String, &Node>,
    depth: usize,
) {
    let indent = "  ".repeat(depth);
    let vis = match &node.metadata.visibility {
        Visibility::Pub => "pub ",
        Visibility::PubCrate => "pub(crate) ",
        Visibility::Private => "",
    };
    let async_str = if node.metadata.is_async { "async " } else { "" };
    let unsafe_str = if node.metadata.is_unsafe {
        "unsafe "
    } else {
        ""
    };

    out.push_str(&format!(
        "{indent}- `{vis}{async_str}{unsafe_str}{kind} {name}` :{line}\n",
        kind = node.kind,
        name = node.name,
        line = node.span.start_line,
    ));

    let id_str = node.id.as_str();
    if let Some(child_ids) = children.get(&id_str) {
        let mut child_nodes: Vec<&Node> = child_ids
            .iter()
            .filter_map(|id| node_map.get(id).copied())
            .collect();
        child_nodes.sort_by_key(|n| n.span.start_line);
        for child in child_nodes {
            render_node(out, child, children, node_map, depth + 1);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn repo_root() -> Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    if !out.status.success() {
        anyhow::bail!("not inside a git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8(out.stdout)?.trim().to_owned(),
    ))
}

fn current_branch(repo_root: &Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        .output()
        .context("git symbolic-ref failed")?;
    if out.status.success() {
        Ok(String::from_utf8(out.stdout)?.trim().to_owned())
    } else {
        let sha = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(repo_root)
            .output()?;
        Ok(String::from_utf8(sha.stdout)?.trim().to_owned())
    }
}
