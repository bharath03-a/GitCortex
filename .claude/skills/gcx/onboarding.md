# Onboarding a New Codebase

Use GitCortex to give yourself (or a teammate) a guided entry path into an unfamiliar repo without reading every file.

## Workflow

1. **Global tour** — `gcx query tour` to get the 12 highest-centrality public symbols, each with its file:line and a centrality rationale.
2. **Wiki the seeds** — for each tour step, `gcx query wiki <name>` to read the symbol's signature, doc-comment, callers, and callees in one pass.
3. **Drill into a flow** — `gcx query tour --seed <symbol>` BFS-walks the call graph outward from a chosen entry point.
4. **Visualise** — `gcx viz` opens the interactive graph in the browser when the textual tour is not enough.

## When to use

- First day on a new repo.
- Asked to explain "what does this codebase do" to a stakeholder.
- About to touch a subsystem you haven't seen before.

## Examples

- "Give me a tour of this repo" → `gcx query tour --limit 15`
- "Walk me through how `apply_diff` is wired" → `gcx query tour --seed apply_diff`
- "I need a wiki page for `GraphStore` to paste into the README" → `gcx query wiki GraphStore`
