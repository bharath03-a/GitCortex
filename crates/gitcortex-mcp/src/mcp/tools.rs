use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitcortex_core::{schema::NodeKind, store::GraphStore};
use gitcortex_store::kuzu::KuzuGraphStore;
use rmcp::{
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
pub struct ContextParams {
    /// Symbol name to look up (unqualified).
    pub name: String,
    /// Branch name (defaults to "main" if omitted).
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
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSubgraphParams {
    /// Seed symbol name (unqualified).
    pub seed_name: String,
    /// How many hops to expand from the seed (1–5, default 2).
    pub depth: Option<u8>,
    /// Direction: "in" (callers/ancestors), "out" (callees/descendants), "both" (default).
    pub direction: Option<String>,
    pub branch: Option<String>,
}

// ── Server ────────────────────────────────────────────────────────────────────

/// The MCP server handler. One shared `KuzuGraphStore` wrapped in `Arc<Mutex>`
/// so all handler calls can share state safely.
#[derive(Clone)]
pub struct GitCortexServer {
    store: Arc<std::sync::Mutex<KuzuGraphStore>>,
    repo_root: PathBuf,
}

impl GitCortexServer {
    pub fn new(repo_root: &Path) -> anyhow::Result<Self> {
        let store = KuzuGraphStore::open(repo_root)?;
        Ok(Self {
            store: Arc::new(std::sync::Mutex::new(store)),
            repo_root: repo_root.to_owned(),
        })
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        description = "Find all functions/methods that call the named function. \
        Use depth=1 (default) for direct callers only, or depth=2..5 to walk the call graph \
        multiple hops. Returns callers grouped by hop distance with a risk level."
    )]
    fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
        let depth = p.depth.unwrap_or(1).max(1);
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };

        if depth == 1 {
            match store.find_callers(&branch, &p.function_name) {
                Ok(nodes) => {
                    let items: Vec<_> = nodes
                        .iter()
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
                    CallToolResult::structured(json!({
                        "function": p.function_name,
                        "depth": 1,
                        "risk_level": match items.len() {
                            0..=2 => "LOW",
                            3..=10 => "MEDIUM",
                            11..=30 => "HIGH",
                            _ => "CRITICAL",
                        },
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
                            let callers: Vec<_> = nodes
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
                            json!({ "hop": i + 1, "callers": callers })
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
    fn context(&self, Parameters(p): Parameters<ContextParams>) -> CallToolResult {
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
                let removed: Vec<_> = diff.removed_node_ids.iter().map(|id| id.as_str()).collect();
                CallToolResult::structured(json!({
                    "from": p.from_branch,
                    "to": p.to_branch,
                    "added_nodes": added,
                    "removed_node_ids": removed,
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();

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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
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
        match store.find_unused_symbols(&branch, kind) {
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
                        })
                    })
                    .collect();
                CallToolResult::structured(json!({
                    "branch": branch,
                    "unused_symbols": items,
                    "count": nodes.len(),
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// Return a neighbourhood subgraph around a seed symbol.
    #[tool(
        description = "Return the subgraph centred on a seed symbol — all nodes and edges reachable \
        within `depth` hops. Use direction='out' for downstream only, 'in' for upstream only, \
        or 'both' (default) for both directions. Ideal for architecture rendering and impact analysis."
    )]
    fn get_subgraph(&self, Parameters(p): Parameters<GetSubgraphParams>) -> CallToolResult {
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
        let depth = p.depth.unwrap_or(2);
        let direction = p.direction.as_deref().unwrap_or("both").to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.get_subgraph(&branch, &p.seed_name, depth, &direction) {
            Ok(sg) => {
                let nodes: Vec<_> = sg
                    .nodes
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
                    "nodes": nodes,
                    "edges": edges,
                }))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
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

#[tool_handler]
#[prompt_handler(router = Self::prompt_router())]
impl rmcp::ServerHandler for GitCortexServer {}

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
