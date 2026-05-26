# Explaining a Symbol

When asked "what is X" or "explain X", compose GitCortex queries instead of grepping files.

## Workflow

1. **Find candidates** — `gcx query search <fragment>` ranks hits by exact > prefix > substring with kind boosts. Pick the right qualified name from the list.
2. **Render the wiki** — `gcx query wiki <name>` returns a markdown page with signature, doc-comment, callers, callees, used-by.
3. **Walk neighbours** — for any caller/callee that matters, repeat `gcx query wiki` on it to follow the thread.
4. **Trace a path** — `gcx query trace-path <from> <to>` if the user wants to know how A reaches B.

## When to use

- "What does `<symbol>` do?"
- "Why is `<symbol>` used here?"
- "Show me the public API for `<module>`"
- Drafting a PR description that references a touched symbol.

## Output rules

- Paste the wiki markdown verbatim — it is already README-ready.
- When listing callers/callees, keep the `file:line` links so the reader can jump.
- When the question is fuzzy ("find anything about auth"), use `gcx query search auth` first and present the top hits, then offer to wiki the chosen one.
