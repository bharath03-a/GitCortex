use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use gitcortex_core::{graph::Node, schema::NodeKind, store::GraphStore};
use gitcortex_store::kuzu::KuzuGraphStore;
use serde::Deserialize;
use serde_json::{json, Value};

static VIZ_INDEX: &[u8] = include_bytes!("../dist-viz/index.html");
static VIZ_JS: &[u8] = include_bytes!("../dist-viz/assets/main.js");
static VIZ_CSS: &[u8] = include_bytes!("../dist-viz/assets/main.css");
static VIZ_WEBGL: &[u8] = include_bytes!("../dist-viz/assets/webgl-device.js");

/// Output format for `gcx viz`.
#[derive(clap::ValueEnum, Clone)]
pub enum VizFormat {
    /// Open an interactive force-directed graph in the browser.
    Web,
    /// Print a Graphviz DOT file to stdout.
    Dot,
}

/// Shared app state. `KuzuGraphStore` is held behind a `std::sync::Mutex` because
/// the underlying `kuzu::Connection` is not `Send`-safe across `.await`; all access
/// is gated through `tokio::task::spawn_blocking`, so the std mutex is correct and
/// won't be held across an await point.
struct AppState {
    store: std::sync::Mutex<KuzuGraphStore>,
    branch: String,
}

pub fn run(branch: String, port: u16, format: VizFormat) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    match format {
        VizFormat::Dot => {
            let dot = build_dot(&store, &branch)?;
            print!("{dot}");
        }
        VizFormat::Web => {
            let state = Arc::new(AppState {
                store: std::sync::Mutex::new(store),
                branch,
            });

            // Multi-thread runtime so spawn_blocking has a real worker pool to fall back to.
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(serve(state, port))?;
        }
    }
    Ok(())
}

async fn serve(state: Arc<AppState>, port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let url = format!("http://{addr}");

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/assets/main.js", get(js_handler))
        .route("/assets/main.css", get(css_handler))
        .route("/assets/webgl-device.js", get(webgl_handler))
        .route("/data", get(data_handler))
        .route("/api/symbol-context/:name", get(symbol_context_handler))
        .route("/api/callers/:name", get(callers_handler))
        .route("/api/branches", get(branches_handler))
        .route("/api/branch-diff", get(branch_diff_handler))
        .route("/api/unused", get(unused_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    tracing::info!(url = %url, "GitCortex viz listening");
    eprintln!("GitCortex Viz → {url}  (Ctrl-C to stop)");
    let _ = open::that(&url);

    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}

// ─── Static asset handlers ────────────────────────────────────────────────────

async fn root_handler() -> Response {
    static_response(VIZ_INDEX, "text/html; charset=utf-8")
}

async fn js_handler() -> Response {
    static_response(VIZ_JS, "application/javascript; charset=utf-8")
}

async fn css_handler() -> Response {
    static_response(VIZ_CSS, "text/css; charset=utf-8")
}

async fn webgl_handler() -> Response {
    static_response(VIZ_WEBGL, "application/javascript; charset=utf-8")
}

fn static_response(bytes: &'static [u8], content_type: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn run_blocking<F, R>(label: &'static str, f: F) -> Result<R, Json<Value>>
where
    F: FnOnce() -> Result<R, anyhow::Error> + Send + 'static,
    R: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(e)) => {
            tracing::warn!(error = format!("{e:#}").as_str(), "{label} failed");
            Err(Json(json!({ "error": format!("{e:#}") })))
        }
        Err(e) => {
            tracing::error!(error = %e, "{label} task panicked or was cancelled");
            Err(Json(json!({ "error": "internal task error" })))
        }
    }
}

fn with_locked_store<F, R>(state: &AppState, f: F) -> Result<R, anyhow::Error>
where
    F: FnOnce(&KuzuGraphStore) -> Result<R, anyhow::Error>,
{
    let store = state
        .store
        .lock()
        .map_err(|_| anyhow::anyhow!("store mutex poisoned"))?;
    f(&store)
}

