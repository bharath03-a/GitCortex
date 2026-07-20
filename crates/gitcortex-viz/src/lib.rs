use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use gitcortex_core::{
    graph::{in_degree_by_calls, Node},
    schema::NodeKind,
    store::GraphStore,
};
use gitcortex_store::kuzu::KuzuGraphStore;
use serde::Deserialize;
use serde_json::{json, Value};

static VIZ_INDEX: &[u8] = include_bytes!("../dist-viz/index.html");
static VIZ_JS: &[u8] = include_bytes!("../dist-viz/assets/main.js");
static VIZ_CSS: &[u8] = include_bytes!("../dist-viz/assets/main.css");
static VIZ_WEBGL: &[u8] = include_bytes!("../dist-viz/assets/webgl-device.js");
static VIZ_COSMOS: &[u8] = include_bytes!("../dist-viz/assets/CosmosCanvas.js");

/// Output format for `gcx viz`.
#[derive(clap::ValueEnum, Clone)]
pub enum VizFormat {
    /// Open the interactive WebGL force-directed graph in the browser
    /// (Cosmograph, served from a local Axum process).
    Web,
    /// Self-contained `graph.html` printed to stdout. Uses vis-network from a
    /// CDN, embeds the graph as inline JSON. No server, can be opened offline
    /// after the first load. Pipe to a file: `gcx viz --format html > graph.html`.
    Html,
    /// Print a Graphviz DOT file to stdout.
    Dot,
    /// Print a static SVG (kind-grouped concentric layout) to stdout.
    Svg,
    /// Print a GraphML XML document to stdout (importable by Gephi, yEd, …).
    Graphml,
    /// Print Cypher `CREATE` statements to stdout, importable by Neo4j.
    Cypher,
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
        VizFormat::Html => {
            let html = build_html(&store, &branch)?;
            print!("{html}");
        }
        VizFormat::Svg => {
            let svg = build_svg(&store, &branch)?;
            print!("{svg}");
        }
        VizFormat::Graphml => {
            let xml = build_graphml(&store, &branch)?;
            print!("{xml}");
        }
        VizFormat::Cypher => {
            let cy = build_cypher(&store, &branch)?;
            print!("{cy}");
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

    // Allowed Host headers. Computed once so the middleware closure can match
    // against them without re-allocating. Includes both `127.0.0.1` and
    // `localhost` (and their bare-name forms in case a tool strips the port).
    let allowed_hosts: Arc<Vec<String>> = Arc::new(vec![
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
        "127.0.0.1".to_owned(),
        "localhost".to_owned(),
    ]);

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/assets/main.js", get(js_handler))
        .route("/assets/main.css", get(css_handler))
        .route("/assets/webgl-device.js", get(webgl_handler))
        .route("/assets/CosmosCanvas.js", get(cosmos_handler))
        .route("/data", get(data_handler))
        .route("/api/graph/manifest", get(graph_manifest_handler))
        .route("/api/graph/nodes", get(graph_nodes_handler))
        .route("/api/graph/edges", get(graph_edges_handler))
        .route("/api/symbol-context/:name", get(symbol_context_handler))
        .route("/api/callers/:name", get(callers_handler))
        .route("/api/callers-by-id/:id", get(callers_by_id_handler))
        .route("/api/neighborhood/:id", get(neighborhood_handler))
        .route("/api/branches", get(branches_handler))
        .route("/api/branch-diff", get(branch_diff_handler))
        .route("/api/unused", get(unused_handler))
        .route("/api/god_nodes", get(god_nodes_handler))
        .layer(middleware::from_fn(move |req, next| {
            let allowed = allowed_hosts.clone();
            host_header_guard(req, next, allowed)
        }))
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

// ─── Middleware: Host-header allowlist ────────────────────────────────────────
//
// Defense against DNS-rebinding. The viz server binds to 127.0.0.1 only, but a
// malicious page on `evil.com` whose DNS resolves to 127.0.0.1 can drive the
// user's browser to call our endpoints — without this check the response is
// served and the page can exfiltrate code-graph data (file paths, function
// names, source body).
//
// Reject any request whose `Host` header isn't on the allowlist. Browsers
// always send the `Host` header; `curl` users can pass `--host` if needed.

async fn host_header_guard(
    req: Request,
    next: Next,
    allowed_hosts: Arc<Vec<String>>,
) -> Result<Response, StatusCode> {
    let Some(host) = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    else {
        // Per RFC 7230 a Host header is mandatory for HTTP/1.1; drop the
        // request when it's missing or non-ASCII.
        return Err(StatusCode::BAD_REQUEST);
    };
    if !allowed_hosts.iter().any(|allowed| host == allowed) {
        tracing::warn!(host = %host, "rejected request with unrecognised Host header");
        return Err(StatusCode::FORBIDDEN);
    }
    let mut response = next.run(req).await;
    // Belt-and-suspenders: instruct user-agents not to cache responses across
    // origins. `Vary: Host` ensures a poisoned cache entry from one host is
    // not served to another.
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Host"));
    Ok(response)
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

async fn cosmos_handler() -> Response {
    static_response(VIZ_COSMOS, "application/javascript; charset=utf-8")
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
                "src":        e.src.as_str(),
                "dst":        e.dst.as_str(),
                "kind":       e.kind.to_string(),
                "confidence": e.confidence.to_string(),
            })
        })
        .collect();

    Json(json!({ "nodes": nodes_json, "edges": edges_json }))
}

