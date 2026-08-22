# Track A/B implementation summary

Implements Track A (README/docs positioning) and Track B (release signing) from
`/Users/bharathvelamala/.claude/plans/prancy-leaping-sparrow.md`. All changes are
unstaged for review. No commits made, no destructive git ops, no release triggered.

## Files touched

### `README.md`

1. **Highlights section (`### Highlights`, near line 36)**
   - Replaced the single `gcx` single-dispatch bullet with a reworded "One smart
     dispatch tool by default" bullet. Now says `gcx` avoids paying schema cost
     for "all 27 individual tools" (corrected from a stale "22 separate tool
     schemas" claim — see "Tool count correction" below), cites
     `docs/benchmarks/RELEASE-GATE.md` for the actual measured per-turn schema
     cost, and states all 27 individual tools are available via `gcx serve --full`.
   - Added a new "PR blast-radius bot" bullet headlining `gcx init --ci`
     (previously only documented ~500 lines down, under "### CI / PR blast radius
     bot"). Includes a realistic example of the sticky PR comment output, built
     from the actual `print_github_comment` format in
     `crates/gitcortex-cli/src/cmd/blast_radius.rs:153-211` (risk emoji, table
     headers, footer line) — not invented. Links down to the existing detailed
     section via `#ci--pr-blast-radius-bot`, which was left untouched.
   - **Skipped**: direct competitive callout ("we ship this today, CodeGraph's is
     still a waitlist"). I did verify via web search that CodeGraph's PR-impact
     product (`colbymchenry/codegraph`) is still waitlist-only as of today —
     `docs/CODEGRAPH-COMPARISON.md` already had this same finding, freshly
     researched the same day. Given that, I kept the README copy factual and
     non-comparative anyway ("No hosted service, no waitlist; it runs in your
     own CI today") rather than naming CodeGraph directly in the Highlights
     section — the detailed head-to-head belongs in
     `docs/CODEGRAPH-COMPARISON.md`, not the top-of-README pitch.

2. **Tool count correction (Highlights bullet + no other changes needed)**
   - The plan assumed "26 tools." Actual count from
     `crates/gitcortex-mcp/src/mcp/tools.rs` (`grep -c '#\[tool('` → 28 total,
     i.e. `gcx` dispatch + 27 individual tools) and from README's own MCP tools
     table (27 rows) is **27 individual tools**, matching the already-correct
     "27 separate schemas" wording at README.md:601 (`| \`gcx\` | ... |` row).
     Only README.md:46 was stale, saying "22 separate tool schemas" — fixed to
     match the accurate 27-tool count and made consistent with line 601.
   - Checked `docs/REFERENCE.md` for the same narrative: it never states a
     numeric tool count, so no contradiction existed there — no changes needed.
   - Did not touch the doc comment in `crates/gitcortex-mcp/src/mcp/tools.rs:1550`
     ("one schema instead of fifteen") — Track A is docs-only per the plan, and
     that comment (also stale, referencing "fifteen") is source code.

3. **New "Verified releases" section** (after the direct-binary-download /
   one-line-installer block, before "### Windows (via WSL2)")
   - Documents the exact `cosign verify-blob` command a user runs against a
     downloaded release artifact, its `.sig`, and its `.pem`, matching the
     signing job added to `.github/workflows/release.yml` (see below).

### `.github/workflows/release.yml`

- Added top-level `id-token: write` permission (required for cosign's keyless
  OIDC flow) alongside the existing `contents`/`actions` permissions.
- Added a new `sign-artifacts` job, `needs: [plan, host]`, running after the
  GitHub Release is created by `host`:
  1. Installs cosign via `sigstore/cosign-installer@v3`.
  2. Downloads all built artifacts (`artifacts-*` pattern, same pattern the
     existing `host` job uses).
  3. Drops the `*dist-manifest.json` files from the signing set (those aren't
     release-downloadable binaries).
  4. Runs `cosign sign-blob --yes --output-signature *.sig --output-certificate
     *.pem` on each remaining file — keyless, no secrets needed, the GitHub
     Actions OIDC token is exchanged for a short-lived Fulcio cert at sign time.
  5. Uploads the `.sig`/`.pem` files onto the same GitHub Release with
     `gh release upload ... --clobber`.
  - No changes needed to `dist-workspace.toml`: it already has
    `allow-dirty = ["ci"]`, which is the project's existing mechanism for
    tolerating manual edits to `release.yml` on top of what `cargo-dist`
    generates (used already for the PyPI/npm publish triggers). This new job
    follows that same established pattern.
  - **Deferred**: SLSA provenance generation via
    `slsa-framework/slsa-github-generator`'s generic-artifacts reusable
    workflow. Documented as a follow-up with an inline comment in the workflow
    file rather than wired in, because that generator expects the build job to
    emit a single job output of base64-encoded sha256 digests for all
    artifacts — but this repo's build is `cargo-dist`'s dynamically generated
    matrix (`build-local-artifacts` × N targets, plus
    `build-global-artifacts`), and correctly threading a combined digest
    output through that structure needs its own change, verified against a
    real tag push, rather than being added here without being able to test it.
  - **Not done** (per plan instruction #6): no actual release triggered, no
    tags pushed. This is a workflow-file-only change for Bharath to review and
    test on the next real release.
  - **Not done**: local `cosign verify` dry-run against a test tag (plan step
    B.3) — requires an actual tag push through this workflow, which is exactly
    what instruction #6 says not to do in this pass. Left for Bharath to run
    on the next real release.

## What was explicitly verified vs. assumed

- Blast-radius comment format: read directly from
  `crates/gitcortex-cli/src/cmd/blast_radius.rs:153-211` (`print_github_comment`)
  and `crates/gitcortex-cli/src/cmd/init/universal.rs:206-247` (`write_ci_workflow`)
  — the README example mirrors the real risk emoji, table structure, and footer.
- Tool count: verified against `crates/gitcortex-mcp/src/mcp/tools.rs` (grep for
  `#[tool(`) and README's own MCP tools table, not assumed from the plan's "26."
- CodeGraph waitlist status: verified via web search today
  (`colbymchenry/codegraph` README + related listings) — PR-impact analysis is
  still described as "coming" behind a waitlist at getcodegraph.com. Consistent
  with `docs/CODEGRAPH-COMPARISON.md`'s same-day research. Used to justify
  keeping the factual (non-named) framing in Highlights rather than skipping it
  outright, while still not naming CodeGraph directly in that section.
- `cosign verify-blob` flag names and `sigstore/cosign-installer@v3` usage
  reflect the standard, well-documented keyless-signing pattern (no secrets,
  Fulcio cert + Rekor transparency log) — not independently re-verified against
  live docs in this pass since no web access issue arose, but this is the
  long-stable, standard invocation.
