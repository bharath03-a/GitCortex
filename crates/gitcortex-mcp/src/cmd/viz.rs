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

use crate::VizFormat;

static VIZ_INDEX: &[u8] = include_bytes!("../../dist-viz/index.html");
static VIZ_JS: &[u8] = include_bytes!("../../dist-viz/assets/main.js");
static VIZ_CSS: &[u8] = include_bytes!("../../dist-viz/assets/main.css");
static VIZ_WEBGL: &[u8] = include_bytes!("../../dist-viz/assets/webgl-device.js");

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

            let rt = tokio::runtime::Builder::new_current_thread()
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

    eprintln!("GitCortex Viz → {url}  (Ctrl-C to stop)");
    let _ = open::that(&url);

    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}

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

async fn data_handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let branch = &state.branch;
    let store = match state.store.lock() {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "store mutex poisoned"})),
    };

    let nodes = store.list_all_nodes(branch).unwrap_or_else(|e| {
        tracing::warn!("list_all_nodes error: {e:#}");
        vec![]
    });
    let edges = store.list_all_edges(branch).unwrap_or_default();

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

#[derive(Debug, Deserialize)]
struct DepthQuery {
    #[serde(default)]
    depth: Option<u8>,
    #[serde(default)]
    branch: Option<String>,
}

async fn symbol_context_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DepthQuery>,
) -> Json<Value> {
    let branch = q.branch.as_deref().unwrap_or(&state.branch).to_owned();
    let store = match state.store.lock() {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "store mutex poisoned"})),
    };
    let ctx = match store.symbol_context(&branch, &name) {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": format!("{e:#}")})),
    };
    Json(json!({
        "definition": node_json(&ctx.definition),
        "callers":    ctx.callers.iter().map(node_json).collect::<Vec<_>>(),
        "callees":    ctx.callees.iter().map(node_json).collect::<Vec<_>>(),
        "used_by":    ctx.used_by.iter().map(node_json).collect::<Vec<_>>(),
    }))
}

async fn callers_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DepthQuery>,
) -> Json<Value> {
    let depth = q.depth.unwrap_or(2).min(5);
    let branch = q.branch.as_deref().unwrap_or(&state.branch).to_owned();
    let store = match state.store.lock() {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "store mutex poisoned"})),
    };
    let result = match store.find_callers_deep(&branch, &name, depth) {
        Ok(r) => r,
        Err(e) => return Json(json!({"error": format!("{e:#}")})),
    };
    let hops: Vec<Value> = result
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
        "risk_level": result.risk_level,
        "hops":       hops,
    }))
}

async fn branches_handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let active = state.branch.clone();
    let branches = list_local_branches().unwrap_or_default();
    let last_sha = state
        .store
        .lock()
        .ok()
        .and_then(|s| s.last_indexed_sha(&active).ok().flatten());
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

async fn unused_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UnusedQuery>,
) -> Json<Value> {
    let branch = q.branch.as_deref().unwrap_or(&state.branch).to_owned();
    let kind = q.kind.as_deref().and_then(parse_node_kind);
    let store = match state.store.lock() {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "store mutex poisoned"})),
    };
    let nodes = match store.find_unused_symbols(&branch, kind) {
        Ok(ns) => ns,
        Err(e) => return Json(json!({"error": format!("{e:#}")})),
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

async fn branch_diff_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BranchDiffQuery>,
) -> Json<Value> {
    let store = match state.store.lock() {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "store mutex poisoned"})),
    };
    let diff = match store.branch_diff(&q.base, &q.head) {
        Ok(d) => d,
        Err(e) => return Json(json!({"error": format!("{e:#}")})),
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

fn list_local_branches() -> Result<Vec<String>> {
    let out = std::process::Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
        .output()
        .context("git for-each-ref failed")?;
    let stdout = String::from_utf8(out.stdout)?;
    Ok(stdout.lines().map(|s| s.trim().to_owned()).collect())
}

// ── DOT export ────────────────────────────────────────────────────────────────

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

// ── Utility ───────────────────────────────────────────────────────────────────

fn repo_root() -> Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    Ok(PathBuf::from(
        String::from_utf8(out.stdout)?.trim().to_owned(),
    ))
}
