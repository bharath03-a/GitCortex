use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    response::{Html, Json},
    routing::get,
    Router,
};
use gitcortex_core::{schema::NodeKind, store::GraphStore};
use gitcortex_store::kuzu::KuzuGraphStore;
use serde_json::{json, Value};

use crate::VizFormat;

static VIZ_HTML: &str = include_str!("../assets/viz.html");

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
    let url  = format!("http://{addr}");

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/data", get(data_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    eprintln!("GitCortex Viz → {url}  (Ctrl-C to stop)");
    let _ = open::that(&url);

    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}

async fn root_handler() -> Html<&'static str> {
    Html(VIZ_HTML)
}

async fn data_handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let branch = &state.branch;
    let store  = match state.store.lock() {
        Ok(s)  => s,
        Err(_) => return Json(json!({"error": "store mutex poisoned"})),
    };

    let nodes = store.list_all_nodes(branch).unwrap_or_default();
    let edges = store.list_all_edges(branch).unwrap_or_default();

    let nodes_json: Vec<Value> = nodes.iter().map(|n| json!({
        "id":             n.id.as_str(),
        "name":           n.name,
        "kind":           n.kind.to_string(),
        "file":           n.file.display().to_string(),
        "start_line":     n.span.start_line,
        "qualified_name": n.qualified_name,
    })).collect();

    let edges_json: Vec<Value> = edges.iter().map(|e| json!({
        "src":  e.src.as_str(),
        "dst":  e.dst.as_str(),
        "kind": e.kind.to_string(),
    })).collect();

    Json(json!({ "nodes": nodes_json, "edges": edges_json }))
}

// ── DOT export ────────────────────────────────────────────────────────────────

fn build_dot(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;

    let mut out = String::from(
        "digraph gitcortex {\n  rankdir=LR;\n  node [shape=box style=filled fontcolor=white fontname=monospace];\n  edge [fontname=monospace fontsize=9];\n"
    );

    for n in &nodes {
        let id    = n.id.as_str();
        let label = dot_escape(&format!("{}\\n{}", n.name, n.kind));
        let color = kind_dot_color(&n.kind);
        out.push_str(&format!("  \"{id}\" [label=\"{label}\" fillcolor=\"{color}\"];\n"));
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
        NodeKind::File       => "#6c7086",
        NodeKind::Module     => "#cba6f7",
        NodeKind::Struct     => "#a6e3a1",
        NodeKind::Enum       => "#94e2d5",
        NodeKind::Trait      => "#fab387",
        NodeKind::TypeAlias  => "#f38ba8",
        NodeKind::Function   => "#89b4fa",
        NodeKind::Method     => "#74c7ec",
        NodeKind::Constant   => "#f9e2af",
        NodeKind::Macro      => "#cdd6f4",
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
