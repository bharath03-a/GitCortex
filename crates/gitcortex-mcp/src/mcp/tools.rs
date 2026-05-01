use std::path::Path;
use std::sync::Arc;

use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;
use rmcp::{
    RoleServer,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
        ListPromptsResult, PaginatedRequestParams, PromptMessage, PromptMessageRole,
    },
    prompt, prompt_handler, prompt_router, service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

// ── Parameter types ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LookupSymbolParams {
    /// Symbol name to search for (unqualified).
    pub name: String,
    /// Branch name (defaults to "main" if omitted).
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindCallersParams {
    /// Name of the function/method to find callers of.
    pub function_name: String,
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

// ── Server ────────────────────────────────────────────────────────────────────

/// The MCP server handler. One shared `KuzuGraphStore` wrapped in `Arc<Mutex>`
/// so all handler calls can share state safely.
#[derive(Clone)]
pub struct GitCortexServer {
    store: Arc<std::sync::Mutex<KuzuGraphStore>>,
}

impl GitCortexServer {
    pub fn new(repo_root: &Path) -> anyhow::Result<Self> {
        let store = KuzuGraphStore::open(repo_root)?;
        Ok(Self {
            store: Arc::new(std::sync::Mutex::new(store)),
        })
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl GitCortexServer {
    /// Look up all nodes (functions, structs, traits, etc.) by name.
    #[tool(description = "Look up nodes in the code knowledge graph by their unqualified name. Returns all matching symbols across files.")]
    fn lookup_symbol(
        &self,
        Parameters(p): Parameters<LookupSymbolParams>,
    ) -> CallToolResult {
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.lookup_symbol(&branch, &p.name) {
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

    /// Find all callers of a function or method.
    #[tool(description = "Find all functions/methods that call the named function. Traverses `calls` edges in the knowledge graph.")]
    fn find_callers(
        &self,
        Parameters(p): Parameters<FindCallersParams>,
    ) -> CallToolResult {
        let branch = p.branch.as_deref().unwrap_or("main").to_owned();
        let store = match self.store.lock() {
            Ok(g) => g,
            Err(_) => return CallToolResult::error(vec![Content::text("store mutex poisoned")]),
        };
        match store.find_callers(&branch, &p.function_name) {
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
                        })
                    })
                    .collect();
                CallToolResult::structured(json!(items))
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("query failed: {e}"))]),
        }
    }

    /// List all symbols defined in a source file, ordered by line number.
    #[tool(description = "List all functions, structs, traits, and other definitions in a source file, ordered by line number.")]
    fn list_definitions(
        &self,
        Parameters(p): Parameters<ListDefinitionsParams>,
    ) -> CallToolResult {
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
    #[tool(description = "Show what nodes were added or removed between two branches. Useful for understanding what changed in a feature branch vs main.")]
    fn branch_diff_graph(
        &self,
        Parameters(p): Parameters<BranchDiffParams>,
    ) -> CallToolResult {
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
                let removed: Vec<_> = diff
                    .removed_node_ids
                    .iter()
                    .map(|id| id.as_str())
                    .collect();
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
    #[prompt(name = "detect_impact",
             description = "Pre-commit impact analysis — maps changed files to affected callers and scores risk")]
    fn detect_impact(&self, Parameters(p): Parameters<DetectImpactParams>) -> GetPromptResult {
        let branch = p.branch.as_deref().unwrap_or("main");
        let files  = p.changed_files.trim().to_owned();

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

        GetPromptResult::new(vec![
            PromptMessage::new_text(PromptMessageRole::User, user_msg),
        ])
        .with_description("Impact analysis of staged changes using the call graph")
    }

    /// Generate a Mermaid architecture diagram from the knowledge graph.
    /// Summarises modules, key structs/traits, and their relationships.
    #[prompt(name = "generate_map",
             description = "Architecture documentation — produces a Mermaid diagram of modules, types, and key relationships")]
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

        GetPromptResult::new(vec![
            PromptMessage::new_text(PromptMessageRole::User, user_msg),
        ])
        .with_description("Architecture documentation with Mermaid diagram from the knowledge graph")
    }
}

// ── Combined ServerHandler (tools + prompts) ──────────────────────────────────

#[tool_handler]
#[prompt_handler(router = Self::prompt_router())]
impl rmcp::ServerHandler for GitCortexServer {}
