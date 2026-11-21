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

/// Maximum length of a per-response `next` teaching hint.
pub const MAX_NEXT_HINT_CHARS: usize = 220;

/// Whether a tool's structured payload represents an empty search/query result.
pub fn is_empty_result(data: &serde_json::Map<String, serde_json::Value>) -> bool {
    if data.get("status").and_then(|v| v.as_str()) == Some("not_found") {
        return true;
    }
    if data
        .get("coverage")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_u64())
        == Some(0)
    {
        return true;
    }
    if let Some(hops) = data.get("hops").and_then(|v| v.as_array()) {
        return hops.iter().all(|hop| {
            hop.get("callees")
                .and_then(|v| v.as_array())
                .map_or(true, Vec::is_empty)
        });
    }
    for key in [
        "items",
        "evidence",
        "matches",
        "callers",
        "callees",
        "usages",
        "implementors",
        "importers",
        "dependencies",
        "paths",
        "nodes",
        "clusters",
        "cycles",
    ] {
        if let Some(arr) = data.get(key).and_then(|v| v.as_array()) {
            return arr.is_empty();
        }
    }
    false
}

/// Bounded next-step hint attached to MCP tool results. Distinguishes an empty
/// result on a `ready` graph (absent from static graph) from an unindexed or
/// `unverified` graph, and names only real dispatched actions.
pub fn teaching_hint(
    action: &str,
    data: &serde_json::Map<String, serde_json::Value>,
    state: &str,
) -> Option<String> {
    if !matches!(state, "ready" | "unverified") {
        return Some(format!(
            "Index is {state} (not indexed/current): empty or blocked results do not mean absent. Follow index.next_action or inspect source."
        ));
    }
    if is_empty_result(data) {
        return Some(if state == "ready" {
            "Index is ready: absent from the static graph on this branch (dynamic/ignored code may still exist). Try search_code or Grep."
                .to_owned()
        } else {
            "Index is unverified (local edits may be missing): empty does not mean absent. Verify in source or try search_code."
                .to_owned()
        });
    }
    let hint = match action {
        "lookup_symbol" | "search_code" | "ast_search" => {
            "Run pre_edit_impact before editing a match, or find_callers / symbol_context to inspect usage."
        }
        "list_definitions" | "list_symbols_in_range" => {
            "Run pre_edit_impact before editing a symbol here, or find_callers / trace_path for cross-file flow."
        }
        "find_callers" => {
            "Inspect exact call lines with get_call_sites, or run pre_edit_impact before changing the symbol."
        }
        "pre_edit_impact" => {
            "Check exact call lines with get_call_sites and confirm callers in source and tests."
        }
        "find_callees" => {
            "Check upstream callers with find_callers or trace a multi-hop path with trace_path."
        }
        "symbol_context" | "get_subgraph" | "wiki_symbol" => {
            "Before editing, run pre_edit_impact; use get_call_sites for exact call lines."
        }
        "find_type_usages" | "type_hierarchy" => {
            "Run pre_edit_impact or find_callers on affected types before changing the interface."
        }
        "find_importers" | "module_dependencies" => {
            "Inspect exported symbols with list_definitions or callers with find_callers."
        }
        "trace_path" => {
            "Inspect symbols along the path with symbol_context or pre_edit_impact before refactoring."
        }
        "get_call_sites" => {
            "Verify the reported lines in source and run pre_edit_impact before changing the callee."
        }
        "start_tour"
        | "graph_stats"
        | "health_report"
        | "find_god_nodes"
        | "find_clusters"
        | "find_cycles" => {
            "Use search_code or lookup_symbol to jump to a symbol, then find_callers to trace dependencies."
        }
        "find_unused_symbols" => {
            "Verify each candidate with search_code and Grep before deleting (static graph misses dynamic refs)."
        }
        "answer_query" | "plan_query" => {
            "Follow up with pre_edit_impact before editing, or get_call_sites for exact line numbers."
        }
        _ => return None,
    };
    Some(hint.to_owned())
}

#[cfg(test)]
mod tests {
    use rmcp::ServiceExt;

    use super::*;
    use crate::mcp::test_support::{commit_file, run_async};
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