// ─── /api/* ───────────────────────────────────────────────────────────────────

const DEFAULT_GRAPH_CHUNK: usize = 5_000;
const MAX_GRAPH_CHUNK: usize = 20_000;

#[derive(Debug, Deserialize)]
struct GraphBranchQuery {
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphPageQuery {
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: Option<usize>,
}

impl GraphPageQuery {
    fn page_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_GRAPH_CHUNK)
            .clamp(1, MAX_GRAPH_CHUNK)
    }
}

#[tracing::instrument(skip(state))]
async fn graph_manifest_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GraphBranchQuery>,
) -> Json<Value> {
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let s = state.clone();
    let branch_for_query = branch.clone();
    let result = run_blocking("graph_manifest_handler", move || {
        with_locked_store(&s, |store| {
            let stats = store.graph_stats(&branch_for_query)?;
            let snapshot = store.last_indexed_sha(&branch_for_query)?;
            Ok((stats, snapshot))
        })
    })
    .await;

    let (stats, snapshot) = match result {
        Ok(value) => value,
        Err(error) => return error,
    };
    Json(json!({
        "branch": branch,
        "snapshot": snapshot,
        "total_nodes": stats.total_nodes,
        "total_edges": stats.total_edges,
        "nodes_by_kind": stats.nodes_by_kind,
        "edges_by_kind": stats.edges_by_kind,
        "recommended_chunk": DEFAULT_GRAPH_CHUNK,
        "max_chunk": MAX_GRAPH_CHUNK,
    }))
}

#[tracing::instrument(skip(state), fields(offset = q.offset, limit = q.page_limit()))]
async fn graph_nodes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GraphPageQuery>,
) -> Json<Value> {
    let offset = q.offset;
    let limit = q.page_limit();
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let s = state.clone();
    let branch_for_query = branch.clone();
    let result = run_blocking("graph_nodes_handler", move || {
        with_locked_store(&s, |store| {
            let nodes = store.list_nodes_page(&branch_for_query, offset, limit)?;
            let snapshot = store.last_indexed_sha(&branch_for_query)?;
            Ok((nodes, snapshot))
        })
    })
    .await;

    let (nodes, snapshot) = match result {
        Ok(value) => value,
        Err(error) => return error,
    };
    let count = nodes.len();
    Json(json!({
        "branch": branch,
        "snapshot": snapshot,
        "offset": offset,
        "count": count,
        "next_offset": (count == limit).then_some(offset.saturating_add(count)),
        "nodes": nodes.iter().map(node_json).collect::<Vec<_>>(),
    }))
}

