use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use gitcortex_core::{
    schema::NodeKind,
    store::{AttributeFilter, GraphStore},
};
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::embeddings::{Embedder, SemanticIndex};

use super::git_helpers::{parse_diff_hunks, run_git_diff};
use super::helpers::{detect_current_branch, parse_node_kind, parse_visibility};
use super::params::*;

pub enum SemanticState {
    /// Background initialiser not done yet — search is text-only.
    Pending,
    /// Model loaded and index populated.
    Ready {
        embedder: Box<Embedder>,
        index: Box<SemanticIndex>,
    },
    /// Initialisation failed (no network, disk error, etc.) — text-only forever.
    Disabled,
}
use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Content, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        PaginatedRequestParams, PromptMessage, PromptMessageRole,
    },
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer,
};
use serde_json::json;

// ── Server ────────────────────────────────────────────────────────────────────

/// The MCP server handler. One shared `KuzuGraphStore` wrapped in `Arc<Mutex>`
/// so all handler calls can share state safely.
#[derive(Clone)]
pub struct GitCortexServer {
    store: Arc<Mutex<KuzuGraphStore>>,
    repo_root: PathBuf,
    default_branch: String,
    compact: bool,
    /// Approximate token budget for a single tool's list payload. List-returning
    /// tools truncate their items to fit this, setting `truncated: true`, so a
    /// high-fan-out symbol can never make the graph arm dump more than a grep
    /// would read. Configurable via `GCX_RESPONSE_BUDGET` (token count).
    response_budget: usize,
    /// Semantic search state. Starts as `Pending`; background task flips to
    /// `Ready` once the model is loaded and missing vectors are embedded.
    /// `Arc<Mutex<…>>` so the background task and all clone'd handler instances
    /// share the same index.
    pub semantic: Arc<Mutex<SemanticState>>,
    /// Cached staleness warning: (computed_at, warning_text). Refreshed at most
    /// once every 5 seconds so every MCP dispatch doesn't pay two subprocess forks.
    staleness_cache: Arc<Mutex<Option<(Instant, String)>>>,
}

/// Default per-tool response token budget when `GCX_RESPONSE_BUDGET` is unset.
const DEFAULT_RESPONSE_BUDGET: usize = 2000;
/// Floor so a misconfigured tiny budget still returns something useful.
const MIN_RESPONSE_BUDGET: usize = 400;

impl GitCortexServer {
    pub fn new(repo_root: &Path) -> anyhow::Result<Self> {
        Self::new_with_mode(repo_root, false)
    }

