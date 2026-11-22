use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use gitcortex_core::{
    graph::{Edge, Node},
    schema::{EdgeKind, NodeKind, Visibility},
    store::GraphStore,
};
use gitcortex_mcp::mcp::readiness;
use gitcortex_store::{branch as store_branch, kuzu::KuzuGraphStore};
use gitcortex_viz::export::{
    resolve_scope, to_cypher, to_dot, to_graph_json, to_graphml, to_html, to_svg, Direction,
    ExportMeta, ExportScope, NodeOrder, ScopedGraph, SeedError,
};

use crate::exit::CliExit;

const DEFAULT_OUTPUT: &str = ".gitcortex/context.md";
const CLAUDE_MD: &str = "CLAUDE.md";
const SYMBOLS_BEGIN: &str = "<!-- gcx:symbols start -->";
const SYMBOLS_END: &str = "<!-- gcx:symbols end -->";

/// Output shape for `gcx export`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// Human-readable Markdown codebase map (default; writes `.gitcortex/context.md`).
    Markdown,
    /// Legacy machine-readable JSON (symbols + edges) printed to stdout.
    Json,
    /// Versioned `gitcortex.graph` v1 JSON document.
    GraphJson,
    /// Graphviz DOT format.
    Dot,
    /// Concentric-ring SVG diagram.
    Svg,
    /// GraphML XML for Gephi / yEd / Cytoscape.
    Graphml,
    /// Neo4j Cypher statements.
    Cypher,
    /// Standalone HTML vis-network graph.
    Html,
}

/// Traversal direction for scoped exports.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportDirection {
    Up,
    Down,
    Both,
}

impl From<ExportDirection> for Direction {
    fn from(value: ExportDirection) -> Self {
        match value {
            ExportDirection::Up => Direction::Up,
            ExportDirection::Down => Direction::Down,
            ExportDirection::Both => Direction::Both,
        }
    }
}

/// CLI options for `gcx export`.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub branch: Option<String>,
    pub format: ExportFormat,
    pub claude_md: bool,
    pub top: usize,
    pub seed: Option<String>,
    pub depth: u8,
    pub direction: ExportDirection,
    pub edge_kinds: Option<Vec<String>>,
    pub max_nodes: Option<usize>,
    pub out: Option<PathBuf>,
    pub allow_stale: bool,
    #[allow(dead_code)]
    pub force: bool,
}

pub fn run(opts: ExportOptions) -> Result<()> {
    let repo_root = repo_root()?;

    let branch = match opts.branch {
        Some(ref b) => {
            verify_branch_ref(&repo_root, b)?;
            b.clone()
        }
        None => current_branch(&repo_root)?,
    };

    // --claude-md overrides --format: it's an upsert into CLAUDE.md, not a dump.
    if opts.claude_md {
        let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;
        let (path, count) = write_claude_md(&repo_root, &store, &branch, opts.top)?;
        println!("upserted {count} symbols into {}", path.display());
        return Ok(());
    }

    match opts.format {
        ExportFormat::Markdown => {
            let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;
            let (path, count) = write_context(&repo_root, &store, &branch, opts.out.as_deref())?;
            println!("wrote {} ({count} definitions)", path.display());
            Ok(())
        }
        ExportFormat::Json => {
            let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;
            let json = build_symbols_json(&store, &branch)?;
            emit_output(&json, opts.out.as_deref())
        }
        ExportFormat::GraphJson
        | ExportFormat::Dot
        | ExportFormat::Svg
        | ExportFormat::Graphml
        | ExportFormat::Cypher
        | ExportFormat::Html => run_scoped_export(&repo_root, &branch, &opts),
    }
}