fn node_json(n: &Node) -> Value {
    json!({
        "id":             n.id.as_str(),
        "name":           n.name,
        "kind":           n.kind.to_string(),
        "file":           n.file.display().to_string(),
        "start_line":     n.span.start_line,
        "end_line":       n.span.end_line,
        "qualified_name": n.qualified_name,
        "loc":            n.metadata.loc,
        "visibility":     n.metadata.visibility.to_string(),
        "is_async":       n.metadata.is_async,
        "is_unsafe":      n.metadata.is_unsafe,
    })
}

// ─── /data ────────────────────────────────────────────────────────────────────

#[tracing::instrument(skip(state), fields(branch = %state.branch))]
async fn data_handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let s = state.clone();
    let result = run_blocking("data_handler", move || {
        with_locked_store(&s, |store| {
            let nodes = store.list_all_nodes(&s.branch)?;
            let edges = store.list_all_edges(&s.branch)?;
            Ok((nodes, edges))
        })
    })
    .await;

    let (nodes, edges) = match result {
        Ok(v) => v,
        Err(j) => return j,
    };

    let nodes_json: Vec<Value> = nodes.iter().map(node_json).collect();
    let edges_json: Vec<Value> = edges
        .iter()
        .map(|e| {
            json!({
                "src":  e.src.as_str(),
                "dst":  e.dst.as_str(),
                "kind": e.kind.to_string(),
            })
        })
        .collect();

    Json(json!({ "nodes": nodes_json, "edges": edges_json }))
}

// ─── /api/* ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DepthQuery {
    #[serde(default)]
    depth: Option<u8>,
    #[serde(default)]
    branch: Option<String>,
}

#[tracing::instrument(skip(state), fields(name = %name))]
async fn symbol_context_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DepthQuery>,
) -> Json<Value> {
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let s = state.clone();
    let name_for_closure = name.clone();
    let result = run_blocking("symbol_context_handler", move || {
        with_locked_store(&s, |store| {
            let ctx = store.symbol_context(&branch, &name_for_closure)?;
            Ok(ctx)
        })
    })
    .await;

    let ctx = match result {
        Ok(v) => v,
        Err(j) => return j,
    };

    Json(json!({
        "definition": node_json(&ctx.definition),
        "callers":    ctx.callers.iter().map(node_json).collect::<Vec<_>>(),
        "callees":    ctx.callees.iter().map(node_json).collect::<Vec<_>>(),
        "used_by":    ctx.used_by.iter().map(node_json).collect::<Vec<_>>(),
    }))
}