    pub fn new_with_mode(repo_root: &Path, compact: bool) -> anyhow::Result<Self> {
        let store = KuzuGraphStore::open(repo_root)?;
        let default_branch = detect_current_branch(repo_root).unwrap_or_else(|| "main".into());
        let response_budget = std::env::var("GCX_RESPONSE_BUDGET")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(DEFAULT_RESPONSE_BUDGET)
            .max(MIN_RESPONSE_BUDGET);
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            repo_root: repo_root.to_owned(),
            default_branch,
            compact,
            response_budget,
            semantic: Arc::new(Mutex::new(SemanticState::Pending)),
            staleness_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Truncate a list of JSON items to fit `response_budget`, returning the
    /// kept items and whether truncation occurred. Token size is estimated as
    /// serialized bytes / 4 (the usual rule of thumb) — cheap and good enough
    /// to bound payloads. Always keeps at least one item so a single large
    /// result is never dropped to nothing.
    fn budget_items(&self, items: Vec<serde_json::Value>) -> (Vec<serde_json::Value>, bool) {
        let budget_bytes = self.response_budget * 4;
        let mut kept: Vec<serde_json::Value> = Vec::with_capacity(items.len());
        let mut used = 0usize;
        let total = items.len();
        for item in items {
            let sz = item.to_string().len() + 2; // +2 for ", " separators
            if !kept.is_empty() && used + sz > budget_bytes {
                break;
            }
            used += sz;
            kept.push(item);
        }
        let truncated = kept.len() < total;
        (kept, truncated)
    }

    /// Return the shared arcs + branch needed by the background semantic indexer.
    pub fn semantic_context(
        &self,
    ) -> (
        Arc<Mutex<SemanticState>>,
        Arc<Mutex<KuzuGraphStore>>,
        String,
    ) {
        (
            self.semantic.clone(),
            self.store.clone(),
            self.default_branch.clone(),
        )
    }

    /// Return the shared store arc + branch needed by the file watcher.
    pub fn store_context(&self) -> (Arc<Mutex<KuzuGraphStore>>, String) {
        (self.store.clone(), self.default_branch.clone())
    }

    /// Returns a staleness warning string if the index is behind HEAD or the
    /// working tree has uncommitted edits. Empty string when index is fresh.
    /// Result is cached for 5 seconds so repeated MCP calls don't each spawn
    /// two git subprocesses.
    fn staleness_warning(&self, branch: &str) -> String {
        const CACHE_TTL_SECS: u64 = 5;

        if let Ok(cache) = self.staleness_cache.lock() {
            if let Some((computed_at, ref warn)) = *cache {
                if computed_at.elapsed().as_secs() < CACHE_TTL_SECS {
                    return warn.clone();
                }
            }
        }

        let warn = self.compute_staleness_warning(branch);

        if let Ok(mut cache) = self.staleness_cache.lock() {
            *cache = Some((Instant::now(), warn.clone()));
        }
        warn
    }

    fn compute_staleness_warning(&self, branch: &str) -> String {
        let indexed = {
            let store = match self.store.lock() {
                Ok(g) => g,
                Err(_) => return String::new(),
            };
            store.last_indexed_sha(branch).unwrap_or_else(|e| {
                tracing::warn!("staleness check: could not read last_indexed_sha: {e}");
                None
            })
        };

        let head_sha = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_root)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout)
                        .ok()
                        .map(|s| s.trim().to_owned())
                } else {
                    None
                }
            });

        let dirty = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().count())
            .unwrap_or(0);

        let behind = match (&indexed, &head_sha) {
            (Some(idx), Some(head)) => idx != head,
            (None, _) => true,
            _ => false,
        };

        if !behind && dirty == 0 {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        if behind {
            parts.push("index is behind HEAD".into());
        }
        if dirty > 0 {
            parts.push(format!("{dirty} uncommitted file(s) not yet indexed"));
        }
        format!(
            "⚠ Stale index: {} — run `gcx hook` to update.",
            parts.join("; ")
        )
    }

    fn active_tool_router(&self) -> ToolRouter<Self> {
        Self::tool_router_for_mode(self.compact)
    }

    fn tool_router_for_mode(compact: bool) -> ToolRouter<Self> {
        let mut router = Self::tool_router();
        if compact {
            for name in [
                "lookup_symbol",
                "find_callers",
                "symbol_context",
                "list_definitions",
                "branch_diff_graph",
                "detect_changes",
                "find_callees",
                "find_implementors",
                "trace_path",
                "list_symbols_in_range",
                "graph_stats",
                "ast_search",
                "type_hierarchy",
                "find_importers",
                "find_type_usages",
                "module_dependencies",
                "get_call_sites",
                "find_unused_symbols",
                "get_subgraph",
                "wiki_symbol",
                "search_code",
                "start_tour",
                "find_god_nodes",
                "find_clusters",
                "find_cycles",
                "health_report",
            ] {
                router.disable_route(name);
            }
        }
        router
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl GitCortexServer {
    /// Look up all nodes (functions, structs, traits, etc.) by name.
    #[tool(
        description = "Look up nodes in the code knowledge graph by name. Set fuzzy=true for substring matching (e.g. 'auth' finds 'validate_auth', 'auth_middleware'). Default is exact match."
    )]
    fn lookup_symbol(&self, Parameters(p): Parameters<LookupSymbolParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let fuzzy = p.fuzzy.unwrap_or(false);
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.lookup_symbol(&branch, &p.name, fuzzy) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .filter(|n| !matches!(n.kind, NodeKind::Section))
                    .map(|n| {
                        json!({
                            "id": n.id.as_str(),
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                            "end_line": n.span.end_line,
                            "visibility": format!("{:?}", n.metadata.visibility),
                            "is_async": n.metadata.is_async,
                            "is_unsafe": n.metadata.is_unsafe,
                        })
                    })
                    .collect();
                let (items, _) = self.budget_items(items);
                CallToolResult::structured(json!(items))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find all callers of a function or method, with optional multi-hop depth.
    #[tool(
        description = "Find callers of one exact function or method. Ambiguous short names \
        return qualified candidates without traversal. Evidence is confidence-ranked, \
        production-before-test, globally budgeted, and includes total coverage. depth=1 \
        (default) is direct; depth=2..5 adds ranked transitive callers."
    )]
    fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        let options = super::agent::AgentQueryOptions {
            limit: 25,
            budget_tokens: self.response_budget.min(600),
        };
        match super::agent::find_callers(
            &*store,
            &branch,
            &p.function_name,
            p.depth.unwrap_or(1),
            options,
        ) {
            Ok(response) => CallToolResult::structured(json!(response)),
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Get a 360° view of a symbol: definition, callers, callees, and type usages.
    #[tool(
        description = "Get a complete picture of a symbol in one call: where it's defined, \
        what calls it (callers), what it calls (callees), and which code references it as a type. \
        Use this instead of chaining lookup_symbol + find_callers separately."
    )]
    fn symbol_context(&self, Parameters(p): Parameters<SymbolContextParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.symbol_context(&branch, &p.name) {
            Ok(ctx) => {
                let node_json = |n: &gitcortex_core::graph::Node| {
                    json!({
                        "kind": n.kind.to_string(),
                        "name": n.name,
                        "qualified_name": n.qualified_name,
                        "file": n.file.display().to_string(),
                        "start_line": n.span.start_line,
                    })
                };
                CallToolResult::structured(json!({
                    "definition": {
                        "kind": ctx.definition.kind.to_string(),
                        "name": ctx.definition.name,
                        "qualified_name": ctx.definition.qualified_name,
                        "file": ctx.definition.file.display().to_string(),
                        "start_line": ctx.definition.span.start_line,
                        "end_line": ctx.definition.span.end_line,
                        "visibility": format!("{:?}", ctx.definition.metadata.visibility),
                        "is_async": ctx.definition.metadata.is_async,
                        "complexity": ctx.definition.metadata.lld.complexity,
                    },
                    "callers": ctx.callers.iter().map(node_json).collect::<Vec<_>>(),
                    "callees": ctx.callees.iter().map(node_json).collect::<Vec<_>>(),
                    "used_by": ctx.used_by.iter().map(node_json).collect::<Vec<_>>(),
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// List all symbols defined in a source file, ordered by line number.
    #[tool(
        description = "List all functions, structs, traits, and other definitions in a source file, ordered by line number."
    )]
    fn list_definitions(&self, Parameters(p): Parameters<ListDefinitionsParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.list_definitions(&branch, Path::new(&p.file)) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "start_line": n.span.start_line,
                            "end_line": n.span.end_line,
                            "loc": n.metadata.loc,
                            "visibility": format!("{:?}", n.metadata.visibility),
                            "is_async": n.metadata.is_async,
                        })
                    })
                    .collect();
                let (items, _) = self.budget_items(items);
                CallToolResult::structured(json!(items))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Aggregate counts for the branch's graph — orientation before exploring.
    #[tool(
        description = "Get aggregate counts for the code graph: total nodes/edges plus per-kind breakdowns (how many functions, structs, calls edges, etc). Use this first to gauge codebase size and shape before drilling into specific symbols."
    )]
    fn graph_stats(&self, Parameters(p): Parameters<GraphStatsParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.graph_stats(&branch) {
            Ok(stats) => {
                let to_obj = |pairs: &[(String, u64)]| -> serde_json::Value {
                    json!(pairs
                        .iter()
                        .map(|(k, c)| json!({ "kind": k, "count": c }))
                        .collect::<Vec<_>>())
                };
                CallToolResult::structured(json!({
                    "branch": branch,
                    "total_nodes": stats.total_nodes,
                    "total_edges": stats.total_edges,
                    "nodes_by_kind": to_obj(&stats.nodes_by_kind),
                    "edges_by_kind": to_obj(&stats.edges_by_kind),
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Structural search over node attributes (no name needed).
    #[tool(
        description = "Find symbols by structural attributes rather than name: kind (function/method/struct/...), is_async, visibility (pub/pub_crate/private), cyclomatic complexity range, and annotation/decorator (e.g. annotation='Test' finds @Test methods, 'route' finds @app.route handlers, 'derive' finds #[derive(...)]). Combine filters to answer 'all async methods', 'public structs', 'functions with complexity ≥ 10', or 'all test functions'. Optional name_contains narrows further. Default limit=30."
    )]
    fn ast_search(&self, Parameters(p): Parameters<AstSearchParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let limit = p.limit.unwrap_or(30).min(200);

        let kind = p.kind.as_deref().and_then(parse_node_kind);
        // Reject an unknown kind string rather than silently ignoring it.
        if p.kind.is_some() && kind.is_none() {
            return CallToolResult::error(vec![Content::text(format!(
                "unknown kind '{}'. Valid: function, method, struct, enum, trait, \
                 interface, type_alias, property, constant, macro, annotation, \
                 enum_member, module, file, folder",
                p.kind.as_deref().unwrap_or("")
            ))]);
        }
        let visibility = p.visibility.as_deref().and_then(parse_visibility);
        if p.visibility.is_some() && visibility.is_none() {
            return CallToolResult::error(vec![Content::text(
                "unknown visibility. Valid: pub, pub_crate, private".to_owned(),
            )]);
        }

        let filter = AttributeFilter {
            kind,
            is_async: p.is_async,
            visibility,
            min_complexity: p.min_complexity,
            max_complexity: p.max_complexity,
            name_contains: p.name_contains.clone(),
            annotation: p.annotation.clone(),
        };

        if filter.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "ast_search needs at least one filter (kind, is_async, visibility, \
                 complexity bound, name_contains, or annotation)"
                    .to_owned(),
            )]);
        }

        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.search_by_attributes(&branch, &filter, limit) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                            "visibility": format!("{:?}", n.metadata.visibility),
                            "is_async": n.metadata.is_async,
                            "complexity": n.metadata.lld.complexity,
                            "annotations": n.metadata.annotations,
                        })
                    })
                    .collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "branch": branch,
                    "results": items,
                    "returned": items.len(),
                    "truncated": truncated,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Compute the graph diff between two branches.
    #[tool(
        description = "Show what nodes were added or removed between two branches. Useful for understanding what changed in a feature branch vs main."
    )]
    fn branch_diff_graph(&self, Parameters(p): Parameters<BranchDiffParams>) -> CallToolResult {
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.branch_diff(&p.from_branch, &p.to_branch) {
            Ok(diff) => {
                let added: Vec<_> = diff
                    .added_nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();

                // Resolve removed node IDs to full node objects from the from_branch.
                let from_nodes = match store.list_all_nodes(&p.from_branch) {
                    Ok(n) => n,
                    Err(e) => {
                        return CallToolResult::error(vec![Content::text(format!(
                            "failed to list nodes on from_branch: {e}"
                        ))])
                    }
                };
                let from_map: std::collections::HashMap<_, _> =
                    from_nodes.iter().map(|n| (n.id.clone(), n)).collect();
                let removed: Vec<_> = diff
                    .removed_node_ids
                    .iter()
                    .filter_map(|id| from_map.get(id))
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();

                CallToolResult::structured(json!({
                    "from": p.from_branch,
                    "to": p.to_branch,
                    "added_nodes": added,
                    "removed_nodes": removed,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Detect which indexed symbols are affected by current staged (or HEAD) changes.
    #[tool(
        description = "Map the current git diff (staged changes, or HEAD diff if nothing is staged) \
        to the indexed symbol graph. Returns which functions/structs were changed, their direct callers, \
        and a risk level. Use this before committing to understand blast radius automatically."
    )]
    fn detect_changes(&self, Parameters(p): Parameters<DetectChangesParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();

        let diff_text = run_git_diff(&self.repo_root, &["diff", "--staged"])
            .filter(|s| !s.trim().is_empty())
            .or_else(|| run_git_diff(&self.repo_root, &["diff", "HEAD"]))
            .unwrap_or_default();

        if diff_text.trim().is_empty() {
            return CallToolResult::success(vec![Content::text(
                "No staged or unstaged changes detected.",
            )]);
        }

        let hunks = parse_diff_hunks(&diff_text);
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };

        let mut changed_symbols: Vec<serde_json::Value> = Vec::new();
        let mut total_affected: usize = 0;

        for (file_path, ranges) in &hunks {
            let path = PathBuf::from(file_path);
            let definitions = match store.list_definitions(&branch, &path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for node in &definitions {
                let overlaps = ranges
                    .iter()
                    .any(|(s, e)| node.span.start_line <= *e && node.span.end_line >= *s);
                if !overlaps {
                    continue;
                }
                let callers = store.find_callers(&branch, &node.name).unwrap_or_default();
                let caller_names: Vec<&str> = callers.iter().map(|c| c.name.as_str()).collect();
                total_affected += 1 + caller_names.len();
                changed_symbols.push(json!({
                    "kind": node.kind.to_string(),
                    "name": node.name,
                    "file": file_path,
                    "start_line": node.span.start_line,
                    "end_line": node.span.end_line,
                    "callers": caller_names,
                }));
            }
        }

        if changed_symbols.is_empty() {
            return CallToolResult::success(vec![Content::text(
                "Changed lines do not overlap with any indexed symbols.",
            )]);
        }

        let risk_level = match total_affected {
            0..=5 => "LOW",
            6..=20 => "MEDIUM",
            21..=50 => "HIGH",
            _ => "CRITICAL",
        };

        CallToolResult::structured(json!({
            "risk_level": risk_level,
            "total_affected": total_affected,
            "changed_symbols": changed_symbols,
        }))
    }

    /// Find all callees of a function/method, tracing forward through the call graph.
    #[tool(
        description = "Find all functions/methods that the named function calls. \
        Inverse of find_callers — traces forward (downstream). Use depth=1..5 to walk multiple hops. \
        Returns callees grouped by hop distance."
    )]
    fn find_callees(&self, Parameters(p): Parameters<FindCalleesParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let depth = p.depth.unwrap_or(1).max(1);
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.find_callees(&branch, &p.function_name, depth) {
            Ok(result) => {
                let hops: Vec<_> = result
                    .hops
                    .iter()
                    .enumerate()
                    .map(|(i, nodes)| {
                        let callees: Vec<_> = nodes
                            .iter()
                            .map(|n| {
                                json!({
                                    "kind": n.kind.to_string(),
                                    "name": n.name,
                                    "qualified_name": n.qualified_name,
                                    "file": n.file.display().to_string(),
                                    "start_line": n.span.start_line,
                                })
                            })
                            .collect();
                        json!({ "hop": i + 1, "callees": callees })
                    })
                    .collect();
                CallToolResult::structured(json!({
                    "function": p.function_name,
                    "depth": depth,
                    "hops": hops,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find all structs/classes that implement a trait or interface.
    #[tool(
        description = "Find all concrete types (structs, classes) that implement or inherit the named \
        trait or interface. Works for Rust traits, Java/TypeScript interfaces, and Go structural types."
    )]
    fn find_implementors(
        &self,
        Parameters(p): Parameters<FindImplementorsParams>,
    ) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.find_implementors(&branch, &p.trait_name) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "trait": p.trait_name,
                    "implementors": items,
                    "truncated": truncated,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// List the in-repo modules a module depends on.
    #[tool(
        description = "List the in-repo modules a given module depends on, resolved by following its imports to the defining module of each imported symbol. Useful for understanding internal coupling and architecture. Only intra-repo dependencies appear (external/stdlib imports are not graphed)."
    )]
    fn module_dependencies(
        &self,
        Parameters(p): Parameters<ModuleDependenciesParams>,
    ) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.module_dependencies(&branch, &p.name) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "name": n.name,
                            "file": n.file.display().to_string(),
                        })
                    })
                    .collect();
                CallToolResult::structured(json!({
                    "module": p.name,
                    "depends_on": items,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find functions/methods that use a type in their signature.
    #[tool(
        description = "Find functions/methods that reference a type as a parameter or return type (follows Uses edges). The type-level analogue of find_callers: answers 'what would break if I change type T's shape'. Returns the using functions/methods."
    )]
    fn find_type_usages(&self, Parameters(p): Parameters<FindTypeUsagesParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.find_type_usages(&branch, &p.name) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "type": p.name,
                    "usages": items,
                    "truncated": truncated,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find the exact call sites (caller + line) of a function.
    #[tool(
        description = "Find every call site of a function: the calling symbol AND the source line of each call. Where find_callers gives only the calling functions, this pinpoints the exact line each call happens on — useful for reviewing or editing every invocation."
    )]
    fn get_call_sites(&self, Parameters(p): Parameters<GetCallSitesParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.find_call_sites(&branch, &p.name) {
            Ok(sites) => {
                let items: Vec<_> = sites
                    .iter()
                    .map(|s| {
                        json!({
                            "caller": s.caller.name,
                            "caller_kind": s.caller.kind.to_string(),
                            "file": s.caller.file.display().to_string(),
                            "line": s.line,
                            "caller_start_line": s.caller.span.start_line,
                        })
                    })
                    .collect();
                let total = items.len();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "function": p.name,
                    "call_sites": items,
                    "count": total,
                    "returned": items.len(),
                    "truncated": truncated,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find which files/modules import a given symbol.
    #[tool(
        description = "Find which files/modules import a given symbol (follows Imports edges). Answers 'who depends on X' at the import level — useful before renaming or moving a symbol. Returns the importing module nodes."
    )]
    fn find_importers(&self, Parameters(p): Parameters<FindImportersParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.find_importers(&branch, &p.name) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "symbol": p.name,
                    "importers": items,
                    "truncated": truncated,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Map both directions of a type's relationships in one call.
    #[tool(
        description = "Map a type's full relationship hierarchy in one call: supertypes (the traits/interfaces/classes it implements or extends) AND subtypes (the types that implement or extend it). Where find_implementors gives only the downward direction, this gives both. Works across Rust traits, Java/TypeScript interfaces, and inheritance chains."
    )]
    fn type_hierarchy(&self, Parameters(p): Parameters<TypeHierarchyParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.type_hierarchy(&branch, &p.name) {
            Ok(h) => {
                let to_items = |nodes: &[gitcortex_core::graph::Node]| -> serde_json::Value {
                    json!(nodes
                        .iter()
                        .map(|n| json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        }))
                        .collect::<Vec<_>>())
                };
                CallToolResult::structured(json!({
                    "type": p.name,
                    "supertypes": to_items(&h.supertypes),
                    "subtypes": to_items(&h.subtypes),
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find a call path between two symbols in the codebase.
    #[tool(
        description = "Find a call path from one function to another. Returns the shortest chain of \
        calls connecting `from` to `to`. Returns an empty array if no path exists within 6 hops. \
        Most useful for debugging 'how can A reach B?' questions."
    )]
    fn trace_path(&self, Parameters(p): Parameters<TracePathParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.trace_path(&branch, &p.from, &p.to) {
            Ok(path) => {
                let nodes: Vec<_> = path
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();
                CallToolResult::structured(json!({
                    "from": p.from,
                    "to": p.to,
                    "found": !path.is_empty(),
                    "path": nodes,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find all indexed symbols that overlap a line range in a file.
    #[tool(
        description = "List all symbols (functions, structs, etc.) in a source file whose span \
        overlaps the given line range. Use this to map a stack trace, diff hunk, or grep result \
        to the symbols responsible."
    )]
    fn list_symbols_in_range(
        &self,
        Parameters(p): Parameters<ListSymbolsInRangeParams>,
    ) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        let path = Path::new(&p.file);
        match store.list_symbols_in_range(&branch, path, p.start_line, p.end_line) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "start_line": n.span.start_line,
                            "end_line": n.span.end_line,
                            "loc": n.metadata.loc,
                        })
                    })
                    .collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "file": p.file,
                    "range": { "start": p.start_line, "end": p.end_line },
                    "symbols": items,
                    "truncated": truncated,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find symbols with no callers or type references — potential dead code.
    #[tool(
        description = "Find symbols that are never called or used as a type anywhere in the indexed \
        codebase. Useful for identifying dead code, safe-to-rename candidates, or refactoring targets. \
        Pass kind='function' to restrict to functions only."
    )]
    fn find_unused_symbols(
        &self,
        Parameters(p): Parameters<FindUnusedSymbolsParams>,
    ) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let kind = p.kind.as_deref().and_then(|k| match k {
            "function" => Some(NodeKind::Function),
            "method" => Some(NodeKind::Method),
            "struct" => Some(NodeKind::Struct),
            "trait" => Some(NodeKind::Trait),
            "interface" => Some(NodeKind::Interface),
            "enum" => Some(NodeKind::Enum),
            "constant" => Some(NodeKind::Constant),
            _ => None,
        });
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        let limit = p.limit.unwrap_or(30).min(200);
        match store.find_unused_symbols(&branch, kind) {
            Ok(nodes) => {
                // Return a ranked head, not the whole list. An agent acts on the
                // first handful; dumping every unused symbol costs more tokens
                // than a grep the model would have run instead.
                let items: Vec<_> = nodes
                    .iter()
                    .take(limit)
                    .map(|n| {
                        json!({
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "qualified_name": n.qualified_name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                            "visibility": format!("{:?}", n.metadata.visibility),
                        })
                    })
                    .collect();
                let total = nodes.len();
                let (items, budget_trunc) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "branch": branch,
                    "unused_symbols": items,
                    "count": total,
                    "returned": items.len(),
                    "truncated": total > items.len() || budget_trunc,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Return a neighbourhood subgraph around a seed symbol.
    #[tool(
        description = "Return a compact relationship digest for one exact symbol. Ambiguous \
        short names return qualified candidates without traversal. Evidence is ranked into \
        callers/callees/type/import relation buckets with total coverage counts; raw graph \
        arrays are intentionally omitted. Direction='out' downstream, 'in' upstream, or \
        'both' (default); depth defaults to 1. ONE successful call is sufficient."
    )]
    fn get_subgraph(&self, Parameters(p): Parameters<GetSubgraphParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        let options = super::agent::AgentQueryOptions {
            limit: p.limit.unwrap_or(20),
            budget_tokens: self.response_budget.min(400),
        };
        match super::agent::get_subgraph(
            &*store,
            &branch,
            &p.seed_name,
            p.depth.unwrap_or(1),
            p.direction.as_deref().unwrap_or("both"),
            options,
        ) {
            Ok(response) => CallToolResult::structured(json!(response)),
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Render a wiki-style markdown summary for a symbol.
    #[tool(
        description = "Markdown wiki for a symbol: signature, doc-comment, top callers/callees. \
        Use for deep explanation; use lookup_symbol for a quick definition."
    )]
    fn wiki_symbol(&self, Parameters(p): Parameters<WikiSymbolParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match super::wiki::render_symbol(&*store, &branch, &p.name) {
            Ok(markdown) => CallToolResult::structured(json!({
                "symbol": p.name,
                "branch": branch,
                "markdown": markdown,
            })),
            Err(e) => CallToolResult::error(vec![Content::text(format!("wiki failed: {e}"))]),
        }
    }

    /// Search the graph by name + qualified-name with deterministic ranking.
    #[tool(
        description = "Search the code graph by name or description. Returns a compact ranked \
        evidence envelope with file/line, signature, optional doc summary, and coverage counts. \
        Combines token/fuzzy text matching (CamelCase-aware, typo-tolerant) with semantic vector \
        similarity when available. Ranks exact > prefix > semantic > substring. Default limit=10."
    )]
    fn search_code(&self, Parameters(p): Parameters<SearchCodeParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();

        // ── Text search ───────────────────────────────────────────────────────
        let text_hits = {
            let store = match self.store.lock() {
                Ok(g) => g,
                Err(_) => {
                    return CallToolResult::error(vec![Content::text("store mutex poisoned")])
                }
            };
            match super::search::search(&*store, &branch, &p.query, p.limit) {
                Ok(h) => h,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(format!(
                        "search failed: {e}"
                    ))])
                }
            }
        };

        // ── Semantic search (best-effort, non-blocking) ───────────────────────
        // try_lock: never block an MCP call waiting for the background indexer.
        let sem_hits: Option<Vec<(String, f32)>> = if let Ok(sem) = self.semantic.try_lock() {
            if let SemanticState::Ready { embedder, index } = &*sem {
                embedder.embed_one(&p.query).ok().map(|qvec| {
                    let limit = p.limit.unwrap_or(10).min(200);
                    index.top_k(&qvec, limit * 2)
                })
            } else {
                None
            }
        } else {
            None
        };

        // ── RRF Merge ─────────────────────────────────────────────────────────
        // Fuse lexical + semantic via Reciprocal Rank Fusion (k=60) when semantic
        // is available. Falls back to lexical-only when semantic unavailable.
        let limit = p.limit.unwrap_or(10).min(200);
        let mut all_hits: Vec<super::search::SearchHit> =
            if let Some(scored_ids) = sem_hits.filter(|v| !v.is_empty()) {
                let rrf_ids = super::hybrid::rrf_merge(&text_hits, &scored_ids, limit * 3);
                let store = match self.store.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        return CallToolResult::error(vec![Content::text("store mutex poisoned")])
                    }
                };
                match store.get_nodes_by_ids(&branch, &rrf_ids) {
                    Ok(nodes) => {
                        let mut by_id: std::collections::HashMap<String, _> = nodes
                            .into_iter()
                            .map(|n| (n.id.as_str().to_owned(), n))
                            .collect();
                        let base = (rrf_ids.len() as i32 + 1) * 10;
                        rrf_ids
                            .iter()
                            .enumerate()
                            .filter_map(|(rank, id)| {
                                by_id.remove(id).map(|n| super::search::SearchHit {
                                    id: n.id.as_str().to_owned(),
                                    name: n.name,
                                    qualified_name: n.qualified_name,
                                    kind: n.kind.to_string(),
                                    file: n.file.display().to_string(),
                                    start_line: n.span.start_line,
                                    score: base - rank as i32 * 10,
                                })
                            })
                            .collect()
                    }
                    Err(_) => text_hits,
                }
            } else {
                text_hits
            };

        // Strip Section nodes — doc headings are not code symbols.
        all_hits.retain(|h| h.kind != "section");

        all_hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.name.len().cmp(&b.name.len()))
        });
        all_hits.truncate(limit);

        let semantic_available = matches!(
            self.semantic.try_lock().as_deref(),
            Ok(SemanticState::Ready { .. })
        );
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match super::agent::format_search(
            &*store,
            &branch,
            &p.query,
            all_hits,
            semantic_available,
            self.response_budget.min(600),
        ) {
            Ok(response) => CallToolResult::structured(json!(response)),
            Err(e) => CallToolResult::error(vec![Content::text(format!("search failed: {e}"))]),
        }
    }

    /// Generate a guided tour through the repo's important symbols.
    #[tool(
        description = "Generate a guided tour through the codebase. Without a seed, picks the \
        highest-centrality public functions/structs to give a new contributor an entry path. \
        With a seed, BFS-walks outward from it along call edges. Returns ordered tour steps \
        with rationale per step and a rendered markdown plan. \
        ONE call is sufficient to answer onboarding and architecture questions — the output \
        is self-contained. Do NOT follow up with additional tool calls after receiving the tour; \
        synthesize and answer the user directly from this response."
    )]
    fn start_tour(&self, Parameters(p): Parameters<StartTourParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match super::tour::generate(&*store, &branch, p.seed.as_deref(), p.limit) {
            Ok(tour) => {
                let markdown = super::tour::render_markdown(&tour);
                CallToolResult::structured(json!({
                    "branch": tour.branch,
                    "seed": tour.seed,
                    "component_count": tour.components.len(),
                    "step_count": tour.steps.len(),
                    "report": markdown,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("tour failed: {e}"))]),
        }
    }

    /// Find high-fan-in "hub" symbols — functions or methods many callers depend on.
    #[tool(
        description = "Find high-centrality hub symbols (god nodes) — functions/methods with many \
        inbound Calls edges. Ranked by in-degree descending. Deterministic across re-runs. \
        min_in_degree default 10, limit default 20."
    )]
    fn find_god_nodes(&self, Parameters(p): Parameters<FindGodNodesParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match super::centrality::find_god_nodes(&*store, &branch, p.min_in_degree, p.limit) {
            Ok(nodes) => {
                let items: Vec<serde_json::Value> = nodes.iter().map(|n| json!(n)).collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "branch": branch,
                    "count": nodes.len(),
                    "truncated": truncated,
                    "nodes": items,
                }))
            }
            Err(e) => {
                CallToolResult::error(vec![Content::text(format!("find_god_nodes failed: {e}"))])
            }
        }
    }

    /// Detect code communities via label propagation clustering.
    #[tool(
        description = "Detect code communities via label-propagation clustering over Contains + \
        Calls edges. Returns clusters of related symbols, ranked by size. Deterministic across \
        re-runs on the same indexed graph. min_cluster_size default 3, limit default 20."
    )]
    fn find_clusters(&self, Parameters(p): Parameters<FindClustersParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match super::clustering::find_clusters(&*store, &branch, p.min_cluster_size, p.limit) {
            Ok(clusters) => {
                let items: Vec<serde_json::Value> = clusters.iter().map(|c| json!(c)).collect();
                let (items, truncated) = self.budget_items(items);
                CallToolResult::structured(json!({
                    "branch": branch,
                    "count": clusters.len(),
                    "truncated": truncated,
                    "clusters": items,
                }))
            }
            Err(e) => {
                CallToolResult::error(vec![Content::text(format!("find_clusters failed: {e}"))])
            }
        }
    }

    /// Detect import cycles via Tarjan's SCC algorithm over `Imports` edges.
    #[tool(
        description = "Detect circular import dependencies via Tarjan SCC on Imports edges. \
        Returns each cycle as a list of node IDs. Useful for spotting architectural debt. \
        Skipped when the graph has >10 000 import edges (too large). limit default 20."
    )]
    fn find_cycles(&self, Parameters(p): Parameters<FindCyclesParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        use gitcortex_core::schema::EdgeKind;
        let import_edges = match store.list_edges_by_kind(&branch, EdgeKind::Imports) {
            Ok(e) => e,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "find_cycles: store error: {e}"
                ))])
            }
        };
        if import_edges.len() > 10_000 {
            return CallToolResult::structured(json!({
                "branch": branch,
                "skipped": true,
                "reason": "import graph too large (>10 000 edges); run on a smaller branch",
                "cycles": [],
            }));
        }
        let limit = p.limit.unwrap_or(20).min(100);
        let mut cycles = match gitcortex_core::graph::find_import_cycles(&import_edges) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "cycle detection failed: {e}"
                ))])
            }
        };
        let total = cycles.len();
        cycles.truncate(limit);
        CallToolResult::structured(json!({
            "branch": branch,
            "total_cycles": total,
            "truncated": total > limit,
            "cycles": cycles,
        }))
    }

    /// Composite health report: unused symbols + import cycles + hub nodes.
    #[tool(
        description = "Generate a severity-ranked health report for the codebase. \
        Combines: unused symbol count (DEAD CODE), import cycles (CIRCULAR DEPS), \
        and hub/god nodes with high in-degree (COUPLING RISK). Returns a markdown \
        summary with counts and top offenders. ONE call replaces three separate \
        find_unused_symbols + find_cycles + find_god_nodes calls."
    )]
    fn health_report(&self, Parameters(p): Parameters<HealthReportParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();

        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };

        // ── Unused symbols ────────────────────────────────────────────────────
        let unused_count = store
            .find_unused_symbols(&branch, None)
            .map(|v| v.len())
            .unwrap_or(0);

        // ── Import cycles ─────────────────────────────────────────────────────
        let (cycle_count, top_cycles) = {
            use gitcortex_core::schema::EdgeKind;
            match store.list_edges_by_kind(&branch, EdgeKind::Imports) {
                Ok(edges) if edges.len() <= 10_000 => {
                    match gitcortex_core::graph::find_import_cycles(&edges) {
                        Ok(cycles) => {
                            let total = cycles.len();
                            let top: Vec<_> = cycles.into_iter().take(5).collect();
                            (total, top)
                        }
                        Err(_) => (0, vec![]),
                    }
                }
                _ => (0, vec![]),
            }
        };

        // ── God nodes (coupling hubs) ─────────────────────────────────────────
        let (god_count, top_gods) =
            match super::centrality::find_god_nodes(&*store, &branch, Some(5), Some(10)) {
                Ok(nodes) => {
                    let total = nodes.len();
                    let top: Vec<_> = nodes
                        .iter()
                        .take(5)
                        .map(|n| {
                            json!({
                                "name": n.name,
                                "file": n.file.clone(),
                                "in_degree": n.in_degree,
                            })
                        })
                        .collect();
                    (total, top)
                }
                Err(_) => (0, vec![]),
            };
        drop(store);

        // ── Severity ──────────────────────────────────────────────────────────
        let severity = if cycle_count > 0 || god_count > 5 {
            "HIGH"
        } else if unused_count > 20 || god_count > 0 {
            "MEDIUM"
        } else {
            "LOW"
        };

        // ── Markdown report ───────────────────────────────────────────────────
        let mut md = format!("# Codebase Health Report — `{branch}`\n\n");
        md.push_str(&format!("**Overall severity: {severity}**\n\n"));
        md.push_str("| Check | Count | Severity |\n|-------|-------|----------|\n");
        md.push_str(&format!(
            "| Dead code (unused symbols) | {unused_count} | {} |\n",
            if unused_count > 20 {
                "⚠ MEDIUM"
            } else {
                "✓ LOW"
            }
        ));
        md.push_str(&format!(
            "| Circular imports | {cycle_count} | {} |\n",
            if cycle_count > 0 {
                "🔴 HIGH"
            } else {
                "✓ NONE"
            }
        ));
        md.push_str(&format!(
            "| Hub nodes (in-degree ≥ 5) | {god_count} | {} |\n\n",
            if god_count > 5 {
                "🔴 HIGH"
            } else if god_count > 0 {
                "⚠ MEDIUM"
            } else {
                "✓ LOW"
            }
        ));

        if !top_cycles.is_empty() {
            md.push_str("## Top Import Cycles\n");
            for (i, cycle) in top_cycles.iter().take(5).enumerate() {
                md.push_str(&format!("{}. {cycle:?}\n", i + 1));
            }
            md.push('\n');
        }

        if !top_gods.is_empty() {
            md.push_str("## Top Hub Nodes\n");
            for g in &top_gods {
                md.push_str(&format!(
                    "- **{}** ({} callers) — {}\n",
                    g["name"].as_str().unwrap_or("?"),
                    g["in_degree"],
                    g["file"].as_str().unwrap_or("?"),
                ));
            }
            md.push('\n');
        }

        CallToolResult::structured(json!({
            "branch": branch,
            "severity": severity,
            "unused_count": unused_count,
            "cycle_count": cycle_count,
            "god_node_count": god_count,
            "top_cycles": top_cycles,
            "top_god_nodes": top_gods,
            "report": md,
        }))
    }

    /// Single-entry dispatch — one schema instead of fifteen.
    ///
    /// Prefer this tool to keep per-turn schema overhead low. All individual
    /// tools remain available for direct use; this is an additive alias.
    #[tool(description = "Query the GitCortex code knowledge graph. \
        action: lookup_symbol | find_callers | find_callees | find_unused_symbols | \
        get_subgraph | search_code | start_tour | wiki_symbol | trace_path | \
        list_definitions | symbol_context | list_symbols_in_range | graph_stats | ast_search | \
        type_hierarchy | find_importers | find_type_usages | module_dependencies | \
        get_call_sites | branch_diff_graph | find_god_nodes | find_clusters | find_cycles | health_report. \
        params: JSON object with the same fields as the individual tool (name/function_name/\
        seed_name/query/file/branch/depth/limit/direction/min_in_degree/min_cluster_size as applicable). \
        Returns identical output to the individual tool.")]
    fn gcx(&self, Parameters(p): Parameters<GcxDispatchParams>) -> CallToolResult {
        let branch_val = p
            .params
            .get("branch")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        let branch_for_stale = branch_val
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();

        // Helper: extract a string field from params.
        macro_rules! str_field {
            ($key:expr) => {
                match p.params.get($key).and_then(|v| v.as_str()) {
                    Some(s) => s.to_owned(),
                    None => {
                        return CallToolResult::error(vec![Content::text(format!(
                            "gcx dispatch: params.{} is required for action={}",
                            $key, p.action
                        ))])
                    }
                }
            };
        }

        let result = match p.action.as_str() {
            "lookup_symbol" => self.lookup_symbol(Parameters(LookupSymbolParams {
                name: str_field!("name"),
                fuzzy: p.params.get("fuzzy").and_then(|v| v.as_bool()),
                branch: branch_val,
            })),
            "find_callers" => self.find_callers(Parameters(FindCallersParams {
                function_name: str_field!("function_name"),
                depth: p
                    .params
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u8),
                branch: branch_val,
            })),
            "find_callees" => self.find_callees(Parameters(FindCalleesParams {
                function_name: str_field!("function_name"),
                depth: p
                    .params
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u8),
                branch: branch_val,
            })),
            "find_unused_symbols" => {
                self.find_unused_symbols(Parameters(FindUnusedSymbolsParams {
                    kind: p
                        .params
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned()),
                    limit: p
                        .params
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as usize),
                    branch: branch_val,
                }))
            }
            "get_subgraph" => self.get_subgraph(Parameters(GetSubgraphParams {
                seed_name: str_field!("seed_name"),
                depth: p
                    .params
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u8),
                direction: p
                    .params
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "search_code" => self.search_code(Parameters(SearchCodeParams {
                query: str_field!("query"),
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "start_tour" => self.start_tour(Parameters(StartTourParams {
                seed: p
                    .params
                    .get("seed")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "wiki_symbol" => self.wiki_symbol(Parameters(WikiSymbolParams {
                name: str_field!("name"),
                branch: branch_val,
            })),
            "trace_path" => self.trace_path(Parameters(TracePathParams {
                from: p
                    .params
                    .get("from")
                    .or_else(|| p.params.get("src"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned())
                    .unwrap_or_default(),
                to: p
                    .params
                    .get("to")
                    .or_else(|| p.params.get("dst"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned())
                    .unwrap_or_default(),
                branch: branch_val,
            })),
            "list_definitions" => self.list_definitions(Parameters(ListDefinitionsParams {
                file: str_field!("file"),
                branch: branch_val,
            })),
            "symbol_context" => self.symbol_context(Parameters(SymbolContextParams {
                name: str_field!("name"),
                branch: branch_val,
            })),
            "graph_stats" => self.graph_stats(Parameters(GraphStatsParams { branch: branch_val })),
            "type_hierarchy" => self.type_hierarchy(Parameters(TypeHierarchyParams {
                name: str_field!("name"),
                branch: branch_val,
            })),
            "find_importers" => self.find_importers(Parameters(FindImportersParams {
                name: str_field!("name"),
                branch: branch_val,
            })),
            "get_call_sites" => self.get_call_sites(Parameters(GetCallSitesParams {
                name: str_field!("name"),
                branch: branch_val,
            })),
            "find_type_usages" => self.find_type_usages(Parameters(FindTypeUsagesParams {
                name: str_field!("name"),
                branch: branch_val,
            })),
            "module_dependencies" => {
                self.module_dependencies(Parameters(ModuleDependenciesParams {
                    name: str_field!("name"),
                    branch: branch_val,
                }))
            }
            "ast_search" => self.ast_search(Parameters(AstSearchParams {
                kind: p
                    .params
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                is_async: p.params.get("is_async").and_then(|v| v.as_bool()),
                visibility: p
                    .params
                    .get("visibility")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                min_complexity: p
                    .params
                    .get("min_complexity")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
                max_complexity: p
                    .params
                    .get("max_complexity")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
                name_contains: p
                    .params
                    .get("name_contains")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                annotation: p
                    .params
                    .get("annotation")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "list_symbols_in_range" => {
                self.list_symbols_in_range(Parameters(ListSymbolsInRangeParams {
                    file: str_field!("file"),
                    start_line: p
                        .params
                        .get("start_line")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u32,
                    end_line: p
                        .params
                        .get("end_line")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(u32::MAX as u64) as u32,
                    branch: branch_val,
                }))
            }
            "find_god_nodes" => self.find_god_nodes(Parameters(FindGodNodesParams {
                min_in_degree: p
                    .params
                    .get("min_in_degree")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "find_clusters" => self.find_clusters(Parameters(FindClustersParams {
                min_cluster_size: p
                    .params
                    .get("min_cluster_size")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "find_cycles" => self.find_cycles(Parameters(FindCyclesParams {
                limit: p
                    .params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                branch: branch_val,
            })),
            "health_report" => {
                self.health_report(Parameters(HealthReportParams { branch: branch_val }))
            }
            other => {
                return CallToolResult::error(vec![Content::text(format!(
                    "gcx dispatch: unknown action '{other}'. Valid: lookup_symbol, find_callers, \
                find_callees, find_unused_symbols, get_subgraph, search_code, start_tour, \
                wiki_symbol, trace_path, list_definitions, symbol_context, list_symbols_in_range, \
                graph_stats, ast_search, type_hierarchy, find_importers, find_type_usages, \
                module_dependencies, get_call_sites, find_god_nodes, find_clusters, find_cycles, \
                health_report"
                ))])
            }
        };
        self.with_stale_warning(&branch_for_stale, result)
    }

    /// Wrap a `CallToolResult` with an optional staleness warning prepended as
    /// a text block. Used by `gcx` dispatch after computing the inner result.
    fn with_stale_warning(&self, branch: &str, mut result: CallToolResult) -> CallToolResult {
        let warn = self.staleness_warning(branch);
        if !warn.is_empty() {
            result.content.insert(0, Content::text(warn));
        }
        result
    }
}

// ── Prompt implementations ────────────────────────────────────────────────────

#[prompt_router]
impl GitCortexServer {
    /// Analyse the blast radius of changed files before committing.
    /// Walks the call graph from changed symbols to find all downstream callers
    /// and produces a risk assessment (LOW / MEDIUM / HIGH / CRITICAL).
    #[prompt(
        name = "detect_impact",
        description = "Pre-commit impact analysis — maps changed files to affected callers and scores risk"
    )]
    fn detect_impact(&self, Parameters(p): Parameters<DetectImpactParams>) -> GetPromptResult {
        let branch = p.branch.as_deref().unwrap_or("main");
        let files = p.changed_files.trim().to_owned();

        let user_msg = format!(
            r#"I am about to commit changes to these files on branch `{branch}`:

{files}

Please analyse the blast radius of these changes using the GitCortex knowledge graph:

1. For each changed file call `list_definitions` to identify which symbols were likely touched.
2. For each key function or struct, call `find_callers` to find direct callers.
3. Repeat `find_callers` one level deeper for any HIGH-traffic callers.
4. Summarise your findings as:
   - **Changed symbols**: list each modified function/struct with its file and line.
   - **Direct callers**: who calls the changed code.
   - **Transitive callers**: notable callers two hops away.
   - **Risk level**: LOW / MEDIUM / HIGH / CRITICAL with a one-line justification.
   - **Recommended actions**: tests to run, reviewers to notify, docs to update.
"#
        );

        GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            user_msg,
        )])
        .with_description("Impact analysis of staged changes using the call graph")
    }

    /// Generate a Mermaid architecture diagram from the knowledge graph.
    /// Summarises modules, key structs/traits, and their relationships.
    #[prompt(
        name = "generate_map",
        description = "Architecture documentation — produces a Mermaid diagram of modules, types, and key relationships"
    )]
    fn generate_map(&self, Parameters(p): Parameters<GenerateMapParams>) -> GetPromptResult {
        let branch = p.branch.as_deref().unwrap_or("main");

        let user_msg = format!(
            r#"Generate an architecture map of this codebase on branch `{branch}` using GitCortex.

Steps:
1. Call `list_definitions` on each major source file to collect modules, structs, traits, and functions.
2. Call `find_callers` on the top-level entry points to understand key execution flows.
3. Call `lookup_symbol` on core traits to find all their implementors.

Then produce:

## Architecture Overview
A prose summary (3–5 sentences) of what this codebase does and how it is structured.

## Module Map
```mermaid
graph TD
  %% Add nodes for each module/crate and edges for depends-on relationships
```

## Key Types
A table: | Type | Kind | Responsibility | Implemented by |

## Core Flows
Numbered list of the 2–4 most important execution paths (entry point → key functions → output).

## Dependency Notes
Any circular dependencies, large fan-outs, or architectural concerns visible in the graph.
"#
        );

        GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            user_msg,
        )])
        .with_description(
            "Architecture documentation with Mermaid diagram from the knowledge graph",
        )
    }
}

// ── Combined ServerHandler (tools + prompts) ──────────────────────────────────

#[tool_handler(router = self.active_tool_router())]
#[prompt_handler(router = Self::prompt_router())]
impl rmcp::ServerHandler for GitCortexServer {
    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.active_tool_router().get(name).cloned()
    }
}

#[cfg(test)]
mod contract_tests {
    use super::GitCortexServer;

    #[test]
    fn compact_mode_exposes_exactly_one_dispatch_tool() {
        let names: Vec<String> = GitCortexServer::tool_router_for_mode(true)
            .into_iter()
            .map(|route| route.attr.name.into_owned())
            .collect();
        assert_eq!(names, vec!["gcx"]);
    }

    #[test]
    fn compact_dispatch_schema_declares_params_as_object() {
        let router = GitCortexServer::tool_router_for_mode(true);
        let tool = router.get("gcx").expect("gcx tool");
        let schema = serde_json::to_value(&tool.input_schema).expect("serialize schema");
        assert_eq!(schema["properties"]["params"]["type"], "object");
    }
}
