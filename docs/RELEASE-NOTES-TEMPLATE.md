# Release notes authoring template

Draft only. Replace placeholders and remove authoring instructions before
copying into CHANGELOG.md. The Pages release log is generated from that file.
Use its existing `## [version] - date` and `### section` conventions.

## Identity

Version, release date, tag/commit, supported platforms, and matching docs URL.
Do not supply a release date until the publication schedule is established.

## Highlights

Three or fewer user outcomes. State who benefits and what they can now do.

## Added / Changed / Fixed

Use applicable sections. For each substantive change explain the trigger and
result, with a short verified example or reference link when useful.

## Compatibility and upgrade

State CLI/MCP/config/schema changes and affected versions. Include tested
upgrade commands, daemon restart/reindex requirements, data preservation,
verification of success, and a rollback path. If no migration is needed, say so.

## Known limitations

Supported scope and important unresolved issues. Link to public issues when
available. Do not describe future roadmap features as shipped capabilities.

## Validation and evidence

Candidate identity, supported-platform checks, benchmark/report links and
limitations. Separate correctness results from performance measurements; report
failures and exclusions. Never reuse another version's results as this release's.

## Install and verify

Version-specific downloads or package commands, checksum/signature links where
available, `gcx --version`, and a tested first-use example. Link to full docs
instead of duplicating the entire reference manual in the release body.
