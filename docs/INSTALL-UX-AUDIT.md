# Installation and initialization usability audit

This audit covers package installation, `gcx init`, editor registration, Git
hooks, storage, cleanup, updates, and release packaging.

## Corrected in this branch

| Previous behavior | Risk | Resolution |
|---|---|---|
| `gcx init` configured every editor when detection failed | Surprising repository and global mutations | Editor setup is now opt-in through `--editor`; `--editor auto` explicitly requests detection |
| `.claude/CLAUDE.md` was always created | Claude-specific instructions appeared for non-Claude users | Claude files are written only for `--editor claude` or explicit `auto` detection |
| Global MCP files were changed as part of normal editor setup | Cross-project side effects | Global changes require `--global-editor-config` |
| Malformed or non-object JSON could be silently replaced | User configuration loss | Existing JSON is validated and updates fail closed |
| Shared editor JSON was written in place | Interrupted writes could corrupt configuration | Shared configuration uses atomic replacement and retains permissions |
| Codex was configured with the removed `--compact` flag | MCP startup failed | Compact mode is now correctly represented by plain `gcx serve`; old configs migrate on init |
| Codex received a GitCortex-repository-specific `AGENTS.md` template | Incorrect instructions in unrelated repositories | Template is now short, generic, marked, and migratable |
| Copilot received instructions but no MCP registration | Feature appeared enabled but was unavailable | `--editor copilot` now writes repository-local `.vscode/mcp.json` |
| Hook installation assumed `.git/hooks` | Failed with `core.hooksPath` and linked worktrees | Hook paths come from `git rev-parse --git-path hooks`; external shared paths require `--shared-git-hooks` |
| Hook failures could block commits and checkouts | Git operations depended on graph availability | Managed hook blocks are non-blocking and preserve existing hook content |
| Hooks raced a long-running Kuzu owner | Lock errors on every Git operation | The repository daemon owns the graph; hooks delegate synchronization to its watcher |
| A running server stayed on its startup branch | Queries and semantic search could use stale branch state | Active branch, committed state, working-tree graph, and semantic revision now update while serving |
| Multiple MCP servers failed with a raw Kuzu error | Users could not open one repository in multiple editors | Stdio proxies share one user-only repository daemon; compact and full clients run concurrently |
| `clean`, purge, or init could race an open database between check and mutation | Data loss or partial initialization | Every graph-opening process and destructive operation holds the same OS-released repository lock for its full lifetime |
| There was no rollback command | Users had to manually find generated files | `gcx deinit`, `--dry-run`, `--global-editor-config`, and `--purge` remove only GitCortex-owned integration |
| First index ran before `.gitcortex/ignore` existed | Build output could be indexed on first run | Exclusions are written before indexing; hooks are installed last |
| Fresh installs printed a schema-mismatch wipe warning | Normal setup looked destructive | Fresh stores initialize silently; only real mismatches warn |
| Any graph open could wipe an incompatible schema before acquiring Kuzu's lock | A query or older hook could delete data as a side effect | Only explicit, ownership-checked `gcx init` may rebuild an incompatible local index; ordinary opens fail with recovery guidance |
| Durable IDs used unspecified `DefaultHasher` behavior | Future compiler updates could change storage identity | New repositories use a defined BLAKE3 ID; existing IDs remain discoverable |
| Models were stored as durable application data | Replaceable downloads polluted the data directory | Models use the platform cache directory and migrate from the legacy location |
| macOS used a Linux-specific default path | Non-native filesystem layout | New installs use native platform data/cache roots while preserving legacy stores |
| Update detection grouped pip, pipx, and uv | Suggested commands could install a second copy | Homebrew, Cargo, npm, pip, pipx, uv, and curl receive method-specific update commands |
| Installer filenames and compact-mode docs were stale | Copy/paste commands failed | Documentation now matches cargo-dist artifacts and current CLI flags |
| Homebrew was unavailable | Missing standard macOS installation path | cargo-dist now builds `gitcortex.rb` and release CI publishes it to the official tap |

## Repository daemon and remaining embedded-store boundary

KuzuDB permits a single read-write process for a database. GitCortex now starts
one short-lived, machine-local daemon per repository and makes each `gcx serve`
process a stdio proxy. Multiple editors can connect simultaneously, including a
mix of compact and full tool-schema clients, while the watcher and semantic
indexer run only once. The Unix socket is repository-scoped, mode `0600`, and is
removed after the final client disconnects.

One embedded-store boundary remains: one-shot CLI and visualization commands
still open KuzuDB directly. Run those after editor MCP clients disconnect; the
daemon releases ownership within a short idle grace period. Routing every CLI
output format through the daemon would require a versioned internal query
protocol and is tracked separately from MCP client multiplexing.

## Homebrew activation prerequisite

Code and CI support are complete, but the repository owner must perform the
one-time external setup in [`HOMEBREW-RELEASE.md`](HOMEBREW-RELEASE.md): create
`bharath03-a/homebrew-tap` and add `HOMEBREW_TAP_TOKEN`. Until that repository
and secret exist, tagged releases cannot publish the generated formula.
