# GitCortex adoption messaging

Use these messages for launch posts, repository outreach, pilot invitations, and internal proposals. Keep claims attached to a named workflow and verifiable evidence. Do not claim broad superiority over other tools until the pinned release evaluation is complete.

## Product position

GitCortex is a local-first, branch-aware code knowledge graph for developers and coding agents. It keeps a graph current as Git changes, then returns bounded file/line evidence for code discovery, relationship queries, and change impact.

It is not a generic document knowledge platform, hosted code-ingestion service, or replacement for source review.

## Individual developers

### Problem

AI coding sessions often spend many turns locating the right definition, reading loosely related files, and reconstructing callers that the repository already implies.

### Message

> Give your coding assistant a local map of definitions, callers, implementations, and branch changes—then verify every answer against file and line evidence.

### Proof workflow

```bash
gcx init --editor none
gcx query search auth --format agent-json
gcx query symbol-context handle_request --format agent-json
gcx deinit --dry-run
```

### Call to action

Try the [10-minute reversible quickstart](QUICKSTART.md) on one repository and one real task.

## Development teams

### Problem

Code-understanding practices vary by developer and assistant. PR impact is often reconstructed manually, and token or speed claims are difficult to compare fairly.

### Message

> Standardize a small set of evidence-backed discovery and pre-edit-impact workflows without sending the repository to a GitCortex-hosted service.

### Proof workflow

- define held-out discovery and impact tasks
- record correctness, files read, tool calls, turns, and tokens when available
- generate a PR blast-radius report from exact base/head revisions
- review false positives and missing relationships weekly

### Call to action

Run the [team pilot](PILOT-GUIDE.md) with 2–5 developers before broader rollout.

## Organizations

### Problem

Organization-wide AI-tool rollouts need reversible installation, observable health, narrow permissions, and evidence that improvements survive different repositories and teams.

### Message

> Evaluate local code intelligence with explicit setup diagnostics, repository-scoped configuration, exact-revision PR analysis, and measurable rollout gates.

### Controls to highlight

- repository-local setup by default (`gcx init --editor none` for explicit CLI-only use)
- global editor changes require `--global-editor-config`
- shared external hook paths require `--shared-git-hooks`
- `gcx doctor` exits non-zero when required checks fail and explicitly notes that MCP registration alone does not confirm an active client connection
- `gcx deinit --dry-run` previews rollback (and `gcx deinit` preserves unrelated config and retains graph data unless `--purge` is passed)
- the generated `.github/workflows/gcx-blast-radius.yml` workflow uses two jobs: a read-only `blast-radius` job (`permissions: contents: read`) that checks out and indexes exact immutable PR SHAs (`pull_request.base.sha` and `pull_request.head.sha`) and uploads the `gitcortex-blast-radius` artifact, plus a same-repository `comment` job (`if: github.event.pull_request.head.repo.full_name == github.repository`, `permissions: actions: read, pull-requests: write`)
- fork PR reports remain artifacts and do not attempt write-token comments
- automated verification covers generated editor configs, the compact MCP protocol, and Read-hook scripts across the six supported installers; real desktop/CLI client discovery is recorded as **not exercised** until validated in a live pilot (Agy CLI is omitted/unsupported)

### Call to action

Approve a bounded pilot with pinned versions and predeclared acceptance criteria; do not start with a required organization-wide check. See [QUICKSTART.md](QUICKSTART.md#6-recovery) for recovery procedures and [EXPORT.md](EXPORT.md) for portable graph exports.

## Open-source maintainers

### Problem

Maintainers need useful contributor guidance and PR context without requiring a hosted bot to ingest their repository or granting broad credentials to forked code.

### Message

> Add a read-only `blast-radius` artifact job for every PR and a same-repository `comment` job for trusted branches, while keeping source analysis inside the repository’s own runner.

### Proof workflow

```bash
gcx init --ci --editor none
git diff -- .github/workflows/gcx-blast-radius.yml
gcx doctor
```

The generated workflow installs the matching `gitcortex` crate version with `--locked`, runs `blast-radius` with `permissions: contents: read` and `persist-credentials: false`, and only runs the `comment` job when `github.event.pull_request.head.repo.full_name == github.repository`. Review and commit the generated workflow only after checking its permissions and installation policy.

## Comparison language

Use:

- “GitCortex returned these caller relationships with file/line evidence.”
- “On this pinned task set, the GitCortex run used fewer exploratory reads.”
- “The exact base/head PR workflow found these affected callers.”
- “All source and graph data remained local to the machine or CI runner.”

Avoid until the release evaluation supports it:

- “always cheaper”
- “X times faster” without suite, client, rounds, and cache state
- “more accurate than every code graph”
- “enterprise-ready”
- “zero false positives”

## Launch checklist

Before publishing a numerical claim, include:

- GitCortex version and commit
- competitor version and commit, when applicable
- repository and revision
- exact prompt/task set
- client and model
- number of repeated runs
- warm/cold cache state
- correctness review method
- raw or machine-readable results

The final release gate owns cross-product claims. PR-4 materials should invite users to reproduce workflows, not pre-announce the PR-5 benchmark outcome.

## Short descriptions

### One sentence

> GitCortex gives developers and coding agents a local, branch-aware graph of code definitions and relationships, with bounded evidence for discovery and change impact.

### Repository description

> Local-first code knowledge graph with incremental indexing, branch-aware impact analysis, compact MCP integration, and a `gcx` CLI.

### Pilot invitation

> We are looking for developers to run a reversible one-week GitCortex pilot on real discovery and PR-impact tasks. We will measure correctness and exploration cost, publish the setup, and record where plain text search remains better.