#[tracing::instrument(skip(state), fields(offset = q.offset, limit = q.page_limit()))]
async fn graph_edges_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GraphPageQuery>,
) -> Json<Value> {
    let offset = q.offset;
    let limit = q.page_limit();
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let s = state.clone();
    let branch_for_query = branch.clone();
    let result = run_blocking("graph_edges_handler", move || {
        with_locked_store(&s, |store| {
            let edges = store.list_edges_page(&branch_for_query, offset, limit)?;
            let snapshot = store.last_indexed_sha(&branch_for_query)?;
            Ok((edges, snapshot))
        })
    })
    .await;

    let (edges, snapshot) = match result {
        Ok(value) => value,
        Err(error) => return error,
    };
    let count = edges.len();
    let edge_json = edges
        .iter()
        .map(|edge| {
            json!({
                "src": edge.src.as_str(),
                "dst": edge.dst.as_str(),
                "kind": edge.kind.to_string(),
                "line": edge.line,
                "confidence": edge.confidence.to_string(),
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "branch": branch,
        "snapshot": snapshot,
        "offset": offset,
        "count": count,
        "next_offset": (count == limit).then_some(offset.saturating_add(count)),
        "edges": edge_json,
    }))
}

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
async fn callers_by_id_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<DepthQuery>,
) -> Json<Value> {
    const MAX_AFFECTED: usize = 500;
    let depth = q.depth.unwrap_or(2).min(5);
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let s = state.clone();
    let id_for_query = id.clone();
    let result = run_blocking("callers_by_id_handler", move || {
        with_locked_store(&s, |store| {
            let mut seen = std::collections::HashSet::from([id_for_query.clone()]);
            let mut frontier = vec![id_for_query];
            let mut hops = Vec::new();
            let mut truncated = false;

            for _ in 0..depth {
                let mut hop = Vec::new();
                let mut next = Vec::new();
                for target_id in &frontier {
                    for (caller, _) in
                        store.find_callers_by_id_with_confidence(&branch, target_id)?
                    {
                        let caller_id = caller.id.as_str();
                        if seen.insert(caller_id.clone()) {
                            next.push(caller_id);
                            hop.push(caller);
                            if seen.len().saturating_sub(1) >= MAX_AFFECTED {
                                truncated = true;
                                break;
                            }
                        }
                    }
                    if truncated {
                        break;
                    }
                }
                hops.push(hop);
                if truncated || next.is_empty() {
                    break;
                }
                frontier = next;
            }
            Ok((hops, truncated))
        })
    })
    .await;

    let (hops, truncated) = match result {
        Ok(value) => value,
        Err(error) => return error,
    };
    let total_affected: usize = hops.iter().map(Vec::len).sum();
    let risk_level = match total_affected {
        0..=2 => "LOW",
        3..=9 => "MEDIUM",
        10..=29 => "HIGH",
        _ => "CRITICAL",
    };
    let hop_json = hops
        .iter()
        .enumerate()
        .map(|(index, nodes)| {
            json!({
                "hop": index + 1,
                "nodes": nodes.iter().map(node_json).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "id": id,
        "depth": depth,
        "risk_level": risk_level,
        "truncated": truncated,
        "hops": hop_json,
    }))
}

#[derive(Debug, Deserialize)]
struct NeighborhoodQuery {
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn neighborhood_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<NeighborhoodQuery>,
) -> Json<Value> {
    const DEFAULT_LIMIT: usize = 500;
    const MAX_LIMIT: usize = 5_000;
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let direction = match q.direction.as_deref() {
        Some("in") => "in",
        Some("out") => "out",
        _ => "both",
    };
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let s = state.clone();
    let branch_for_query = branch.clone();
    let id_for_query = id.clone();
    let result = run_blocking("neighborhood_handler", move || {
        with_locked_store(&s, |store| {
            Ok(store.get_neighborhood_by_id(&branch_for_query, &id_for_query, direction, limit)?)
        })
    })
    .await;
    let subgraph = match result {
        Ok(value) => value,
        Err(error) => return error,
    };
    let edge_count = subgraph.edges.len();
    Json(json!({
        "seed_id": id,
        "branch": branch,
        "direction": direction,
        "limit": limit,
        "limit_reached": edge_count == limit,
        "nodes": subgraph.nodes.iter().map(node_json).collect::<Vec<_>>(),
        "edges": subgraph.edges.iter().map(|edge| json!({
            "src": edge.src.as_str(),
            "dst": edge.dst.as_str(),
            "kind": edge.kind.to_string(),
            "line": edge.line,
            "confidence": edge.confidence.to_string(),
        })).collect::<Vec<_>>(),
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
struct GodNodesQuery {
    /// Minimum inbound call-edges for a node to be considered a hub. Default 5.
    #[serde(default)]
    min_in_degree: Option<u32>,
    #[serde(default)]
    branch: Option<String>,
}

#[tracing::instrument(skip(state), fields(min_in_degree = ?q.min_in_degree))]
async fn god_nodes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GodNodesQuery>,
) -> Json<Value> {
    let branch = q.branch.unwrap_or_else(|| state.branch.clone());
    let min_in_degree = q.min_in_degree.unwrap_or(5);
    let s = state.clone();
    let result = run_blocking("god_nodes_handler", move || {
        with_locked_store(&s, |store| {
            let nodes = store.list_all_nodes(&branch)?;
            let edges = store.list_all_edges(&branch)?;
            let in_degree = in_degree_by_calls(&edges);
            let mut god_nodes: Vec<(&Node, u32)> = nodes
                .iter()
                .filter_map(|n| {
                    let id = n.id.as_str();
                    let deg = *in_degree.get(&id).unwrap_or(&0);
                    if deg >= min_in_degree {
                        Some((n, deg))
                    } else {
                        None
                    }
                })
                .collect();
            god_nodes.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then(a.0.qualified_name.cmp(&b.0.qualified_name))
            });
            let limit = 50usize;
            let result: Vec<Value> = god_nodes
                .iter()
                .take(limit)
                .map(|(n, deg)| {
                    let mut v = node_json(n);
                    v["in_degree"] = (*deg).into();
                    v
                })
                .collect();
            Ok(result)
        })
    })
    .await;

    let nodes = match result {
        Ok(v) => v,
        Err(j) => return j,
    };

    Json(json!({
        "count": nodes.len(),
        "nodes": nodes,
        "min_in_degree": min_in_degree,
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
        "added_edges": diff.added_edges.iter().map(|edge| json!({
            "src": edge.src.as_str(),
            "dst": edge.dst.as_str(),
            "kind": edge.kind.to_string(),
            "line": edge.line,
            "confidence": edge.confidence.to_string(),
        })).collect::<Vec<_>>(),
        "removed_edges": diff.removed_edges.iter().map(|(src, dst, kind)| json!({
            "src": src.as_str(),
            "dst": dst.as_str(),
            "kind": kind.to_string(),
        })).collect::<Vec<_>>(),
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
        "section" => NodeKind::Section,
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

// ─── HTML (vis-network, self-contained) ───────────────────────────────────────
//
// Emits a single HTML document that loads vis-network from a CDN, embeds the
// full graph as inline JSON, and renders a force-directed layout in the
// browser. No server, no build step. Pipe to a file:
//   gcx viz --format html > graph.html
// Then double-click `graph.html`. After the first load vis-network caches
// in the browser, so subsequent opens work offline too.
fn build_html(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;

    let nodes_json: Vec<Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "id":    n.id.as_str(),
                "label": format!("{}\n{}", n.name, n.kind),
                "title": format!(
                    "{} ({})\\n{}:{}\\nkind: {}",
                    n.name, n.qualified_name, n.file.display(), n.span.start_line, n.kind
                ),
                "color": kind_dot_color(&n.kind),
                "group": n.kind.to_string(),
                "shape": "box",
            })
        })
        .collect();

    let edges_json: Vec<Value> = edges
        .iter()
        .map(|e| {
            json!({
                "from":  e.src.as_str(),
                "to":    e.dst.as_str(),
                "label": e.kind.to_string(),
                "arrows": "to",
            })
        })
        .collect();

    let payload = json!({ "nodes": nodes_json, "edges": edges_json });
    let payload_str = escape_script_payload(&serde_json::to_string(&payload)?);
    let branch_esc = svg_escape(branch);
    let total_nodes = nodes.len();
    let total_edges = edges.len();

    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>GitCortex graph — {branch_esc}</title>
<script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
<style>
  html, body {{ margin: 0; padding: 0; height: 100%; background: #1e1e2e; color: #cdd6f4; font-family: -apple-system, sans-serif; }}
  #header {{ padding: 10px 16px; background: #181825; border-bottom: 1px solid #313244; font-size: 13px; }}
  #header strong {{ color: #89b4fa; }}
  #header input {{ background: #313244; color: #cdd6f4; border: 1px solid #45475a; padding: 4px 8px; border-radius: 4px; margin-left: 12px; }}
  #net {{ width: 100vw; height: calc(100vh - 48px); }}
</style>
</head>
<body>
<div id="header">
  <strong>GitCortex</strong> · branch <code>{branch_esc}</code> · {total_nodes} nodes, {total_edges} edges
  <input id="q" type="search" placeholder="search by name…">
</div>
<div id="net"></div>
<script>
  const DATA = {payload_str};
  const nodes = new vis.DataSet(DATA.nodes);
  const edges = new vis.DataSet(DATA.edges);
  const net = new vis.Network(document.getElementById('net'), {{ nodes, edges }}, {{
    nodes: {{ font: {{ color: '#1e1e2e', face: 'monospace', size: 12 }} }},
    edges: {{ color: {{ color: '#6c7086', highlight: '#f5c2e7' }}, font: {{ color: '#bac2de', size: 10, strokeWidth: 0, background: 'rgba(30,30,46,0.7)' }}, smooth: false }},
    physics: {{ stabilization: {{ iterations: 200 }}, barnesHut: {{ gravitationalConstant: -8000, springLength: 120 }} }},
    interaction: {{ hover: true, tooltipDelay: 200 }}
  }});
  document.getElementById('q').addEventListener('input', (e) => {{
    const term = e.target.value.toLowerCase();
    if (!term) {{ net.unselectAll(); return; }}
    const hits = DATA.nodes.filter(n => (n.label || '').toLowerCase().includes(term)).map(n => n.id);
    net.selectNodes(hits);
    if (hits.length) net.focus(hits[0], {{ animation: true, scale: 0.8 }});
  }});
</script>
</body>
</html>"#
    ))
}

// ─── SVG (kind-grouped concentric layout) ─────────────────────────────────────
//
// Simple deterministic layout: group nodes by NodeKind, lay each group on a
// concentric ring, edges drawn as straight lines. Not pretty for huge graphs,
// but readable for small/medium repos and useful for embedding in docs.
fn build_svg(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    use std::collections::HashMap;

    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;

    let mut by_kind: HashMap<String, Vec<&Node>> = HashMap::new();
    for n in &nodes {
        by_kind.entry(n.kind.to_string()).or_default().push(n);
    }
    let mut kinds: Vec<String> = by_kind.keys().cloned().collect();
    kinds.sort();

    let cx = 600.0_f64;
    let cy = 600.0_f64;
    let ring_base = 80.0_f64;
    let ring_step = 95.0_f64;

    let mut pos: HashMap<String, (f64, f64)> = HashMap::new();
    let mut svg = String::new();
    let branch_esc = svg_escape(branch);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1200 1200\" font-family=\"monospace\" font-size=\"9\">\n  <rect width=\"1200\" height=\"1200\" fill=\"#1e1e2e\"/>\n  <text x=\"20\" y=\"24\" fill=\"#cdd6f4\">GitCortex · branch {branch_esc} · {n} nodes · {e} edges</text>\n",
        n = nodes.len(),
        e = edges.len()
    ));

    for (i, kind) in kinds.iter().enumerate() {
        let radius = ring_base + ring_step * (i as f64);
        let group = &by_kind[kind];
        let n = group.len() as f64;
        for (j, node) in group.iter().enumerate() {
            let theta = (j as f64) * std::f64::consts::TAU / n.max(1.0);
            let x = cx + radius * theta.cos();
            let y = cy + radius * theta.sin();
            pos.insert(node.id.as_str(), (x, y));
        }
    }

    // Draw edges first (behind nodes).
    for e in &edges {
        let src = e.src.as_str();
        let dst = e.dst.as_str();
        let (Some(&(x1, y1)), Some(&(x2, y2))) = (pos.get(&src), pos.get(&dst)) else {
            continue;
        };
        svg.push_str(&format!(
            "  <line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"#45475a\" stroke-width=\"0.5\" opacity=\"0.5\"/>\n"
        ));
    }

    // Draw nodes + labels.
    for n in &nodes {
        let nid = n.id.as_str();
        let Some(&(x, y)) = pos.get(&nid) else {
            continue;
        };
        let color = kind_dot_color(&n.kind);
        let name = svg_escape(&n.name);
        svg.push_str(&format!(
            "  <circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"3.5\" fill=\"{color}\" stroke=\"#11111b\" stroke-width=\"0.5\"/>\n  <text x=\"{tx:.1}\" y=\"{ty:.1}\" fill=\"#cdd6f4\">{name}</text>\n",
            tx = x + 5.0,
            ty = y + 3.0
        ));
    }

    svg.push_str("</svg>\n");
    Ok(svg)
}

