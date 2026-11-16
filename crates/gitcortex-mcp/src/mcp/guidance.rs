//! Text agents read before choosing a tool. It is sent on every session, so
//! each line must earn its tokens.

/// MCP clients surface `instructions` in the system prompt; keep it short.
pub const MAX_INSTRUCTIONS_CHARS: usize = 1_000;

const INSTRUCTIONS: &str = "\
GitCortex is a code graph of this repo (definitions, calls, imports, types), synced with git.
Use it when the task needs structure, not text: who calls X, what breaks if X changes \
(run pre_edit_impact before editing, renaming, or deleting a symbol), where a type is used, \
how two symbols connect across files (trace_path), or orientation (search_code, symbol_context).
Use Grep/Read instead for exact strings, log or error messages, config, comments, and non-code files.
Read index.state on every result: ready = matches the checkout; unverified = local edits may be \
missing; stale/missing = behind or absent, follow index.next_action; indexing = retry shortly. \
Only when ready does an empty result mean absent.
Results are static evidence: dynamic dispatch, macros, reflection, and ignored files can be \
missing. Verify in source before claiming a change is safe.";

const COMPACT_CALL: &str =
    "\nCall actions through the gcx tool: {\"action\": \"find_callers\", \"params\": {...}}.";

pub fn server_instructions(compact: bool) -> String {
    if compact {
        format!("{INSTRUCTIONS}{COMPACT_CALL}")
    } else {
        INSTRUCTIONS.to_owned()
    }
}

/// Serialized compact `tools/list` budget; this schema rides in every turn.
pub const COMPACT_TOOLS_LIST_MAX_BYTES: usize = 2_000;

/// Actions the compact `gcx` tool dispatches, with the task words agents
/// search for. Order is the order agents read them in.
pub const DISPATCH_ACTIONS: &[(&str, &str)] = &[
    ("find_callers", "who calls X"),
    ("get_call_sites", "each call line"),
    ("pre_edit_impact", "before editing or renaming X"),
    ("find_callees", "what X calls"),
    ("trace_path", "from/to call path"),
    ("find_type_usages", "where type T is used"),
    ("type_hierarchy", "implementations, supertypes"),
    ("find_importers", "who imports X"),
    ("module_dependencies", ""),
    ("lookup_symbol", "find definition"),
    ("symbol_context", ""),
    ("wiki_symbol", ""),
    ("list_definitions", "file outline"),
    ("list_symbols_in_range", "line to symbol"),
    ("search_code", "search by name or description"),
    ("ast_search", "by kind, async, annotation, route"),
    ("answer_query", "plain-English question"),
    ("plan_query", ""),
    ("get_subgraph", ""),
    ("start_tour", "orientation"),
    ("graph_stats", ""),
    ("find_unused_symbols", "dead code"),
    ("find_god_nodes", ""),
    ("find_clusters", ""),
    ("find_cycles", ""),
    ("health_report", ""),
];

const COMPACT_LEAD: &str = "Use when you need code relationships grep cannot give: who calls a \
function (callers, references, call sites), what breaks before you edit, rename, or delete a \
symbol (impact, blast radius), the call path between two functions across files, where a type is \
used, imports and module dependencies, a file outline, or a symbol search. Not for exact strings, \
config, or comments.";

const COMPACT_PARAMS: &str = "params: object with the action's fields: question, name, \
function_name, from, to, query, file, kind, start_line, end_line, seed_name, direction, fuzzy, \
annotation, is_async, visibility, name_contains, min_in_degree, min_cluster_size, branch, depth, \
limit, offset, include_details. Read index.state in every result.";

pub fn compact_tool_description() -> String {
    let actions: Vec<String> = DISPATCH_ACTIONS
        .iter()
        .map(|(name, purpose)| {
            if purpose.is_empty() {
                (*name).to_owned()
            } else {
                format!("{name} ({purpose})")
            }
        })
        .collect();
    format!(
        "{COMPACT_LEAD}\nActions: {}.\n{COMPACT_PARAMS}",
        actions.join(", ")
    )
}

