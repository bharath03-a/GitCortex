---
name: add-mcp-tool
description: Recipe for adding a new MCP tool (query/action) to gitcortex-mcp. Mirrors wiki/search/tour. Use when exposing a new graph query to AI editors.
---

# Add MCP Tool

GitCortex exposes graph queries to AI editors via MCP tools defined in `crates/gitcortex-mcp/src/mcp/`. Every tool follows the same shape — clone `search.rs` (simplest) as the template.

## Files to touch (in order)

```
1. crates/gitcortex-mcp/src/mcp/<tool>.rs       # new — handler + input/output types
2. crates/gitcortex-mcp/src/mcp/mod.rs          # register the module
3. crates/gitcortex-mcp/src/mcp/tools.rs        # JSON schema entry for tool discovery
4. crates/gitcortex-mcp/src/mcp/server.rs       # dispatch arm in the tool-call handler (if not auto)
5. crates/gitcortex-cli/src/cmd/query.rs        # add CLI subcommand wrapping the same handler
6. crates/gitcortex-cli/src/main.rs             # wire subcommand into clap derive
7. .claude/commands/gcx/gcx-<tool>.md           # slash command wrapper
8. .claude/skills/gcx/<workflow>.md             # mention in the relevant user-facing skill
9. README.md                                    # bump MCP tool count (currently 12)
10. tests/integration/                          # end-to-end: gcx serve → mcp client → assert
```

## Input/output contract

- Input: `#[derive(Deserialize, JsonSchema)]` struct. Every field documented with `#[schemars(description = "...")]` — this becomes the tool description in the AI editor.
- Output: structured JSON, NOT a markdown blob. The MCP client renders. If markdown is needed (wiki/tour case), use a single `markdown: String` field, not raw mixed types.
- Errors: return `GitCortexError` via `?`. Convert to MCP error at the server boundary, not in the tool handler.

## Hard rules

- The handler is **async** (this is the only crate where async lives) but the underlying `GraphStore` call is **sync**. Wrap blocking work in `tokio::task::spawn_blocking` if it exceeds a few ms.
- Tool name in `tools.rs` MUST be `snake_case` per project convention.
- Never expose mutation tools (write/delete) — MCP surface is read-only by design.
- Branch-aware: every tool that reads the graph takes an implicit `current_branch` from the server context. Do not invent a `branch` input arg unless cross-branch is the explicit purpose.
- Output must be deterministic and bounded — cap result count, sort by a stable key.

## Verify

1. `cargo nextest run -p gitcortex-mcp` — handler unit test passes.
2. `gcx serve` locally → test with MCP inspector or Claude Code → tool appears, runs, returns expected shape.
3. CLI subcommand prints the same data the MCP tool returns.
4. Slash command file is one short paragraph + the `gcx query <subcommand>` invocation (match `gcx-wiki.md` style — minimal).

## Subagents

- **kuzu-cypher** if the tool needs a new graph query.
- **rust-reviewer** before commit.