fn svg_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape `</` inside a JSON string about to be embedded in a `<script>`
/// block. `serde_json::to_string` does not escape this sequence, so a node
/// name/file path containing the literal substring `</script>` (sourced
/// from a cloned, potentially untrusted repo) would close the tag early and
/// inject arbitrary HTML/JS into the exported file.
fn escape_script_payload(s: &str) -> String {
    s.replace("</", "<\\/")
}

// ─── GraphML (Gephi / yEd / Cytoscape) ────────────────────────────────────────
fn build_graphml(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
    out.push_str("  <key id=\"name\" for=\"node\" attr.name=\"name\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"file\" for=\"node\" attr.name=\"file\" attr.type=\"string\"/>\n");
    out.push_str(
        "  <key id=\"qname\" for=\"node\" attr.name=\"qualified_name\" attr.type=\"string\"/>\n",
    );
    out.push_str("  <key id=\"line\" for=\"node\" attr.name=\"start_line\" attr.type=\"int\"/>\n");
    out.push_str("  <key id=\"ekind\" for=\"edge\" attr.name=\"kind\" attr.type=\"string\"/>\n");
    out.push_str(&format!(
        "  <graph id=\"gitcortex-{}\" edgedefault=\"directed\">\n",
        svg_escape(branch)
    ));
    for n in &nodes {
        out.push_str(&format!(
            "    <node id=\"{id}\">\n      <data key=\"name\">{name}</data>\n      <data key=\"kind\">{kind}</data>\n      <data key=\"file\">{file}</data>\n      <data key=\"qname\">{qname}</data>\n      <data key=\"line\">{line}</data>\n    </node>\n",
            id = svg_escape(&n.id.as_str()),
            name = svg_escape(&n.name),
            kind = n.kind,
            file = svg_escape(&n.file.display().to_string()),
            qname = svg_escape(&n.qualified_name),
            line = n.span.start_line
        ));
    }
    for (i, e) in edges.iter().enumerate() {
        out.push_str(&format!(
            "    <edge id=\"e{i}\" source=\"{src}\" target=\"{dst}\">\n      <data key=\"ekind\">{kind}</data>\n    </edge>\n",
            src = svg_escape(&e.src.as_str()),
            dst = svg_escape(&e.dst.as_str()),
            kind = e.kind
        ));
    }
    out.push_str("  </graph>\n</graphml>\n");
    Ok(out)
}

