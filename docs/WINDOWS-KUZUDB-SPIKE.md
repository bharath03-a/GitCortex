# Windows/KuzuDB Spike (Track E, items 13–15)

Investigation + spike only, per plan `prancy-leaping-sparrow.md`. This document does **not**
choose between path (a) and (b) — it gives Bharath the information to make that call.

## 1. What GitCortex actually pins

- `Cargo.toml:50` (workspace deps): `kuzu = "0.11"`.
- `Cargo.lock`: resolves to `kuzu 0.11.3`, `source = registry+https://github.com/rust-lang/crates.io-index`.
- `crates/gitcortex-store/Cargo.toml`: `kuzu` is an **optional** dependency behind the
  `kuzu-backend` feature, which is in `default = ["kuzu-backend"]`. No `arrow` or
  `extension_tests` crate features are enabled — just the default `kuzu` crate feature set,
  which pulls the embedded C++ engine and links it via the `kuzu` crate's own `build.rs`
  (which in turn shells out to `cmake`/`cc`, and links against the `cxx` bridge crate and a long
  list of statically-linked third-party libs: `antlr4_runtime`, `antlr4_cypher`, `re2`,
  `fastpfor`, `parquet`, `thrift`, `snappy`, `zstd`, `miniz`, `mbedtls`, `brotli*`, `lz4`,
  `roaring_bitmap`, `simsimd`, `utf8proc`, plus a `kuzu_rs` glue static lib).
- README's own two mentions of the pinned version **disagree with each other**: line 261-263
  says "KuzuDB 0.11.3 (upstream archived Oct 2025)"; the Limitations section at line 825 says
  "KuzuDB 0.6.3 (upstream archived)". `Cargo.lock` confirms **0.11.3** is the real pinned
  version — the 0.6.3 reference in Limitations is stale and should be corrected regardless of
  which path Bharath picks (this is a one-line doc fix, not part of this spike's scope, flagging
  it here only because I found it while cross-checking).

## 2. Upstream status (fresh research, beyond what README already says)

