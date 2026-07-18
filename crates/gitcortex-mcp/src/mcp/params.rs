use schemars::JsonSchema;
use serde::Deserialize;

// ── Parameter types ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GcxDispatchParams {
    /// Which graph operation to run. One of: lookup_symbol, find_callers, find_callees,
    /// find_unused_symbols, get_subgraph, search_code, start_tour, wiki_symbol,
    /// trace_path, list_definitions, symbol_context, list_symbols_in_range, graph_stats,
    /// ast_search, type_hierarchy, find_importers, find_type_usages, module_dependencies,
    /// get_call_sites, find_god_nodes, find_clusters.
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
pub struct TypeHierarchyParams {
    /// Type name (struct/class/trait/interface) to map relationships for.
    pub name: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindImportersParams {
    /// Symbol name to find importers of (the imported thing, unqualified).
    pub name: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCallSitesParams {
    /// Function/method name to find call sites of.
    pub name: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindTypeUsagesParams {
    /// Type name (struct/class/trait/interface/enum) to find usages of.
    pub name: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModuleDependenciesParams {
    /// Module name (file stem, e.g. "tools" for tools.rs) to list dependencies of.
    pub name: String,
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
pub struct FindGodNodesParams {
    /// Minimum inbound `Calls` edges to count as a hub (default 10).
    pub min_in_degree: Option<u32>,
    /// Max results (default 20, capped at 100).
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindClustersParams {
    /// Minimum members for a group to be reported as a cluster (default 3).
    pub min_cluster_size: Option<usize>,
    /// Max clusters returned (default 20, capped at 100).
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindCyclesParams {
    /// Max import cycles to return (default 20, capped at 100).
    pub limit: Option<usize>,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HealthReportParams {
    /// Branch to report on (defaults to current branch if omitted).
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
    /// Annotation/decorator name the symbol must carry (case-insensitive
    /// substring): "Test" finds `@Test`, "route" finds `@app.route`,
    /// "derive" finds `#[derive(...)]`.
    pub annotation: Option<String>,
    /// Max results (default 30, capped at 200).
    pub limit: Option<usize>,
    pub branch: Option<String>,
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
