# 0.8.0 positioning and release notes draft

Reviewed 2026-10-18. Not published; release gates remain open.

## Verified deployment baseline

The latest successful GitHub Pages deployment was 2026-10-05 at main commit
`95a5421707f40f17b24f0d4268bb13db170bfd62` (PR #85). Both the live homepage and
release log still identify v0.7.6 as latest. The latest GitHub binary release is
also v0.7.6, published 2026-08-26 UTC, with release notes dated 2026-08-25.
A Pages deployment is not a package release. Product PRs #77, #80, #82 and #85
are merged but have not been included in a newer tagged package release.

Sources:
- [Live homepage](https://bharath03-a.github.io/GitCortex/)
- [Live release log](https://bharath03-a.github.io/GitCortex/releases.html)
- [Pages deployment](https://github.com/bharath03-a/GitCortex/actions/runs/37354539374)
- [Latest package release](https://github.com/bharath03-a/GitCortex/releases/tag/v0.7.6)

## Moat to build

A graph, an MCP wrapper, a graph visualization, and small responses are features.
Our defensible advantage should become reliable, inexpensive relationship
evidence that stays useful through real coding sessions. This is a product
thesis, not an established advantage over competitors.

Build that advantage through four compounding assets:

1. Incremental correctness across edits, renames, commits and branch changes,
   with explicit readiness, reproducible regressions, and trustworthy recovery.
2. A corpus of pinned multi-step coding episodes with independent behavioral
   tests and replayable scoring, including negative and unsupported cases.
3. Efficient relationship navigation: bounded evidence now; generation-aware
   change summaries and explained test candidates when measured demand warrants.
4. A short setup-to-value path across supported clients, with diagnostics that
   explain limits rather than presenting missing evidence as safety.

Local operation is an important adoption property. It is not a unique moat by
itself. Anonymous call counts do not become a data moat automatically. Keep
usage local and optional; build the reusable learning asset from consented
feedback, public fixtures, and measured failure patterns.

Suggested headline: **Code relationships that stay useful while you code.**
Suggested subheading: **Explore callers, understand change impact, and query a
branch-aware local graph with explicit freshness and source evidence.**
Keep ordinary source search and tests in the workflow.

## Landing-page changes before 0.8.0 launch

- Replace grep-replacement framing with complementary graph-and-source use.
- Replace unconditional freshness and completeness assurances with supported
  update mechanisms, explicit readiness, and recovery behavior.
- Clarify local analysis versus setup downloads and the assistant's own network
  behavior. Do not promise that the entire surrounding toolchain has no network
  paths or can never leak data.
- Align the search description with the shipped deterministic lexical contract;
  distinguish any separately supported semantic feature and its setup costs.
- Add a demo showing edit, follow-up query, branch switch, and degraded recovery.
- Keep historical benchmark panels clearly dated. The live page's 20-task and
  one-round token figures are historical evidence, not current 35-task or
  continuous-coding results. Retain failures and explain exclusions.
- Publish version-matched install commands and tested upgrade/reindex steps.
- Keep usage insights and Activity UI marked as future work until implemented.

## Proposed public release notes

### GitCortex 0.8.0 — clearer evidence for changing code

GitCortex 0.8.0 improves how coding assistants explore relationships and assess
changes as a repository evolves. It adds typed question planning and progressive
search, strengthens incremental indexing, and makes graph scope and freshness
more explicit.

#### Added

- Typed, read-only planning for supported repository questions, with
  clarification when a question cannot be safely mapped to a supported action.
- Progressive symbol search with compact results, filters, and pagination, so
  assistants can narrow exploration before requesting more detail.
- Exact-revision PR-impact workflows with separated analysis/comment permissions,
  plus quickstart, pilot, and reversible adoption guidance.
- Opt-in local MCP call metadata for investigating tool usage and failures.
  Logging excludes raw arguments and response contents, uses a bounded background
  queue, and retains at most 2 MiB per configured path. Events can be dropped;
  logging is diagnostic, not a complete security audit.

#### Fixed

- Incremental relationship preservation when targets change and unchanged
  callers need reconciliation, repeated call-site handling, stable identities,
  and branch storage-key isolation.
- Repository-escaping source/setup symlink handling.
- Default query branch resolution and readiness checks so missing, stale, or
  degraded evidence does not silently become a successful dependency answer.
- Watcher handling of edits, renames, deletions, event pressure and recovery,
  with persistent dirty-state and parsed-source verification.
- MCP structured response envelopes for list results across compact/full modes.
- Explorer graph loading and font routes, and misleading empty-symbol setup
  output fixed since the previous tagged release.

#### Compatibility and limits

The graph schema advances to v15. Existing indexes require explicit rebuilding;
final tested upgrade and rollback instructions must accompany the release.
MCP clients should use object response envelopes and inspect readiness metadata.
Static graph analysis can miss dynamic or generated relationships. Validate
changes against current source and relevant tests. Risk labels are investigation
signals, not a guarantee of safe changes.

#### Validation

Publish the final candidate's pinned retrieval, continuous-coding, client and
competitor reports here after all required gates pass. No speed, token-savings,
or superiority claim is approved by this draft. Development test counts and the
earlier seven-task Cobra smoke are not substitutes for release evidence.

## Publication checklist

Keep this draft separate from CHANGELOG.md until the version and validated scope
are final: the Pages workflow generates the public release log from CHANGELOG.md.
Before publishing, reconcile every bullet with the final diff, replace the
validation paragraph with actual scoped results, add tested migration steps,
and synchronize homepage, changelog, GitHub release, packages and demo.
