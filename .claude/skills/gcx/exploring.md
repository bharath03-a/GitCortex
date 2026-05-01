# Exploring Unfamiliar Code

Use the GitCortex knowledge graph to navigate unfamiliar parts of the codebase fast.

## Workflow

1. **Find a symbol** — `lookup_symbol` to locate any struct, function, or trait by name
2. **See a file's shape** — `list_definitions` on any file to get all definitions at a glance
3. **Trace callers** — `find_callers` to understand who calls a function and build a call chain
4. **Visualise** — run `gcx viz` to open the interactive graph in the browser

## When to use
- Starting a task in an unfamiliar module
- Understanding how a piece of code fits into the larger system
- Navigating a large codebase without reading every file

## Examples
- "Where is `GraphStore` defined?" → `lookup_symbol(name: "GraphStore")`
- "What does `indexer.rs` contain?" → `list_definitions(file: "crates/gitcortex-indexer/src/indexer.rs")`
- "What calls `apply_diff`?" → `find_callers(function_name: "apply_diff")`