// ─── Cypher (Neo4j bulk import) ───────────────────────────────────────────────
//
// Emits `CREATE` statements with a single :Symbol label and properties.
// Edges keep their EdgeKind as the relationship type. Pipe to `cypher-shell`
// or paste into the Neo4j browser. Keeping each statement on its own line so
// large dumps can be sliced with `split -l`.
fn build_cypher(store: &KuzuGraphStore, branch: &str) -> Result<String> {
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;
    let mut out = String::new();
    out.push_str(&format!(
        "// GitCortex Cypher export — branch {branch} — {} nodes, {} edges\n",
        nodes.len(),
        edges.len()
    ));
    for n in &nodes {
        out.push_str(&format!(
            "CREATE (`{id}`:Symbol {{name: '{name}', kind: '{kind}', file: '{file}', qualified_name: '{qname}', start_line: {line}}});\n",
            id = cypher_id(&n.id.as_str()),
            name = cypher_str(&n.name),
            kind = n.kind,
            file = cypher_str(&n.file.display().to_string()),
            qname = cypher_str(&n.qualified_name),
            line = n.span.start_line
        ));
    }
    for e in &edges {
        // Map to ALL_CAPS relationship types per Neo4j convention.
        let rel = e.kind.to_string().to_uppercase();
        out.push_str(&format!(
            "MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`{src}`) AND id(b) = id(`{dst}`) CREATE (a)-[:{rel}]->(b);\n",
            src = cypher_id(&e.src.as_str()),
            dst = cypher_id(&e.dst.as_str())
        ));
    }
    Ok(out)
}

