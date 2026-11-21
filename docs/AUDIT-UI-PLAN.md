# Local Activity panel

Status: proposed UI; bounded MCP metadata logging is implemented. This panel is
not yet available. The existing React UI fetches graph data from the local Axum
server in `gitcortex-viz`; it does not currently have an audit API.

## What the user sees

Add an Activity tab beside the repository graph, with:

- Logging state and retained storage: off, available, unavailable, or malformed.
- A recent-call list: time, tool, outcome, duration, response text bytes, index state.
- Action counts, error counts, latency median/p95, and readiness failures over
  the retained window. Show sample counts and the oldest/newest retained times.
- Filters for action and outcome, plus a clear explanation of missing history.

A stale-index failure should explain that source inspection or reindexing may
be needed. An old call's readiness is historical evidence, not the current
checkout's health. Do not infer branch identity from timestamps or generation
numbers: the current record has no branch or repository identifier. Generation
numbers are process-local, so group them with process ID rather than joining
across restarts. Process IDs alone are not durable session identifiers.

Use one configured audit path per repository daemon. The UI must explicitly
identify this as MCP usage; CLI and ordinary shell activity are not captured.
Keep correctness, task success, and token savings out of the dashboard unless
separate benchmark/client evidence supports them.

Schema version 2 changes response size to `response_text_bytes` and includes
cumulative drop/write-failure counts. Parse versions explicitly; do not mix v1
protocol byte measurements with v2 text bytes. Queue loss or shutdown can leave
gaps. Show loss counters as observed lower bounds, not proof of complete logs.

## Storage and implementation

Reuse the existing two log files, capped at 1 MiB each. Do not create a second
analytics database, persistent browser copy, or per-day/per-session files by
default. Logging off consumes no audit disk space. Enabled logging adds at most
2 MiB per configured path; manually created experiment logs have separate
budgets and are not covered by that cap.

Expose a read-only local `/api/activity` endpoint using the server's configured
path, never a path supplied through HTTP. Read at most the two bounded files;
aggregate in memory, return at most 100 recent rows and a fixed-size action
summary. Reject oversized files before parsing, cap line length, skip partial
or malformed lines with a visible skipped-record count, and handle rotation
between reads without crashing. Only return whitelisted metadata fields.

Serve the panel through the existing local UI boundary. Refresh only while the
Activity tab is visible, at a modest interval such as five seconds. Cancel
pending requests when hidden. Do not send log data to external analytics.
Provide opt-in setup instructions and storage usage before adding any controls
that modify logging configuration or delete data.

## Acceptance checks before shipping

Verify disabled/missing/unreadable logs, empty history, malformed/partial lines,
rotation during reads, oversized legacy files, and restarts. Ensure the UI
labels counts as retained-window counts, never lifetime totals. Check that no
argument/result content reaches the HTTP response or browser storage. Test
endpoint bounds and aggregation with deterministic fixtures; delegate browser
and Playwright checks to Luna per repository instructions.

## Agent-facing usage insights

Propose a read-only `usage_insights` action, available through compact dispatch,
that uses the same bounded metadata and aggregation as the Activity panel. It
must work when the graph is unavailable: do not put it behind graph-readiness
checks. Return logging/coverage state, window boundaries, sample count, and a
small list of observations with measured evidence, suggested next steps, and
limitations. Reading insights should not recursively generate recommendations
about insight requests themselves.

Initial rules can use existing metadata:

- Repeated `index_not_ready` or `index_changed`: report the count, recommend
  checking diagnostics and waiting for synchronization; avoid blind retries.
- High observed durations or payload sizes for an action: show sample count
  and distributions; mention documented bounds/filters where that action
  supports them. Do not claim the response was irrelevant or wasted tokens.
- Repeated protocol errors: suggest checking the advertised schema. Error
  metadata alone cannot identify which argument was invalid.

Do not classify frequent calls as misuse: progressive exploration may require
repetition. Do not infer duplicate queries, omitted branches, abandoned results,
or the best replacement tool from action counts. Future signals could include
explicit-branch presence, truncation and returned-item counts, and an ephemeral
per-daemon keyed argument fingerprint. These require a separate privacy and
schema design; raw arguments remain excluded. Session attribution also needs an
explicit trustworthy client/session mechanism before recommendations claim to
be about one agent rather than all clients of a daemon.

Keep rules deterministic and bounded, with no background LLM calls, extra
persistent database, or automatic intervention. The agent decides whether a
suggestion applies. Benchmark insights enabled versus disabled during continuous
coding: task correctness first, then failed calls, latency, and measured client
token usage. Include the extra insights call's cost. Share the observation model
between UI and MCP so they cannot give conflicting advice.
