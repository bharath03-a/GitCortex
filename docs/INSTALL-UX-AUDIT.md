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
| Hooks raced a long-running Kuzu owner | Lock errors on every Git operation | One server owns the graph; hooks delegate synchronization to its watcher |
| A running server stayed on its startup branch | Queries and semantic search could use stale branch state | Active branch, committed state, working-tree graph, and semantic revision now update while serving |
| Multiple MCP servers failed with a raw Kuzu error | Poor diagnosis and repeated lock contention | Advisory ownership lock reports the owning PID and the one-server rule |
| `clean`, purge, or init could touch an open database | Data loss or partial initialization | Destructive/initialization operations refuse while `gcx serve` is active |
| There was no rollback command | Users had to manually find generated files | `gcx deinit`, `--dry-run`, `--global-editor-config`, and `--purge` remove only GitCortex-owned integration |
| First index ran before `.gitcortex/ignore` existed | Build output could be indexed on first run | Exclusions are written before indexing; hooks are installed last |
| Fresh installs printed a schema-mismatch wipe warning | Normal setup looked destructive | Fresh stores initialize silently; only real mismatches warn |
| Durable IDs used unspecified `DefaultHasher` behavior | Future compiler updates could change storage identity | New repositories use a defined BLAKE3 ID; existing IDs remain discoverable |
| Models were stored as durable application data | Replaceable downloads polluted the data directory | Models use the platform cache directory and migrate from the legacy location |
| macOS used a Linux-specific default path | Non-native filesystem layout | New installs use native platform data/cache roots while preserving legacy stores |
| Update detection grouped pip, pipx, and uv | Suggested commands could install a second copy | Homebrew, Cargo, npm, pip, pipx, uv, and curl receive method-specific update commands |
| Installer filenames and compact-mode docs were stale | Copy/paste commands failed | Documentation now matches cargo-dist artifacts and current CLI flags |
| Homebrew was unavailable | Missing standard macOS installation path | cargo-dist now builds `gitcortex.rb` and release CI publishes it to the official tap |

## Intentional constraint

KuzuDB permits a single read-write process for a database. GitCortex therefore
supports one `gcx serve` owner per repository. While that server is active, its
watcher owns branch and index synchronization; a second server receives a clear
ownership error. Direct CLI commands that need to open the embedded store must
use the active editor MCP server or run after stopping `gcx serve`.

A future shared daemon could multiplex multiple MCP clients and CLI requests,
but this branch deliberately chooses explicit single-process ownership rather
than hiding lock races or risking concurrent database access.

## Homebrew activation prerequisite

Code and CI support are complete, but the repository owner must perform the
one-time external setup in [`HOMEBREW-RELEASE.md`](HOMEBREW-RELEASE.md): create
`bharath03-a/homebrew-tap` and add `HOMEBREW_TAP_TOKEN`. Until that repository
and secret exist, tagged releases cannot publish the generated formula.