fn cypher_id(s: &str) -> String {
    // Backticked node-variable name: strip anything not [A-Za-z0-9_].
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn cypher_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
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
        NodeKind::Section => "#f5c2e7",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_page_limits_are_bounded() {
        let query = GraphPageQuery {
            branch: None,
            offset: 0,
            limit: None,
        };
        assert_eq!(query.page_limit(), DEFAULT_GRAPH_CHUNK);

        let too_large = GraphPageQuery {
            branch: None,
            offset: 0,
            limit: Some(usize::MAX),
        };
        assert_eq!(too_large.page_limit(), MAX_GRAPH_CHUNK);

        let zero = GraphPageQuery {
            branch: None,
            offset: 0,
            limit: Some(0),
        };
        assert_eq!(zero.page_limit(), 1);
    }

    #[test]
    fn escape_script_payload_breaks_closing_tag() {
        let malicious = r#"{"name":"</script><script>alert(1)</script>"}"#;
        let escaped = escape_script_payload(malicious);
        assert!(
            !escaped.contains("</script>"),
            "escaped payload still contains an unescaped closing script tag: {escaped}"
        );
        assert!(escaped.contains("<\\/script>"));
    }

    #[test]
    fn escape_script_payload_is_noop_without_slash_sequence() {
        let benign = r#"{"name":"validate_token"}"#;
        assert_eq!(escape_script_payload(benign), benign);
    }

    #[test]
    fn svg_escape_covers_html_entities() {
        let s = svg_escape(r#"<script>&"'</script>"#);
        assert_eq!(s, "&lt;script&gt;&amp;&quot;&#39;&lt;/script&gt;");
    }

    #[test]
    fn cypher_str_escapes_quotes_and_backslashes() {
        // Regression test locking in already-correct Cypher injection defenses.
        let malicious = "O'Brien'; DROP TABLE--\\";
        let escaped = cypher_str(malicious);
        assert_eq!(escaped, "O\\'Brien\\'; DROP TABLE--\\\\");
    }

    #[test]
    fn cypher_id_strips_non_alphanumeric() {
        let id = cypher_id("abc-123'; DROP TABLE--");
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }
}
