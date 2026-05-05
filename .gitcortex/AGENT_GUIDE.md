# GitCortex Agent Guide

This repository is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
The MCP server is configured in `mcp.json` (or your editor's equivalent). Use these
tools to navigate the codebase — they read the live knowledge graph, not grep output.

## Available MCP Tools

| Tool | Description |
|------|-------------|
| `lookup_symbol(name)` | Find any struct, function, trait, or class by name |
| `find_callers(function_name)` | Who calls this function? |
| `find_callees(function_name, depth)` | What does this function call? (forward trace) |
| `list_definitions(file)` | All symbols defined in a file |
| `find_implementors(trait_name)` | Who implements this trait or interface? |
| `trace_path(from, to)` | All call paths from A to B |
| `list_symbols_in_range(file, start, end)` | Symbols overlapping a line range |
| `find_unused_symbols(branch)` | Dead code candidates (0 callers) |
| `get_subgraph(seed_name, depth, direction)` | Everything around a symbol |
| `detect_changes(base_branch)` | Changed symbols + blast radius vs another branch |

## Workflows

**Navigating unfamiliar code**
1. `lookup_symbol("ThingYouHeardAbout")` — confirm it exists and find the file
2. `list_definitions("path/to/file.rs")` — see the full shape of a file
3. `get_subgraph("ThingYouHeardAbout", 2, "both")` — visualise its neighbours

**Debugging**
1. `lookup_symbol("failingFn")` — confirm location
2. `find_callers("failingFn")` — walk up the call chain
3. Repeat until you reach an entry point

**Impact analysis before changing a public API**
1. `find_callers("publicFn")` — direct callers
2. `get_subgraph("publicFn", 3, "in")` — full upstream blast radius
3. `find_implementors("TraitYouAreChanging")` — all implementors that must change

**Safe refactoring**
1. `find_unused_symbols(branch)` — find candidates for deletion
2. `list_symbols_in_range(file, start, end)` — map a diff hunk to graph nodes
3. `trace_path(from, to)` — verify a code path before removing an intermediate

## Slash commands (Claude Code / Cursor)
- `/gcx-lookup <name>` — `lookup_symbol` with formatted output
- `/gcx-callers <name>` — `find_callers` with call chain summary
- `/gcx-file <path>` — `list_definitions` ordered by line
- `/gcx-blast-radius` — changed symbols + risk report vs main
