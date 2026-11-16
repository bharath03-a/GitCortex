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
}