fn run_scoped_export(repo_root: &Path, branch: &str, opts: &ExportOptions) -> Result<()> {
    let edge_kinds = parse_edge_kinds(opts.edge_kinds.as_deref())?;
    let max_nodes = opts.max_nodes.or(Some(5000));
    let scope = ExportScope {
        seed: opts.seed.clone(),
        depth: opts.depth,
        direction: opts.direction.into(),
        edge_kinds,
        max_nodes,
        order: NodeOrder::Deterministic,
    };

    let repo_id = store_branch::storage_repo_id(repo_root);
    let data_root = store_branch::data_root();
    let data_dir = store_branch::data_dir(&repo_id);
    let db_path = store_branch::db_path(&repo_id);

    // If the configured store root or repo dir is non-directory (e.g. unwritable/corrupt
    // path), let KuzuGraphStore::open surface an operational exit-1 error rather than
    // misclassifying it as a missing index.
    let store_path_broken =
        (data_root.exists() && !data_root.is_dir()) || (data_dir.exists() && !data_dir.is_dir());

    let (graph, meta) = if !db_path.exists() && !store_path_broken {
        let next_action =
            "Run `gcx init` on the requested checkout. An empty index does not mean no dependencies.";
        if !opts.allow_stale {
            return Err(anyhow::Error::new(CliExit::NotReady).context(format!(
                "index is missing on branch '{branch}': {next_action} (pass --allow-stale to export anyway)"
            )));
        }
        eprintln!(
            "warning: exporting with index state 'missing' on branch '{branch}' ({next_action})"
        );
        if let Some(seed) = &scope.seed {
            return Err(anyhow::Error::new(SeedError::NotFound(seed.clone()))
                .context(CliExit::InvalidInput));
        }
        let g = ScopedGraph::from_explicit(branch.to_owned(), Vec::new(), Vec::new())?;
        let m = ExportMeta {
            gcx_version: env!("CARGO_PKG_VERSION").to_owned(),
            repo_name: repo_name(repo_root),
            commit: git_commit_sha(repo_root, branch),
            readiness: "missing".to_owned(),
            generated_at: utc_now_rfc3339(),
            scope,
        };
        (g, m)
    } else {
        let store = KuzuGraphStore::open(repo_root).context("failed to open graph store")?;
        let readiness = readiness::inspect(repo_root, &store, branch);
        if readiness.state != "ready" {
            if !opts.allow_stale {
                return Err(anyhow::Error::new(CliExit::NotReady).context(format!(
                    "index is {} on branch '{branch}': {} (pass --allow-stale to export anyway)",
                    readiness.state, readiness.next_action
                )));
            }
            eprintln!(
                "warning: exporting with index state '{}' on branch '{}' ({})",
                readiness.state, branch, readiness.next_action
            );
        }

        let g = resolve_scope(&store, branch, &scope).map_err(|err| {
            if err.is::<SeedError>() {
                err.context(CliExit::InvalidInput)
            } else {
                err
            }
        })?;
        let m = ExportMeta {
            gcx_version: env!("CARGO_PKG_VERSION").to_owned(),
            repo_name: repo_name(repo_root),
            commit: readiness
                .indexed_revision
                .clone()
                .or_else(|| readiness.target_revision.clone()),
            readiness: readiness.state.to_owned(),
            generated_at: utc_now_rfc3339(),
            scope,
        };
        (g, m)
    };

    let rendered = match opts.format {
        ExportFormat::GraphJson => {
            let doc = to_graph_json(&graph, &meta);
            serde_json::to_string_pretty(&doc).context("serialize graph-json")?
        }
        ExportFormat::Dot => to_dot(&graph),
        ExportFormat::Svg => to_svg(&graph),
        ExportFormat::Graphml => to_graphml(&graph),
        ExportFormat::Cypher => to_cypher(&graph),
        ExportFormat::Html => to_html(&graph),
        ExportFormat::Markdown | ExportFormat::Json => unreachable!(),
    };

    emit_output(&rendered, opts.out.as_deref())
}

fn emit_output(content: &str, out: Option<&Path>) -> Result<()> {
    if let Some(path) = out {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create parent directory for {}", path.display()))?;
            }
        }
        fs::write(path, content).with_context(|| format!("write {}", path.display()))?;
    } else {
        println!("{content}");
    }
    Ok(())
}

