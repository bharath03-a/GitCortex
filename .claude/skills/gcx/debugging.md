# Debugging with the Call Graph

Trace bugs backward through the call chain using the knowledge graph.

## Workflow

1. **Locate the failing function** — `lookup_symbol` to find it and confirm the file/line
2. **Find direct callers** — `find_callers` to identify what triggered the bad code path
3. **Walk up the chain** — repeat `find_callers` on each caller to reach the entry point
4. **Check file context** — `list_definitions` on the relevant file to see surrounding code

## Key insight
`find_callers` traverses `Calls` edges in the knowledge graph — this is the actual parsed call graph,
not a grep. Use it iteratively to reconstruct the full execution path to a crash or wrong value.

## When to use
- Tracking down where a corrupted value originates
- Finding all the places that can trigger a bug
- Understanding the execution path to an error