    #[test]
    fn next_hint_names_real_action() {
        let real_actions: std::collections::HashSet<&str> =
            DISPATCH_ACTIONS.iter().map(|(name, _)| *name).collect();
        let non_empty = serde_json::Map::from_iter([("items".to_owned(), serde_json::json!([1]))]);
        let empty = serde_json::Map::from_iter([("items".to_owned(), serde_json::json!([]))]);

        for (action, _) in DISPATCH_ACTIONS {
            for (label, payload, state) in [
                ("non-empty ready", &non_empty, "ready"),
                ("empty ready", &empty, "ready"),
                ("empty unverified", &empty, "unverified"),
                ("missing", &empty, "missing"),
            ] {
                let hint = teaching_hint(action, payload, state)
                    .unwrap_or_else(|| panic!("missing hint for {action} ({label})"));
                assert!(
                    hint.chars().count() <= MAX_NEXT_HINT_CHARS,
                    "{action} ({label}) hint too long ({} chars): {hint}",
                    hint.chars().count()
                );
                let mentioned: Vec<&str> = hint
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .filter(|token| token.contains('_') && *token != "next_action")
                    .collect();
                if matches!(state, "ready" | "unverified") {
                    assert!(
                        !mentioned.is_empty(),
                        "{action} ({label}) hint names no real action: {hint}"
                    );
                }
                for token in mentioned {
                    assert!(
                        real_actions.contains(token),
                        "{action} ({label}) hint names unimplemented action {token:?}: {hint}"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_result_distinguishes_absent_from_unindexed() {
        run_async(|repo| async move {
            let head = commit_file(repo.path(), "src/lib.rs", "pub fn present() {}\n", "init");
            let server = GitCortexServer::new_with_mode(repo.path(), true).expect("server");
            let served = server.clone();
            let (server_io, client_io) = tokio::io::duplex(256 * 1024);
            let serving = tokio::spawn(async move {
                let running = served.serve(server_io).await.expect("serve");
                running.waiting().await.ok();
            });
            let client = ().serve(client_io).await.expect("client");

            // 1. Unindexed (`missing`) repo: must say not indexed, never absent.
            let missing = client
                .call_tool(
                    rmcp::model::CallToolRequestParams::new("gcx").with_arguments(
                        serde_json::json!({
                            "action": "lookup_symbol",
                            "params": {"name": "no_such_symbol"}
                        })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                    ),
                )
                .await
                .expect("missing call")
                .structured_content
                .expect("missing structured");
            assert_eq!(missing["index"]["state"], "missing");
            let missing_next = missing["next"].as_str().expect("missing next");
            assert!(
                missing_next.contains("not indexed") && missing_next.contains("do not mean absent"),
                "{missing_next}"
            );

            // 2. Index the repo so state is `ready`: empty means absent from static graph.
            let ctx = server.store_context();
            {
                let mut store = ctx.store.lock().expect("store");
                crate::mcp::tree_jobs::apply_tree_index(repo.path(), &mut *store, "main", &head)
                    .expect("index main");
            }
            let ready = client
                .call_tool(
                    rmcp::model::CallToolRequestParams::new("gcx").with_arguments(
                        serde_json::json!({
                            "action": "lookup_symbol",
                            "params": {"name": "no_such_symbol"}
                        })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                    ),
                )
                .await
                .expect("ready call")
                .structured_content
                .expect("ready structured");
            assert_eq!(ready["index"]["state"], "ready");
            let ready_next = ready["next"].as_str().expect("ready next");
            assert!(
                ready_next.contains("ready") && ready_next.contains("absent"),
                "{ready_next}"
            );

            // 3. Unverified local edit: empty must explicitly refuse calling it absent.
            std::fs::write(
                repo.path().join("src/lib.rs"),
                "pub fn present() {}\npub fn dirty_only() {}\n",
            )
            .expect("edit");
            let unverified = client
                .call_tool(
                    rmcp::model::CallToolRequestParams::new("gcx").with_arguments(
                        serde_json::json!({
                            "action": "lookup_symbol",
                            "params": {"name": "dirty_only"}
                        })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                    ),
                )
                .await
                .expect("unverified call")
                .structured_content
                .expect("unverified structured");
            assert_eq!(unverified["index"]["state"], "unverified");
            let unverified_next = unverified["next"].as_str().expect("unverified next");
            assert!(
                unverified_next.contains("unverified")
                    && unverified_next.contains("does not mean absent"),
                "{unverified_next}"
            );

            client.cancel().await.expect("cancel");
            serving.await.expect("server task");
        });
    }

    #[test]
    fn advertised_capabilities_are_implemented() {
        run_async(|repo| async move {
            let server = GitCortexServer::new_with_mode(repo.path(), true).expect("server");
            let (server_io, client_io) = tokio::io::duplex(64 * 1024);
            let serving = tokio::spawn(async move {
                let running = server.serve(server_io).await.expect("serve");
                running.waiting().await.ok();
            });
            let client = ().serve(client_io).await.expect("client");
            let caps = &client.peer_info().expect("peer info").capabilities;
            assert!(caps.tools.is_some());
            assert!(caps.prompts.is_some());
            assert!(caps.resources.is_none());
            assert!(caps.logging.is_none());
            assert!(caps.completions.is_none());
            let prompts = client.list_prompts(None).await.expect("list_prompts");
            assert!(
                !prompts.prompts.is_empty(),
                "prompts capability advertised with no prompts"
            );
            client.cancel().await.expect("cancel");
            serving.await.expect("server task");
        });
    }
}
