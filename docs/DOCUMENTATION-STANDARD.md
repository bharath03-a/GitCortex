# Documentation and release standard

Adopted 2026-10-20. Applies to every future release, including patches.
Documentation is part of the release candidate, not post-release cleanup.
This policy does not claim that the current site already satisfies every item.

## Patterns reviewed

- [uv](https://docs.astral.sh/uv/) separates first steps, task guides, concepts,
  integrations, reference, troubleshooting, and policies. Adopt that separation
  so beginners do not have to read the whole CLI reference to get started.
- [jq](https://jqlang.org/manual/) identifies the documented version and links to
  older and development manuals. Adopt explicit stable/development identity.
- [Zig downloads](https://ziglang.org/download/) groups version-specific notes,
  reference documentation, binaries, and verification information. Adopt a
  release landing page that connects these artifacts.
- [mdBook](https://rust-lang.github.io/mdBook/) demonstrates Markdown-based
  documentation with search, chapters, and keyboard navigation. It is a sensible
  candidate for our Rust project; adopting it is a separate build migration.

These are structural examples, not evidence that any project uses our release
process or that its implementation should be copied wholesale.

## GitHub Pages information architecture

Keep `/GitCortex/` as the product overview. Build a separate documentation area
under `/GitCortex/guide/`, retaining existing URLs or providing redirects.

| Area | What belongs there | Existing source to reuse |
|---|---|---|
| Start | Install, initialize, first useful query, verify readiness | QUICKSTART.md |
| Guides | Understand a repo, trace behavior, edit/refactor, review a PR, resume work | ADOPTION.md, PILOT-GUIDE.md, DEMO_SCRIPT.md |
| Concepts | Branch scope, freshness, confidence, static-analysis limits, local storage | REFERENCE.md, ARCHITECTURE.md |
| Reference | CLI, MCP, configuration, environment variables, schema | REFERENCE.md; generate supported surfaces where feasible |
| Troubleshooting | Missing/stale graph, locks, incompatible index, recovery | Doctor output and verified failure fixtures |
| Releases | Highlights, compatibility, upgrade/rollback, downloads, evidence | CHANGELOG.md, version-specific upgrade instructions |
| Contributing | Setup, architecture, tests, release process | CONTRIBUTING.md, DEV_SETUP.md, RELEASING.md |

Use a sidebar, search, per-page contents, accessible contrast and keyboard
navigation, mobile layout, copyable examples, and an edit-source link. Keep the
landing page concise; put exhaustive tool details in reference pages.

Stable docs must describe the latest published tag. Development docs must carry
an obvious unreleased banner. For future releases, retain versioned snapshots
and a stable alias; do not relabel main-branch documentation as v0.7.6. Any
historical snapshot must come from that release tag, with corrections identified.
An initial versioned-doc migration is pending; the current Pages site is built
from main and does not yet implement these stable/development guarantees.

Prefer one maintained Markdown source per topic. Do not add another manually
maintained HTML reference beside the existing Markdown one. Evaluate mdBook for
the guide while retaining the current homepage and generated release log. Before
switching generators, verify project-subpath URLs, search assets, existing links,
version handling and the CI build. Do not delay correctness fixes for a redesign.

## Required before tagging any release

- [ ] Version, release date, package metadata, homepage and download links agree.
- [ ] CHANGELOG.md explains user-visible changes; fixes include the triggering
      situation and corrected behavior, not only internal implementation names.
- [ ] CLI/MCP/config changes are reflected in reference and task guides, with
      flags, defaults, result envelopes, errors and supported scope checked.
- [ ] Quickstart and changed examples run against the exact candidate binary in
      a disposable repository; record platform, version and outcome.
- [ ] Breaking/schema changes include tested upgrade/reindex and rollback steps.
      Explicitly say when no migration is required.
- [ ] Known limitations, support matrix and storage/network behavior are accurate.
- [ ] Performance claims link to versioned evidence, methodology, failures and
      limitations. Historical numbers remain labeled with their actual versions.
- [ ] The site builds; internal links, anchors, assets and navigation work under
      the GitHub Pages `/GitCortex/` project prefix.
- [ ] Search, mobile and keyboard behavior receive a browser smoke when the site
      changes. Playwright work belongs to Luna per repository instructions.
- [ ] Release notes are reviewed before publishing; unresolved placeholders and
      unimplemented features are excluded from the public release entry.

Use `RELEASE-NOTES-TEMPLATE.md` as an authoring aid. Omit irrelevant sections,
but keep compatibility, upgrade requirements and known limitations explicit.
For docs-only changes, validate the renderer and links rather than rerunning
unrelated product benchmarks. Changes to documented behavior require matching
product verification. Record checks in the release PR; do not claim a manual
checklist is an automated gate.

## Publication and verification

Merge the reviewed candidate, tag its intended commit, publish artifacts and
version-matched documentation using RELEASING.md. Only identify a new release as
available after its artifacts can actually be installed. Check the public Pages
site, release notes, checksums, package links and quickstart after deployment.
Report partial publication explicitly if a channel fails.

Never move an existing published tag to newer code or overwrite its binaries to
reuse a version. Documentation corrections can be published as corrections;
product changes require a new version. Retain the prior release's docs and notes.
