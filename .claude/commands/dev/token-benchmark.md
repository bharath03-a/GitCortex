Run the REAL token benchmark (measured Claude usage) and turn it into a ship/fix decision. Use before a release or after changing any MCP tool.

Delegate to the **token-benchmark** agent. Pass along any arguments in `$ARGUMENTS`:
- a model id (default `claude-haiku-4-5-20251001`),
- a scope hint (`full` = 5 languages, or a single repo/tool name),
- `diff` to compare against the previous run.

The agent will:

1. Build `target/release/gcx` if stale.
2. **State the cost estimate** (full haiku sweep ≈ $5–8, 20–40 min; each session
   capped by `BUDGET`, default $0.75) before launching anything.
3. Run `docs/benchmarks/real-sweep.sh` (or a single `real-harness.sh` for a quick
   check), which runs Claude twice per question — grep arm vs GitCortex MCP arm —
   and records real `usage` tokens, cost, and turns.
4. Render the tool × language scorecard: `python3 docs/benchmarks/real-report.py`
   → `docs/benchmarks/real-report.html`.
5. Diff against the previous run; flag any tool that went green→red as a
   regression.
6. Report a per-tool verdict (**keep / marginal / REDESIGN**), a concrete minimal
   fix for each REDESIGN tool, and a one-line ship-or-block call.

Do not let it edit MCP code — it proposes fixes; apply them via the
`add-mcp-tool` skill, then re-run this to confirm the fix worked.

See `docs/benchmarks/RELEASE-GATE.md` for the full method.
