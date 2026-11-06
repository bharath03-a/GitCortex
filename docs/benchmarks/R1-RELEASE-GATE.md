# Combined R1 release gate

Run `python3 scripts/r1_release_gate.py` from the repository. A missing manifest
or incomplete evidence fails with JSON `ready: false`, actionable `errors`, and
exit status 1. No placeholder success manifest is supplied.

Default manifest: `tools/agent-bench/r1-release-manifest.json`. `--manifest PATH`
selects another file. All result and artifact paths are repository-relative;
resolved paths, including symlinks, must remain inside the repository.

## Manifest format

```json
{
  "candidate_commit": "<40 lowercase hex characters>",
  "source_tree_sha256": "<python3 scripts/r1_release_gate.py --source-tree>",
  "binary_path": "target/release/gcx",
  "binary_sha256": "<64 lowercase hex characters>",
  "suite_path": "tools/agent-bench/suite.toml",
  "suite_sha256": "<64 lowercase hex characters>",
  "competitors_path": "tools/agent-bench/competitors.toml",
  "competitors_sha256": "<64 lowercase hex characters>",
  "required_lanes": ["retrieval", "codex-graph-cli", "claude-code-mcp", "graphify", "codegraph"],
  "results": [{"path": "tools/agent-bench/results/r1-retrieval.summary.json", "sha256": "<sha256>"}],
  "claims": [{"id": "named-scoped-claim", "result_id": "retrieval-candidate", "task_ids": ["task-id"]}]
}
```

The list above illustrates required lanes; each lane needs a separate result.
Document unavailable optional clients explicitly in the release report. Do not
remove required lanes merely to make the gate pass.

## Normalized result format

```json
{
  "id": "retrieval-candidate",
  "lane": "retrieval",
  "candidate_commit": "<candidate commit>",
  "binary_sha256": "<candidate binary hash>",
  "suite_sha256": "<suite hash>",
  "repo_commits": {"repository": "<40-character commit>"},
  "client": "none",
  "model": "none",
  "rounds": 1,
  "cache_state": "cold-index",
  "tasks": [{"id": "task-id", "valid": true, "quality_non_inferior": true,
             "samples": [{"round": 1, "valid": true, "quality_non_inferior": true}]}],
  "relevance_non_inferior": true,
  "artifacts": [{"path": "tools/agent-bench/results/r1.jsonl", "sha256": "<raw trace hash>"}]
}
```

The examples abbreviate the full task list. Every result must account for the
exact pinned suite's task IDs and repository commits, with one sample per task
per round. Native-client and competitor lanes require at least three rounds (at
most 100 are accepted). Retrieval
requires one complete deterministic run, valid tasks, non-inferior evidence,
and passing relevance. Summaries must be derived from retained harness traces;
the checker verifies checksums and attestations, not whether a human fabricated
a summary. Reviewers must verify scoring and raw results. This is one gate,
not a substitute for correctness, security, packaging, and independent review.

The gate also rejects package-version disagreement, dirty or untracked files,
duplicate/empty results, missing provenance, missing lanes, invalid tasks,
quality regressions, and claims without matching result/task identifiers.
It checks workspace Cargo, lockfile crates, npm platform/umbrella versions and
optional dependency pins, and Python's version.

The competitor manifest is a TOML or JSON object keyed by `graphify` and
`codegraph`. Each pin must contain `url` (the canonical repository), `commit`
(40 hex characters), `install_command`, and nonempty `runtime_versions`.
Each competitor result includes a `competitor` object identical to that pin.
Do not fill runtime versions from guesses: record the smoke-tested environment.

Competitor losses may use `outcome: "loss"` with valid measured samples;
non-inferiority is required for GCX correctness lanes, not as a demand that
GCX beat every competitor. Unsupported competitor tasks must still appear with
`outcome: "unsupported"` and a nonempty `reason`; they do not enter claims or
valid-task denominators. At least one supported task must be measured. Failed
commands remain in raw traces and block valid evidence; do not discard them.

Build and benchmark a committed candidate first, then commit evidence separately.
The candidate and checked-out HEAD must have the same input-tree fingerprint,
excluding documentation, README/changelog, and release evidence. This supports
the planned squash merge: ancestry may change while approved inputs remain
identical. Changing code, package inputs, or harness inputs requires rebuilding
and rerunning. Retain/fetch the candidate commit object before running the gate
on merged main so its fingerprint can be independently verified. This avoids
the circular requirement that a trace embed the hash of its own future commit.

Test the validator with `python3 -m unittest scripts/test_r1_release_gate.py -v`.
Public claims and package publication remain blocked until real evidence and
the rest of the PR-5 release checks pass.
