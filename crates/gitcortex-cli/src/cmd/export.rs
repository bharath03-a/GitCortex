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
const CLAUDE_MD: &str = "CLAUDE.md";
const SYMBOLS_BEGIN: &str = "<!-- gcx:symbols start -->";
const SYMBOLS_END: &str = "<!-- gcx:symbols end -->";

/// Output shape for `gcx export`.
#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ExportFormat {
    /// Human-readable Markdown codebase map (default; writes `.gitcortex/context.md`).
    Markdown,
    /// Machine-readable JSON (symbols + edges) printed to stdout — committable,
    /// CI-friendly, consumable without the binary or the embedded DB.
    Json,
}

pub fn run(
    branch: Option<String>,
    format: ExportFormat,
    claude_md: bool,
    top: usize,
) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    let branch = match branch {
        Some(b) => b,
        None => current_branch(&repo_root)?,
    };

    // --claude-md overrides --format: it's an upsert into CLAUDE.md, not a dump.
    if claude_md {
        let (path, count) = write_claude_md(&repo_root, &store, &branch, top)?;
        println!("upserted {count} symbols into {}", path.display());
        return Ok(());
    }

    match format {
        ExportFormat::Markdown => {
            let (path, count) = write_context(&repo_root, &store, &branch)?;
            println!("wrote {} ({count} definitions)", path.display());
        }
        ExportFormat::Json => {
            let json = build_symbols_json(&store, &branch)?;
            // Print to stdout so callers can redirect (`> symbols.json`) or pipe.
            println!("{json}");
        }
    }
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

// ── JSON export ─────────────────────────────────────────────────────────────

/// Serialize the branch graph as a single committable JSON document:
/// `{ branch, sha, symbols: [...], edges: [...] }`. File/folder structural
/// nodes are omitted from `symbols` (they aren't code symbols).
fn build_symbols_json(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    use serde_json::{json, Value};

    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let sha = store
        .last_indexed_sha(branch)?
        .unwrap_or_else(|| "unknown".into());

    let symbols: Vec<Value> = nodes
        .iter()
        .filter(|n| !matches!(n.kind, NodeKind::File | NodeKind::Folder))
        .map(|n| {
            json!({
                "id": n.id.as_str(),
                "name": n.name,
                "qualified_name": n.qualified_name,
                "kind": n.kind.to_string(),
                "file": n.file.display().to_string(),
                "line": n.span.start_line,
                "end_line": n.span.end_line,
                "visibility": vis_str(&n.metadata.visibility),
                "is_async": n.metadata.is_async,
            })
        })
        .collect();

    let edges_json: Vec<Value> = edges
        .iter()
        .map(
            |e| json!({ "src": e.src.as_str(), "dst": e.dst.as_str(), "kind": e.kind.to_string() }),
        )
        .collect();

    let doc = json!({
        "branch": branch,
        "sha": sha,
        "symbol_count": symbols.len(),
        "edge_count": edges_json.len(),
        "symbols": symbols,
        "edges": edges_json,
    });
    serde_json::to_string_pretty(&doc).context("serialize symbols json")
}

fn vis_str(v: &Visibility) -> &'static str {
    match v {
        Visibility::Pub => "pub",
        Visibility::PubCrate => "pub_crate",
        Visibility::Private => "private",
    }
}

// ── CLAUDE.md symbol-table injection ──────────────────────────────────────────

/// Upsert a compact table of the top-`limit` highest-centrality code symbols
/// into `CLAUDE.md`, bounded by `<!-- gcx:symbols start/end -->` markers. The
/// assistant then has the most-referenced symbols (name → file:line) in
/// context with zero tool calls. Idempotent: re-running replaces the block.
fn write_claude_md(
    repo_root: &Path,
    store: &KuzuGraphStore,
    branch: &str,
    limit: usize,
) -> Result<(PathBuf, usize)> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let sha = store
        .last_indexed_sha(branch)?
        .unwrap_or_else(|| "unknown".into());

    // Centrality = inbound Calls count per node id.
    let mut in_degree: HashMap<String, u32> = HashMap::new();
    for e in &edges {
        if e.kind == EdgeKind::Calls {
            *in_degree.entry(e.dst.as_str()).or_insert(0) += 1;
        }
    }

    // Rank code symbols (functions/methods/types) by centrality, then name.
    let mut ranked: Vec<&Node> = nodes
        .iter()
        .filter(|n| {
            matches!(
                n.kind,
                NodeKind::Function
                    | NodeKind::Method
                    | NodeKind::Struct
                    | NodeKind::Trait
                    | NodeKind::Interface
                    | NodeKind::Enum
            )
        })
        .collect();
    ranked.sort_by(|a, b| {
        let da = in_degree.get(&a.id.as_str()).copied().unwrap_or(0);
        let db = in_degree.get(&b.id.as_str()).copied().unwrap_or(0);
        db.cmp(&da)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });

    let mut block = String::new();
    block.push_str(SYMBOLS_BEGIN);
    block.push('\n');
    block.push_str(&format!(
        "<!-- Auto-generated by `gcx export --claude-md`. Branch `{branch}` @ `{sha}`. Do not edit by hand. -->\n",
    ));
    block.push_str("## Key symbols (GitCortex)\n\n");
    block.push_str(
        "Most-referenced symbols in this repo. For anything not listed, query the \
         GitCortex MCP tools (`lookup_symbol`, `search_code`, `wiki_symbol`) \
         instead of scanning files.\n\n",
    );
    let shown = ranked.len().min(limit);
    for n in ranked.iter().take(limit) {
        let deg = in_degree.get(&n.id.as_str()).copied().unwrap_or(0);
        block.push_str(&format!(
            "- `{}` ({}) — `{}:{}`{}\n",
            n.name,
            n.kind,
            n.file.display(),
            n.span.start_line,
            if deg > 0 {
                format!("  · {deg} refs")
            } else {
                String::new()
            },
        ));
    }
    block.push('\n');
    block.push_str(SYMBOLS_END);

    let path = repo_root.join(CLAUDE_MD);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = upsert_block(&existing, &block);
    fs::write(&path, updated).context("write CLAUDE.md")?;

    Ok((PathBuf::from(CLAUDE_MD), shown))
}

/// Replace the text between the symbol markers (inclusive) with `block`. If no
/// markers exist, append the block to the end (with a separating blank line).
fn upsert_block(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(SYMBOLS_BEGIN), existing.find(SYMBOLS_END)) {
        if end >= start {
            let end_full = end + SYMBOLS_END.len();
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..start]);
            out.push_str(block);
            out.push_str(&existing[end_full..]);
            return out;
        }
    }
    if existing.trim().is_empty() {
        format!("{block}\n")
    } else {
        format!("{}\n\n{block}\n", existing.trim_end())
    }
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
