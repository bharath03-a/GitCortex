use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gitcortex_core::{
    schema::{NodeKind, Visibility},
    store::{AttributeFilter, GraphStore},
};
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::embeddings::{Embedder, SemanticIndex};

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
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

// ── Parameter types ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GcxDispatchParams {
    /// Which graph operation to run. One of: lookup_symbol, find_callers, find_callees,
    /// find_unused_symbols, get_subgraph, search_code, start_tour, wiki_symbol,
    /// trace_path, list_definitions, symbol_context, list_symbols_in_range, graph_stats,
    /// ast_search.
    pub action: String,
    /// Parameters for the chosen action as a JSON object (same fields as the
    /// individual tool: name, function_name, seed_name, query, file, branch,
    /// depth, limit, direction, src, dst, start_line, end_line).
    pub params: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LookupSymbolParams {
    /// Symbol name to search for (unqualified).
    pub name: String,
    /// When true, matches any symbol whose name *contains* `name` (substring).
    /// When false (default), exact match only.
    pub fuzzy: Option<bool>,
    /// Branch name (defaults to "main" if omitted).
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindCallersParams {
    /// Name of the function/method to find callers of.
    pub function_name: String,
    /// How many hops to walk up the call graph (1–5, default 1).
    /// depth=1 returns direct callers only. depth=3 walks three levels.
    pub depth: Option<u8>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolContextParams {
    /// Symbol name to look up (unqualified).
    pub name: String,
    /// Branch name (defaults to current branch if omitted).
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDefinitionsParams {
    /// Repo-relative path to a source file.
    pub file: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BranchDiffParams {
    pub from_branch: String,
    pub to_branch: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetectChangesParams {
    /// Branch to query (defaults to "main" if omitted).
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindCalleesParams {
    /// Name of the function/method to trace callees of.
    pub function_name: String,
    /// How many hops to walk forward in the call graph (1–5, default 1).
    pub depth: Option<u8>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindImplementorsParams {
    /// Trait, interface, or abstract class name to find implementors of.
    pub trait_name: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TracePathParams {
    /// Starting function/method name.
    pub from: String,
    /// Target function/method name.
    pub to: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSymbolsInRangeParams {
    /// Repo-relative path to a source file.
    pub file: String,
    /// Start line of the range (1-indexed, inclusive).
    pub start_line: u32,
    /// End line of the range (1-indexed, inclusive).
    pub end_line: u32,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindUnusedSymbolsParams {
    /// Optional NodeKind filter: "function", "method", "struct", etc.
    pub kind: Option<String>,
    /// Max symbols returned (default 30, capped at 200). `count` always reports
    /// the true total; `truncated` flags when the list was longer.
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSubgraphParams {
    /// Seed symbol name (unqualified).
    pub seed_name: String,
    /// How many hops to expand from the seed (1–5, default 1). Depth 2+ on a
    /// high-degree hub returns a large subgraph — raise deliberately.
    pub depth: Option<u8>,
    /// Direction: "in" (callers/ancestors), "out" (callees/descendants), "both" (default).
    pub direction: Option<String>,
    /// Max nodes returned (default 30, capped at 200). Edges are filtered to the
    /// kept node set; `truncated` flags when the neighbourhood was larger.
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WikiSymbolParams {
    /// Symbol to summarise (unqualified name).
    pub name: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchCodeParams {
    /// Free-text query — substring matched against `name` and `qualified_name`.
    pub query: String,
    /// Max results (default 10, capped at 200).
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartTourParams {
    /// Optional seed symbol — when given, the tour walks outward from it
    /// along the call graph. When omitted, picks the highest-centrality
    /// entry points across the repo.
    pub seed: Option<String>,
    /// How many steps in the tour (default 12, capped at 50).
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphStatsParams {
    /// Branch to summarise (defaults to current branch if omitted).
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AstSearchParams {
    /// NodeKind filter: "function", "method", "struct", "trait", "interface",
    /// "enum", "constant", "type_alias", "module", etc. Omit for any kind.
    pub kind: Option<String>,
    /// When set, match only async (true) or only non-async (false) symbols.
    pub is_async: Option<bool>,
    /// Visibility filter: "pub", "pub_crate", or "private".
    pub visibility: Option<String>,
    /// Inclusive lower bound on cyclomatic complexity. Symbols without a
    /// recorded complexity are excluded when this is set.
    pub min_complexity: Option<u32>,
    /// Inclusive upper bound on cyclomatic complexity.
    pub max_complexity: Option<u32>,
    /// Case-insensitive substring the symbol name must contain.
    pub name_contains: Option<String>,
    /// Max results (default 30, capped at 200).
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

// ── Server ────────────────────────────────────────────────────────────────────

/// The MCP server handler. One shared `KuzuGraphStore` wrapped in `Arc<Mutex>`
/// so all handler calls can share state safely.
#[derive(Clone)]
pub struct GitCortexServer {
    store: Arc<Mutex<KuzuGraphStore>>,
    repo_root: PathBuf,
    default_branch: String,
    compact: bool,
    /// Semantic search state. Starts as `Pending`; background task flips to
    /// `Ready` once the model is loaded and missing vectors are embedded.
    /// `Arc<Mutex<…>>` so the background task and all clone'd handler instances
    /// share the same index.
    pub semantic: Arc<Mutex<SemanticState>>,
}

impl GitCortexServer {
    pub fn new(repo_root: &Path) -> anyhow::Result<Self> {
        Self::new_with_mode(repo_root, false)
    }

    pub fn new_with_mode(repo_root: &Path, compact: bool) -> anyhow::Result<Self> {
        let store = KuzuGraphStore::open(repo_root)?;
        let default_branch = detect_current_branch(repo_root).unwrap_or_else(|| "main".into());
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            repo_root: repo_root.to_owned(),
            default_branch,
            compact,
            semantic: Arc::new(Mutex::new(SemanticState::Pending)),
        })
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

    fn active_tool_router(&self) -> ToolRouter<Self> {
        let mut router = Self::tool_router();
        if self.compact {
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
                "find_unused_symbols",
                "get_subgraph",
                "wiki_symbol",
                "search_code",
                "start_tour",
            ] {
                router.disable_route(name);
            }
        }
        router
    }
}

/// Parse a NodeKind from its snake_case string form (matches `NodeKind::Display`).
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

/// Parse a Visibility from its snake_case string form.
fn parse_visibility(s: &str) -> Option<Visibility> {
    Some(match s {
        "pub" => Visibility::Pub,
        "pub_crate" => Visibility::PubCrate,
        "private" => Visibility::Private,
        _ => return None,
    })
}

fn detect_current_branch(repo_root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8(out.stdout).ok()?;
        let b = s.trim().to_owned();
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    } else {
        None
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
                CallToolResult::structured(json!(items))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Find all callers of a function or method, with optional multi-hop depth.
    #[tool(
        description = "Find callers of a function. depth=1 (default) = direct callers; \
        depth=2..5 = multi-hop. Results capped per hop; total count always returned."
    )]
    fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
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

        // Cap the caller list. The risk level is computed from the true total,
        // so a hub symbol still reports CRITICAL even though we return a head.
        const MAX_CALLERS: usize = 25;
        const MAX_PER_HOP: usize = 15;
        if depth == 1 {
            match store.find_callers(&branch, &p.function_name) {
                Ok(nodes) => {
                    let total = nodes.len();
                    let items: Vec<_> = nodes
                        .iter()
                        .take(MAX_CALLERS)
                        .map(|n| {
                            json!({
                                "hop": 1,
                                "kind": n.kind.to_string(),
                                "name": n.name,
                                "qualified_name": n.qualified_name,
                                "file": n.file.display().to_string(),
                                "start_line": n.span.start_line,
                            })
                        })
                        .collect();
                    let risk = match total {
                        0..=2 => "LOW",
                        3..=10 => "MEDIUM",
                        11..=30 => "HIGH",
                        _ => "CRITICAL",
                    };
                    CallToolResult::structured(json!({
                        "summary": format!("{total} caller(s) — risk {risk}{}",
                            if total > items.len() {
                                format!(", showing top {}", items.len())
                            } else { String::new() }),
                        "function": p.function_name,
                        "depth": 1,
                        "risk_level": risk,
                        "total_callers": total,
                        "returned": items.len(),
                        "truncated": total > items.len(),
                        "callers": items,
                    }))
                }
                Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
            }
        } else {
            match store.find_callers_deep(&branch, &p.function_name, depth) {
                Ok(result) => {
                    let hops: Vec<_> = result
                        .hops
                        .iter()
                        .enumerate()
                        .map(|(i, nodes)| {
                            let total = nodes.len();
                            let callers: Vec<_> = nodes
                                .iter()
                                .take(MAX_PER_HOP)
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
                            json!({
                                "hop": i + 1,
                                "total": total,
                                "truncated": total > MAX_PER_HOP,
                                "callers": callers,
                            })
                        })
                        .collect();
                    CallToolResult::structured(json!({
                        "function": p.function_name,
                        "depth": depth,
                        "risk_level": result.risk_level,
                        "hops": hops,
                    }))
                }
                Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
            }
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
        description = "Find symbols by structural attributes rather than name: kind (function/method/struct/...), is_async, visibility (pub/pub_crate/private), and cyclomatic complexity range. Combine filters to answer questions like 'all async methods', 'public structs', or 'functions with complexity ≥ 10'. Optional name_contains narrows further. Default limit=30."
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
        };

        if filter.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "ast_search needs at least one filter (kind, is_async, visibility, \
                 complexity bound, or name_contains)"
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
                        })
                    })
                    .collect();
                CallToolResult::structured(json!({
                    "branch": branch,
                    "results": items,
                    "returned": items.len(),
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
                let from_nodes = store.list_all_nodes(&p.from_branch).unwrap_or_default();
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
                CallToolResult::structured(json!({
                    "trait": p.trait_name,
                    "implementors": items,
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
                CallToolResult::structured(json!({
                    "file": p.file,
                    "range": { "start": p.start_line, "end": p.end_line },
                    "symbols": items,
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
                CallToolResult::structured(json!({
                    "branch": branch,
                    "unused_symbols": items,
                    "count": nodes.len(),
                    "returned": items.len(),
                    "truncated": nodes.len() > items.len(),
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Return a neighbourhood subgraph around a seed symbol.
    #[tool(
        description = "Return the subgraph centred on a seed symbol — nodes and edges reachable \
        within `depth` hops (default 1; raise for wider context). Direction='out' downstream, \
        'in' upstream, 'both' (default). Capped at `limit` nodes (default 30) with a `truncated` \
        flag — prefer find_callers/find_callees for a targeted answer over a wide neighbourhood dump."
    )]
    fn get_subgraph(&self, Parameters(p): Parameters<GetSubgraphParams>) -> CallToolResult {
        let branch = p
            .branch
            .as_deref()
            .unwrap_or(&self.default_branch)
            .to_owned();
        let depth = p.depth.unwrap_or(1).clamp(1, 5);
        let max_nodes = p.limit.unwrap_or(30).min(200);
        let direction = p.direction.as_deref().unwrap_or("both").to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.get_subgraph(&branch, &p.seed_name, depth, &direction) {
            Ok(sg) => {
                // Cap the node set, then keep only edges whose endpoints both
                // survive — a full neighbourhood dump on a hub symbol otherwise
                // costs more tokens than reading the file it describes.
                let kept: Vec<_> = sg.nodes.iter().take(max_nodes).collect();
                let kept_ids: std::collections::HashSet<String> =
                    kept.iter().map(|n| n.id.as_str()).collect();
                let nodes: Vec<_> = kept
                    .iter()
                    .map(|n| {
                        json!({
                            "id": n.id.as_str(),
                            "kind": n.kind.to_string(),
                            "name": n.name,
                            "file": n.file.display().to_string(),
                            "start_line": n.span.start_line,
                        })
                    })
                    .collect();
                let edges: Vec<_> = sg
                    .edges
                    .iter()
                    .filter(|e| {
                        kept_ids.contains(&e.src.as_str()) && kept_ids.contains(&e.dst.as_str())
                    })
                    .map(|e| {
                        json!({
                            "src": e.src.as_str(),
                            "dst": e.dst.as_str(),
                            "kind": e.kind.to_string(),
                        })
                    })
                    .collect();
                CallToolResult::structured(json!({
                    "seed": p.seed_name,
                    "depth": depth,
                    "direction": direction,
                    "node_count": sg.nodes.len(),
                    "edge_count": sg.edges.len(),
                    "returned_nodes": nodes.len(),
                    "returned_edges": edges.len(),
                    "truncated": sg.nodes.len() > nodes.len(),
                    "nodes": nodes,
                    "edges": edges,
                }))
            }
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
        description = "Search the code graph by name or description. Combines token/fuzzy text \
        matching (CamelCase-aware, typo-tolerant) with semantic vector similarity so you can \
        search without knowing the exact symbol name. Ranks exact > prefix > semantic > \
        substring; functions/structs boosted. Default limit=10."
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
        let sem_hits = if let Ok(sem) = self.semantic.try_lock() {
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

        // ── Merge: resolve semantic IDs to full nodes, deduplicate ────────────
        let mut all_hits = text_hits;
        let text_names: std::collections::HashSet<String> =
            all_hits.iter().map(|h| h.name.clone()).collect();

        if let Some(sem_ids) = sem_hits {
            if !sem_ids.is_empty() {
                let store = match self.store.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        return CallToolResult::error(vec![Content::text("store mutex poisoned")])
                    }
                };
                if let Ok(nodes) = store.get_nodes_by_ids(&branch, &sem_ids) {
                    for n in nodes {
                        if !text_names.contains(&n.name) {
                            all_hits.push(super::search::SearchHit {
                                name: n.name,
                                qualified_name: n.qualified_name,
                                kind: n.kind.to_string(),
                                file: n.file.display().to_string(),
                                start_line: n.span.start_line,
                                score: 45, // semantic match: between prefix (60) and substring (30)
                            });
                        }
                    }
                }
            }
        }

        let limit = p.limit.unwrap_or(10).min(200);
        all_hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.name.len().cmp(&b.name.len()))
        });
        all_hits.truncate(limit);

        CallToolResult::structured(json!({
            "query": p.query,
            "branch": branch,
            "count": all_hits.len(),
            "semantic_available": matches!(
                self.semantic.try_lock().as_deref(),
                Ok(SemanticState::Ready { .. })
            ),
            "hits": all_hits,
        }))
    }

    /// Generate a guided tour through the repo's important symbols.
    #[tool(
        description = "Generate a guided tour through the codebase. Without a seed, picks the \
        highest-centrality public functions/structs to give a new contributor an entry path. \
        With a seed, BFS-walks outward from it along call edges. Returns ordered tour steps \
        with rationale per step and a rendered markdown plan."
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
                    "steps": tour.steps,
                    "markdown": markdown,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("tour failed: {e}"))]),
        }
    }

    /// Single-entry dispatch — one schema instead of fifteen.
    ///
    /// Prefer this tool to keep per-turn schema overhead low. All individual
    /// tools remain available for direct use; this is an additive alias.
    #[tool(description = "Query the GitCortex code knowledge graph. \
        action: lookup_symbol | find_callers | find_callees | find_unused_symbols | \
        get_subgraph | search_code | start_tour | wiki_symbol | trace_path | \
        list_definitions | symbol_context | list_symbols_in_range | graph_stats | ast_search | branch_diff_graph. \
        params: JSON object with the same fields as the individual tool (name/function_name/\
        seed_name/query/file/branch/depth/limit/direction as applicable). \
        Returns identical output to the individual tool.")]
    fn gcx(&self, Parameters(p): Parameters<GcxDispatchParams>) -> CallToolResult {
        let branch_val = p
            .params
            .get("branch")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

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

        match p.action.as_str() {
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
            other => CallToolResult::error(vec![Content::text(format!(
                "gcx dispatch: unknown action '{other}'. Valid: lookup_symbol, find_callers, \
                find_callees, find_unused_symbols, get_subgraph, search_code, start_tour, \
                wiki_symbol, trace_path, list_definitions, symbol_context, list_symbols_in_range, \
                graph_stats, ast_search"
            ))]),
        }
    }
}

// ── Prompt parameter types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetectImpactParams {
    /// Comma-separated list of changed file paths (repo-relative).
    pub changed_files: String,
    /// Branch to query (defaults to "main").
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GenerateMapParams {
    /// Branch to document (defaults to "main").
    pub branch: Option<String>,
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

// ── Git diff helpers ──────────────────────────────────────────────────────────

fn run_git_diff(repo_root: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        None
    }
}

/// Parse unified diff text into `(repo_relative_file_path, [(start_line, end_line)])`.
fn parse_diff_hunks(diff: &str) -> Vec<(String, Vec<(u32, u32)>)> {
    let mut result: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
    let mut cur_file: Option<String> = None;
    let mut cur_hunks: Vec<(u32, u32)> = Vec::new();

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            if let Some(f) = cur_file.take() {
                if !cur_hunks.is_empty() {
                    result.push((f, std::mem::take(&mut cur_hunks)));
                }
            }
            cur_file = Some(path.to_owned());
        } else if line.starts_with("@@ ") {
            if let Some(hunk) = parse_hunk_header(line) {
                cur_hunks.push(hunk);
            }
        }
    }
    if let Some(f) = cur_file {
        if !cur_hunks.is_empty() {
            result.push((f, cur_hunks));
        }
    }
    result
}

/// Extract the new-file line range from a unified diff hunk header.
/// `@@ -old_start[,old_count] +new_start[,new_count] @@`
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ ")?;
    let plus_pos = rest.find(" +")?;
    let new_part = &rest[plus_pos + 2..];
    let end = new_part.find(' ').unwrap_or(new_part.len());
    let range = &new_part[..end];
    if let Some(comma) = range.find(',') {
        let start: u32 = range[..comma].parse().ok()?;
        let count: u32 = range[comma + 1..].parse().ok()?;
        Some((start, start + count.saturating_sub(1)))
    } else {
        let start: u32 = range.parse().ok()?;
        Some((start, start))
    }
}