- **KuzuDB's GitHub repo (`kuzudb/kuzu`) was archived by the owner on 2025-10-10**, the same day
  `v0.11.3` shipped as (so far) the final release. Multiple independent sources confirm this and
  attribute it to Apple's acquisition of the company behind Kùzu (University of Waterloo spinout),
  with the team reportedly "working on something new." Coverage: [The Register, 2025-10-14](https://www.theregister.com/software/2025/10/14/kuzudb-graph-database-abandoned-community-mulls-options/1142229),
  [BigGo News](https://biggo.com/news/202510130126_KuzuDB-embedded-graph-database-archived).
  This is now a genuinely dead upstream, not just a project that's slow to fix Windows — no new
  commits, no maintainer response, repo is read-only.
- The repo is *archived*, not deleted — existing issues/PRs are still browsable via the GitHub
  API/`gh`, which is how the rest of this research was done.
- Community forks exist post-archival (e.g. `Vela-Engineering/kuzu`, `KeplerOps/kuzu`), but none
  has established itself as a de facto successor yet; no evidence any of them has landed a
  Windows/MSVC fix that GitCortex could pull in today.

## 3. The specific MSVC blocker (not just "doesn't link")

Sandbox cross-compilation caveat first: this machine is `aarch64-apple-darwin`. I added the
`x86_64-pc-windows-msvc` Rust target via `rustup target add x86_64-pc-windows-msvc
--toolchain 1.95-aarch64-apple-darwin` (the repo pins `channel = "1.95"` in
`rust-toolchain.toml`, which needed the target added separately from the default `stable`
toolchain) and ran `cargo check --target x86_64-pc-windows-msvc -p gitcortex-store`. This
confirmed the target's Rust std library is fetchable, but the build failed immediately in the
`link-cplusplus` crate's build script with:

```
error occurred in cc-rs: failed to find tool "lib.exe": No such file or directory (os error 2)
```

That's expected and not the real signal — `lib.exe` is the MSVC librarian, part of Visual Studio
Build Tools, which does not exist on macOS. **This sandbox cannot reach the actual KuzuDB C++
link stage** (no MSVC linker/librarian available at all), so I cannot personally reproduce
GitCortex's README-cited LNK1169/LNK2038 with this exact 0.11.3 pin. That is the honest limit of
what a cross-compile attempt from this machine can show. The rest of this section is the deepest
research pass available in place of a real reproduction, using `kuzudb/kuzu`'s own issue tracker
(searched via `gh api search/issues`) for the exact failure class GitCortex is citing.

**Root cause class, confirmed from multiple upstream issues, all specific to the Rust binding on
Windows/MSVC:**

KuzuDB's C++ core and its Rust crate (`tools/rust_api`, published as the `kuzu` crate) get built
by `cc`/`cmake` as part of the crate's own `build.rs`. On MSVC, the C/C++ runtime library choice
(`/MD` dynamic release, `/MDd` dynamic debug, `/MT` static release, `/MTd` static debug) must
match exactly across every object file being linked into the same binary, or the linker fails
with `LNK2038: mismatch detected for 'RuntimeLibrary'` — this is the precise mechanism behind the
"LNK2038 symbol conflicts" GitCortex's README already names. LNK1169 ("one or more multiply
defined symbols found") is the typical companion when a static-lib/CRT mismatch also produces
duplicate symbol resolution. Evidence this is a real, repeated failure mode for this exact
codebase (not a one-off):

- [kuzudb/kuzu#4553](https://github.com/kuzudb/kuzu/issues/4553) — filed against the
  `Makefile`'s Windows rust-build rule, which was hardcoding `/MDd` (**debug** CRT) even for
  release builds. Debug vs. release CRT is exactly the kind of mismatch that throws LNK2038 when
  something else in the link (a dependency, or the calling Rust/`cxx` binary) was built against
  the release CRT.
- [kuzudb/kuzu#3226](https://github.com/kuzudb/kuzu/issues/3226) ("Always build rust integration
  with release runtime library on Windows", **merged**) — a fix for the same class of problem
  filed as #3225. Confirms the CRT-linkage-mismatch bug was real and was patched at least once,
  meaning it had already resurfaced/needed re-fixing by the time later issues (like #4553, filed
  after this merge) were opened — i.e. this was not a single fixed-and-done issue, it's a
  recurring category.
- [kuzudb/kuzu#4796](https://github.com/kuzudb/kuzu/issues/4796) — "Fix compatibility with older
  version of MSVC runtime," another CRT-version-mismatch report.
- [kuzudb/kuzu#2521](https://github.com/kuzudb/kuzu/issues/2521) — "Windows: C++ ABI stability?"
  — a maintainer/user flags that the C++ API (e.g. `Database(std::string databasePath)`) is only
  ABI-stable if compiled with the *exact same compiler and settings* as the consuming binary,
  because MSVC's `std::string` ABI is not stable across compiler/CRT-setting combinations. This
  is a structural property of MSVC C++, not a bug KuzuDB introduced, but it means any prebuilt
  static/shared KuzuDB lib is fragile against whatever toolchain/CRT setting the Rust build picks
  for the rest of the binary.
- [kuzudb/kuzu#2527](https://github.com/kuzudb/kuzu/issues/2527) — a related but distinct
  Windows-specific failure: C API functions that allocate strings on the KuzuDB DLL's heap crash
  when freed with the caller's `free()`, because "different heaps in application and dll" on
  Windows (Linux shares a single CRT heap by default; Windows mostly doesn't). Confirms the
  Windows DLL boundary is a second, independent hazard beyond the static-link CRT mismatch.
- [kuzudb/kuzu#6039](https://github.com/kuzudb/kuzu/issues/6039) (**open**, filed 2025-09-30, 11
  days before archival) — confirms KuzuDB's GitHub release artifacts ship **no static library at
  all**, only headers + shared lib; anyone wanting a static link (which is what GitCortex's
  `Cargo.lock`-resolved build does via `-l 'static:+whole-archive=kuzu'` etc.) must build KuzuDB's
  C++ core from source themselves, pulling in the full CRT-linkage-matching problem above. A
  maintainer reply confirms this and points at `tools/rust_api/build.rs` as the piece that
  assembles ~15 static third-party libs (`antlr4_runtime`, `re2`, `snappy`, `zstd`, etc.) that all
  have to agree on CRT settings for the final link to succeed on MSVC.

**Summary of the blocker:** it is not one bug but a structural mismatch between (1) MSVC's
requirement that every statically-linked object in a binary share identical CRT linkage
(`/MD`/`/MT`, debug/release), (2) KuzuDB's `build.rs`/`Makefile` build path for the Rust crate,
which has repeatedly picked the wrong CRT setting (debug runtime in release builds, per #4553,
even after one fix in #3226), and (3) KuzuDB shipping no prebuilt static Windows artifact, so any
consumer building from source inherits whatever CRT-setting bugs exist in that build path at the
time. With the repo archived since 2025-10-10 and v0.11.3 as the last release, none of these
open/recurring issues (#2521, #4796, #6039) will get further upstream attention.

## 4. Rough effort for path (a) vs path (b)

Framed as inputs to Bharath's decision, not a recommendation to start either:

**(a) Upstream fix or fork of KuzuDB for MSVC**
- Requires forking `kuzudb/kuzu` (archived, so no upstream PR path — a fork is now the *only*
  option, there's no "get it merged upstream" anymore).
- Requires C++/CMake work: fix CRT linkage consistency across `tools/rust_api`'s own build script
  and however many of the ~15 static third-party libs turn out to be affected, verified on a real
  Windows/MSVC machine (this sandbox cannot validate any of it, per Section 3).
- Ongoing maintenance cost: GitCortex would now own a KuzuDB fork indefinitely — every future
  KuzuDB version bump (if any community fork produces one worth tracking) means re-applying or
  re-verifying the Windows patch.
- Effort: multi-week, C++-build-system-literate work, plus recurring maintenance tax. Highest
  ceiling (keeps full graph-native depth on Windows) but highest and longest-tail cost.

**(b) Second `GraphStore`-trait-backed store for Windows only**
- The `GraphStore` trait boundary already exists (README: "the local KuzuDB backend can be
  swapped for a remote backend without touching the indexer or MCP layer" — `crates/gitcortex-store`
  already ships a `memory` feature stub with no KuzuDB link, confirming the seam is real and
  already exercised, not hypothetical).
- Requires picking and implementing a Windows-native graph-capable store (e.g. SQLite+recursive
  CTEs à la CodeGraph's own approach per the plan's Track E framing, or `redb`/`sled` +
  hand-rolled traversal) behind the same trait, then wiring it into `gcx init`/CLI feature
  detection so Windows builds pick it automatically.
- Accepts reduced graph-native query depth on Windows specifically (the plan already flags this
  tradeoff) — needs the deferred CodeGraph benchmark's read on how much that depth matters in
  practice, which is explicitly not decided yet.
- Effort: bounded, ordinary Rust work reusing an established internal seam — no C++/MSVC
  toolchain expertise required, no forked-dependency maintenance tax. Lower ceiling, much lower
  and more predictable cost.

## 5. Recommendation (for Bharath's decision, not implemented here)

Path (b) looks more tractable given what this spike found, for three reasons: (1) path (a) no
longer has an upstream fix path — the archival means it is unconditionally a fork-and-maintain
commitment, not a wait-for-a-patch situation; (2) the specific blocker (MSVC CRT-linkage
consistency across a from-source build with no prebuilt static artifact) is a category KuzuDB
itself struggled to keep fixed even while actively maintained (#3226 fixed it once, #4553
reported it again later); (3) the `GraphStore` trait seam for (b) is already built and already
proven with the existing `memory` stub feature, so (b)'s incremental engineering surface is
smaller than it would be starting from nothing.

This is not a decision — it's the tradeoff data the plan's Section 15 asked for before either
path gets picked. The deferred CodeGraph benchmark (how much graph-native depth actually matters
vs. CodeGraph's SQLite+FTS5 approach) is still the more decisive input for whether the effort is
worth it at all, on either path.
