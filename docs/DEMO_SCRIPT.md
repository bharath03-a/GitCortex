# GitCortex demo — evidence-capture script

**Format:** 3–5 minute screen recording
**Goal:** Demonstrate a repeatable code-understanding workflow without making benchmark claims the recording does not measure.

## Before recording

Choose a public repository and pin the exact commit. Use a fresh clone or worktree so both runs begin from the same state.

Record:

- repository URL and commit SHA
- GitCortex version
- assistant, model, and client versions
- cache state
- exact prompt

Select one held-out question with a verifiable answer, for example:

> Where is authentication dispatched, and which functions call the dispatch point? Give file and line evidence.

Do not rewrite the prompt after seeing either result.

## Setup

```bash
cd path/to/repository
git rev-parse HEAD
gcx --version
gcx init --editor none
gcx doctor
gcx status
```

Then configure the assistant through the supported installer rather than manually writing MCP files:

```bash
gcx init --editor claude
# or codex / cursor / windsurf / copilot / antigravity
```

## Shot 1 — scope and controls

Show the pinned commit, client/model version, prompt, and whether caches are warm or cold. State that this is one workflow demonstration, not a universal performance claim.

## Shot 2 — baseline

Disable GitCortex for the client or use an equivalent clean client session. Ask the exact held-out question.

Capture:

- tool calls
- files read
- turns
- elapsed time
- total input/output tokens when available
- final answer and evidence

Do not cut failed searches from the recording.

## Shot 3 — GitCortex

Start a fresh client session with the compact GitCortex MCP server enabled. Ask the exact same prompt.

Highlight the sequence rather than a predetermined result:

1. typed question planning or compact `gcx` dispatch
2. ranked search evidence
3. exact caller/context query
4. final file/line evidence

Capture the same measures as the baseline.

## Shot 4 — pre-edit impact

Use a symbol found in the previous answer:

> Before I change this symbol, show the direct callers and likely impact with file and line evidence.

Verify at least one relationship against source code on screen.

## Shot 5 — branch-aware PR impact

On a real feature branch:

```bash
gcx blast-radius --base main --head HEAD --format text
```

Show changed symbols, affected callers, and the risk band. Explain that the report is evidence for review, not an automatic merge decision.

## Shot 6 — diagnostics and rollback

```bash
gcx doctor
gcx deinit --dry-run
```

Show that setup is diagnosable and the generated integration can be inspected before removal.

## End card

Report only what this recording measured:

- repository and commit
- client/model
- baseline vs GitCortex tool calls, files read, turns, and tokens
- correctness review
- cache state

Link to the raw transcript or machine-readable run data when publishing numerical claims.

## Editing rules

- Keep identical prompts and repository state visible.
- Do not remove failed GitCortex calls.
- Do not compare different models or clients as if they were one experiment.
- Label warm/cold cache state.
- Avoid “X% cheaper” or “Y× faster” overlays unless those figures come from the displayed repeated run set.
- Separate product demonstration from the pinned competitor benchmark performed during the release gate.
