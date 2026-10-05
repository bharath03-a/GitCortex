# GitCortex team pilot guide

Use this guide for a one- or two-week evaluation. The objective is not to prove a broad “AI productivity” claim. It is to determine whether local, branch-aware graph evidence improves specific code-understanding workflows for your repositories and assistants.

## Recommended pilot scope

Start with:

- 2–5 developers
- 1–3 actively maintained repositories
- at least one repository with duplicate symbol names or multiple modules
- one supported language per repository: Rust, Python, TypeScript/JavaScript, Go, or Java
- the assistants the team already uses

Do not begin with an organization-wide rollout. First establish indexing correctness, evidence quality, and workflow fit.

## Safety boundary

GitCortex runs locally and uses an embedded graph store. Source text and graph data are not sent to a hosted GitCortex service.

Before the pilot:

1. Review `.gitcortex/ignore` and add generated, vendored, sensitive, or irrelevant paths.
2. Run `gcx init --editor none` first.
3. Run `gcx doctor` and require a successful exit.
4. Use `gcx deinit --dry-run` to understand rollback.
5. Configure only the editor(s) used by pilot participants.
6. Review the generated GitHub Action before committing it.

## Baseline

For each selected task, record the existing workflow before enabling GitCortex:

- assistant and model version
- repository commit
- question or task prompt
- warm or cold cache state
- planned repetition count (use at least three runs per condition for numerical comparisons)
- tool calls and files read
- elapsed time
- input/output tokens when the client exposes them
- whether the final answer was correct
- evidence needed to verify the answer

Use held-out tasks; do not tune questions after seeing GitCortex results.

## Pilot tasks

Run representative tasks from these groups.

### Discovery

- Find the exact definition of an ambiguous symbol.
- Identify the main entry points for an unfamiliar subsystem.
- Find implementations of an interface or trait.

### Change understanding

- Identify direct and transitive callers before editing a function.
- Explain which modules depend on a changed type.
- Compare a feature branch to the target branch.

### Pull-request review

- Generate a blast-radius report for a real PR.
- Verify every reported changed symbol and affected caller.
- Record false positives, missing relationships, and ambiguous results.

## Measures

Track results per task rather than reporting one unsupported aggregate.

| Measure | How to record it |
|---|---|
| Correctness | Reviewer marks correct, partially correct, or incorrect |
| Evidence precision | Relevant evidence items / returned evidence items |
| Evidence coverage | Expected relationships found / known expected relationships |
| Exploration cost | Tool calls, files read, and turns before final answer |
| Token use | Total session input + output tokens, when available |
| Freshness | Time from committed edit to correct query result |
| Setup reliability | `gcx doctor` success and any remediation required |
| PR usefulness | Reviewer rating and concrete decision influenced |

Do not compare token counts across different models, prompts, repositories, cache states, or client versions without labeling those differences.

## Acceptance criteria

Choose thresholds and denominators before collecting results. A reasonable
starting gate for a pilot with at least 20 held-out tasks is:

- 0 stale or cross-scope answers across all held-out tasks
- 100% of answers used for a decision include verifiable file/line evidence
- fewer than 1% of measured commits/checkouts exceed the team's predeclared hook-latency budget
- median exploratory tool calls improve by at least 15% versus baseline on the same task set
- at least 70% of pilot developers choose to keep GitCortex enabled
- rollback has been tested successfully on one non-production clone

Token reduction may be an additional criterion, but it should not override correctness.

## Weekly review

At the end of each week, review:

- failures grouped by indexing, retrieval, client integration, or documentation
- tasks where plain text search was better
- tasks where graph evidence prevented incorrect exploration
- index freshness and hook latency
- feature requests that support the local-first code-understanding goal

Reject requests that turn the pilot into a generic document-ingestion or hosted knowledge-platform project.

## Rollout decision

Proceed to a broader team only when the acceptance criteria are met and failures have owners. For a wider rollout:

1. Pin the GitCortex version.
2. Commit a reviewed `.gitcortex/ignore`.
3. Standardize one editor setup path per team.
4. Add the PR blast-radius workflow in observation-only mode first.
5. Keep reports informational until false-positive and coverage data justify a required check.
6. Re-run the pilot task set after every GitCortex upgrade.

## Reporting template

```markdown
Repository / commit:
Client / model:
GitCortex version:
Task:
Cache state (warm/cold):
Run number / planned repetitions:
Baseline result:
GitCortex result:
Correctness:
Tool calls / files read / turns:
Total tokens (if available):
Freshness:
Reviewer notes:
Decision:
```