#[tracing::instrument(skip(state), fields(name = %name, depth = q.depth.unwrap_or(2)))]
async fn callers_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DepthQuery>,
) -> Json<Value> {
    let depth = q.depth.unwrap_or(2).min(5);
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let s = state.clone();
    let name_for_closure = name.clone();
    let result = run_blocking("callers_handler", move || {
        with_locked_store(&s, |store| {
            let r = store.find_callers_deep(&branch, &name_for_closure, depth)?;
            Ok(r)
        })
    })
    .await;

    let result_val = match result {
        Ok(v) => v,
        Err(j) => return j,
    };

    let hops: Vec<Value> = result_val
        .hops
        .iter()
        .enumerate()
        .map(|(i, ns)| {
            json!({
                "hop":   i + 1,
                "nodes": ns.iter().map(node_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    Json(json!({
        "name":       name,
        "depth":      depth,
        "risk_level": result_val.risk_level,
        "hops":       hops,
    }))
}

#[tracing::instrument(skip(state))]
async fn branches_handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let active = state.branch.clone();

    let branches = list_local_branches_async().await.unwrap_or_default();

    let s = state.clone();
    let active_for_closure = active.clone();
    let last_sha = run_blocking("branches_handler", move || {
        with_locked_store(&s, |store| Ok(store.last_indexed_sha(&active_for_closure)?))
    })
    .await
    .ok()
    .flatten();

    Json(json!({
        "active":   active,
        "branches": branches,
        "last_sha": last_sha,
    }))
}

#[derive(Debug, Deserialize)]
struct UnusedQuery {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    branch: Option<String>,
}

#[tracing::instrument(skip(state), fields(kind = ?q.kind))]
async fn unused_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UnusedQuery>,
) -> Json<Value> {
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let kind = q.kind.as_deref().and_then(parse_node_kind);
    let s = state.clone();
    let result = run_blocking("unused_handler", move || {
        with_locked_store(&s, |store| Ok(store.find_unused_symbols(&branch, kind)?))
    })
    .await;

    let nodes = match result {
        Ok(v) => v,
        Err(j) => return j,
    };

    Json(json!({
        "count": nodes.len(),
        "nodes": nodes.iter().map(node_json).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize)]
struct BranchDiffQuery {
    base: String,
    head: String,
}

#[tracing::instrument(skip(state), fields(base = %q.base, head = %q.head))]
async fn branch_diff_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BranchDiffQuery>,
) -> Json<Value> {
    let s = state.clone();
    let base = q.base.clone();
    let head = q.head.clone();
    let result = run_blocking("branch_diff_handler", move || {
        with_locked_store(&s, |store| Ok(store.branch_diff(&base, &head)?))
    })
    .await;

    let diff = match result {
        Ok(v) => v,
        Err(j) => return j,
    };

    Json(json!({
        "base":             q.base,
        "head":             q.head,
        "added_nodes":      diff.added_nodes.iter().map(node_json).collect::<Vec<_>>(),
        "removed_node_ids": diff.removed_node_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
    }))
}

fn parse_node_kind(s: &str) -> Option<NodeKind> {
    Some(match s {
        "folder" => NodeKind::Folder,
        "file" => NodeKind::File,
        "module" => NodeKind::Module,
        "struct" => NodeKind::Struct,
        "enum" => NodeKind::Enum,
        "trait" => NodeKind::Trait,
        "interface" => NodeKind::Interface,
        "type_alias" => NodeKind::TypeAlias,
        "function" => NodeKind::Function,
        "method" => NodeKind::Method,
        "property" => NodeKind::Property,
        "constant" => NodeKind::Constant,
        "macro" => NodeKind::Macro,
        "annotation" => NodeKind::Annotation,
        "enum_member" => NodeKind::EnumMember,
        _ => return None,
    })
}

async fn list_local_branches_async() -> Result<Vec<String>> {
    let out = tokio::process::Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
        .output()
        .await
        .context("git for-each-ref failed")?;
    let stdout = String::from_utf8(out.stdout)?;
    Ok(stdout.lines().map(|s| s.trim().to_owned()).collect())
}

// ─── DOT export (sync — called from `run` before tokio runtime exists) ────────

fn build_dot(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;

    let mut out = String::from(
        "digraph gitcortex {\n  rankdir=LR;\n  node [shape=box style=filled fontcolor=white fontname=monospace];\n  edge [fontname=monospace fontsize=9];\n"
    );

    for n in &nodes {
        let id = n.id.as_str();
        let label = dot_escape(&format!("{}\\n{}", n.name, n.kind));
        let color = kind_dot_color(&n.kind);
        out.push_str(&format!(
            "  \"{id}\" [label=\"{label}\" fillcolor=\"{color}\"];\n"
        ));
    }

    for e in &edges {
        let src = e.src.as_str();
        let dst = e.dst.as_str();
        let lbl = e.kind.to_string();
        out.push_str(&format!("  \"{src}\" -> \"{dst}\" [label=\"{lbl}\"];\n"));
    }

    out.push('}');
    Ok(out)
}

fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn kind_dot_color(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::Folder => "#45475a",
        NodeKind::File => "#6c7086",
        NodeKind::Module => "#cba6f7",
        NodeKind::Struct => "#a6e3a1",
        NodeKind::Enum => "#94e2d5",
        NodeKind::Trait => "#fab387",
        NodeKind::TypeAlias => "#f38ba8",
        NodeKind::Function => "#89b4fa",
        NodeKind::Method => "#74c7ec",
        NodeKind::Constant => "#f9e2af",
        NodeKind::Macro => "#cdd6f4",
        NodeKind::Interface => "#89dceb",
        NodeKind::Property => "#cba6f7",
        NodeKind::Annotation => "#eba0ac",
        NodeKind::EnumMember => "#a6d189",
    }
}

fn repo_root() -> Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    Ok(PathBuf::from(
        String::from_utf8(out.stdout)?.trim().to_owned(),
    ))
}