pub fn dispatch_action_names() -> String {
    DISPATCH_ACTIONS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use rmcp::ServiceExt;

    use super::*;
    use crate::mcp::test_support::run_async;
    use crate::mcp::tools::GitCortexServer;

    #[test]
    fn initialize_returns_bounded_instructions() {
        for compact in [true, false] {
            let instructions = run_async(|repo| async move {
                let server = GitCortexServer::new_with_mode(repo.path(), compact).expect("server");
                let (server_io, client_io) = tokio::io::duplex(64 * 1024);
                let serving = tokio::spawn(async move {
                    let running = server.serve(server_io).await.expect("serve");
                    running.waiting().await.ok();
                });
                let client = ().serve(client_io).await.expect("client");
                let info = client.peer_info().expect("initialize result");
                client.cancel().await.expect("cancel");
                serving.await.expect("server task");
                assert_eq!(info.server_info.name, "gitcortex");
                info.instructions.clone().expect("instructions")
            });
            assert!(
                instructions.chars().count() <= MAX_INSTRUCTIONS_CHARS,
                "{} chars",
                instructions.chars().count()
            );
            for required in [
                "who calls",
                "pre_edit_impact before editing",
                "trace_path",
                "Grep/Read instead for exact strings",
                "config",
                "index.state",
                "Only when ready does an empty result mean absent",
                "Verify in source",
            ] {
                assert!(instructions.contains(required), "missing {required:?}");
            }
            assert_eq!(instructions.contains("gcx tool"), compact);
        }
    }

    #[test]
    fn tool_descriptions_state_when_to_use() {
        for tool in GitCortexServer::tool_router_for_mode(false).list_all() {
            let description = tool.description.as_deref().unwrap_or("");
            let lowered = description.to_lowercase();
            assert!(
                lowered.starts_with("use when ") || lowered.starts_with("use before "),
                "{} does not lead with when to use it: {description}",
                tool.name
            );
            let lead = description.split(". ").next().unwrap_or("");
            assert!(lead.len() <= 360, "{} lead is too long: {lead}", tool.name);
        }
    }

    #[test]
    fn compact_tools_list_under_budget() {
        let tools = GitCortexServer::tool_router_for_mode(true).list_all();
        let bytes = serde_json::to_string(&tools).expect("serialize").len();
        assert!(
            bytes <= COMPACT_TOOLS_LIST_MAX_BYTES,
            "compact tools/list is {bytes} bytes"
        );
    }

    /// Claude Code defers MCP schemas behind a keyword search over tool name
    /// and description; these are words agents type for graph questions.
    #[test]
    fn deferred_search_terms_match_compact_tool() {
        let tools = GitCortexServer::tool_router_for_mode(true).list_all();
        let tool = tools.first().expect("gcx tool");
        let haystack = format!(
            "{} {}",
            tool.name,
            tool.description.as_deref().unwrap_or("")
        )
        .to_lowercase();
        for query in [
            "callers",
            "who calls",
            "references",
            "call sites",
            "call graph",
            "impact",
            "blast radius",
            "rename",
            "before edit",
            "dependencies",
            "imports",
            "definition",
            "usages",
            "implementations",
            "call path",
            "dead code",
            "code search",
            "outline",
        ] {
            for word in query.split_whitespace() {
                assert!(
                    haystack.contains(word),
                    "search term {query:?} misses the compact gcx tool"
                );
            }
        }
    }

    #[test]
    fn every_advertised_action_dispatches() {
        run_async(|repo| async move {
            let server = GitCortexServer::new_with_mode(repo.path(), true).expect("server");
            let (server_io, client_io) = tokio::io::duplex(256 * 1024);
            let serving = tokio::spawn(async move {
                let running = server.serve(server_io).await.expect("serve");
                running.waiting().await.ok();
            });
            let client = ().serve(client_io).await.expect("client");
            for (action, _) in DISPATCH_ACTIONS {
                let arguments = serde_json::json!({"action": action, "params": {}});
                let result = client
                    .call_tool(
                        rmcp::model::CallToolRequestParams::new("gcx")
                            .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
                    )
                    .await;
                let text = format!("{result:?}");
                assert!(!text.contains("unknown action"), "{action}: {text}");
            }
            client.cancel().await.expect("cancel");
            serving.await.expect("server task");
        });
    }
}