fn parse_edge_kinds(raw: Option<&[String]>) -> Result<Option<Vec<EdgeKind>>> {
    let Some(items) = raw else {
        return Ok(None);
    };
    let mut kinds = Vec::with_capacity(items.len());
    for item in items {
        let trimmed = item.trim().to_ascii_lowercase();
        let kind = match trimmed.as_str() {
            "contains" => EdgeKind::Contains,
            "calls" => EdgeKind::Calls,
            "implements" => EdgeKind::Implements,
            "inherits" => EdgeKind::Inherits,
            "uses" => EdgeKind::Uses,
            "imports" => EdgeKind::Imports,
            "annotated" => EdgeKind::Annotated,
            "throws" => EdgeKind::Throws,
            "references" => EdgeKind::References,
            "handled_by" | "handled-by" => EdgeKind::HandledBy,
            _ => {
                return Err(anyhow::Error::new(CliExit::InvalidInput).context(format!(
                    "unknown edge kind '{item}'; expected one of: contains, calls, implements, inherits, uses, imports, annotated, throws, references, handled_by"
                )));
            }
        };
        kinds.push(kind);
    }
    Ok(Some(kinds))
}

fn verify_branch_ref(repo_root: &Path, branch: &str) -> Result<()> {
    let out = std::process::Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{branch}^{{commit}}"),
        ])
        .current_dir(repo_root)
        .output()
        .context("git rev-parse failed")?;
    if !out.status.success() {
        return Err(anyhow::Error::new(CliExit::InvalidInput)
            .context(format!("invalid branch or revision '{branch}'")));
    }
    Ok(())
}

fn git_commit_sha(repo_root: &Path, branch: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{branch}^{{commit}}"),
        ])
        .current_dir(repo_root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn repo_name(repo_root: &Path) -> String {
    repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("repository")
        .to_owned()
}

/// Format the current UTC instant as RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`) without
/// pulling in a date/time crate.
fn utc_now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_utc(secs)
}

fn format_unix_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    // Howard Hinnant's civil_from_days algorithm (Gregorian calendar).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }

    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Regenerate context.md only if it already exists (opt-in refresh from hook).
pub fn refresh_if_exists(repo_root: &Path, store: &KuzuGraphStore, branch: &str) {
    let path = repo_root.join(DEFAULT_OUTPUT);
    if path.exists() {
        if let Err(e) = write_context(repo_root, store, branch, None) {
            tracing::warn!("context.md refresh failed: {e}");
        }
    }
}

fn write_context(
    repo_root: &Path,
    store: &KuzuGraphStore,
    branch: &str,
    out_override: Option<&Path>,
) -> Result<(PathBuf, usize)> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let sha = store
        .last_indexed_sha(branch)?
        .unwrap_or_else(|| "unknown".into());

    let def_count = nodes.iter().filter(|n| n.kind != NodeKind::File).count();
    let content = build_context_md(&nodes, &edges, branch, &sha);

    let (out_path, display_path) = match out_override {
        Some(p) => (p.to_path_buf(), p.to_path_buf()),
        None => (
            repo_root.join(DEFAULT_OUTPUT),
            PathBuf::from(DEFAULT_OUTPUT),
        ),
    };
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&out_path, content).context("write context.md")?;

    Ok((display_path, def_count))
}

// ── JSON export ──────────────────────────────────────────────────────────────

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

// ── CLAUDE.md symbol-table injection ─────────────────────────────────────────

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

// ── Markdown generation ──────────────────────────────────────────────────────

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

// ── Helpers ──────────────────────────────────────────────────────────────────

fn repo_root() -> Result<PathBuf> {
    crate::cmd::init::helpers::repo_root()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_utc_known_timestamps() {
        assert_eq!(format_unix_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_utc(1_793_836_800), "2026-11-05T00:00:00Z");
    }
}
