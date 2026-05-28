---
name: add-language-parser
description: Recipe for adding a new tree-sitter language parser to gitcortex-indexer. Mirrors the existing rust/python/typescript/go/java pattern. Use when extending GitCortex language support.
---

# Add Language Parser

GitCortex indexes a language via one parser file in `crates/gitcortex-indexer/src/parser/<lang>.rs`. Every language follows the same shape — copy the smallest existing one (`python.rs` or `go.rs`) as the template, do not invent a new structure.

## Files to touch (in order)

```
1. crates/gitcortex-indexer/Cargo.toml         # add tree-sitter-<lang> dep (workspace dep first)
2. Cargo.toml (workspace)                      # tree-sitter-<lang> = "x.y" under [workspace.dependencies]
3. crates/gitcortex-indexer/src/parser/<lang>.rs   # new — clone python.rs structure
4. crates/gitcortex-indexer/src/parser/queries/<lang>.scm  # tree-sitter queries (if file-based)
5. crates/gitcortex-indexer/src/parser/mod.rs  # register the language: extension map + dispatch
6. crates/gitcortex-indexer/src/parser/deftext.rs  # shared definition-text helpers if language needs custom slicing
7. .gitcortex/config.toml                       # add <lang> to [index].languages default
8. tests/integration/fixtures/<lang>/          # 2-3 small files exercising every NodeKind/EdgeKind the parser emits
9. README.md                                   # update language table
```

## What the parser must produce

For every supported `NodeKind` (Function, Method, Struct, Trait, Enum, Module, Constant, Macro, TypeAlias, File):
- `Node` with `name`, `qualified_path`, `kind`, `byte_range`, `line_range`, `metadata` (visibility, is_async, is_unsafe, loc).
- `qualified_path` format must match siblings — for namespaced langs use `module::sub::Name`, for path-based use `dir/file::Name`.

For every supported `EdgeKind`:
- `Contains` — always emit File→TopDef, Module→ChildDef, Struct→Method.
- `Calls` — call expression resolution. OK to emit unresolved (best-effort name match later in store).
- `Implements` — for langs with trait/interface concept.
- `Uses` — type references in fn params/returns.
- `Imports` — top-level import/use statements.

## Hard rules

- Parser is **sync**, no `tokio`, no `async`.
- Parser must be **deterministic** — same file in, same nodes out, in same order. Tests will pin this.
- No `unwrap()` — return `Result<ParseOutput, GitCortexError>`.
- No file I/O inside parser. Caller passes `&str` source. Parser is pure.
- Respect `max_file_size_kb` from config — bail early with a `Skipped` outcome, not an error.

## Verify

1. `cargo nextest run -p gitcortex-indexer` — parser unit tests pass.
2. Run gcx on the `tests/integration/fixtures/<lang>/` fixture: `gcx init && gcx query list-definitions <file>`. Every expected definition appears.
3. `cargo clippy -p gitcortex-indexer -- -D warnings` clean.
4. Update README language count (currently "5 — full edge coverage").

## Subagents

Use **rust-build-fixer** if `cargo check` fails after the parser file lands. Use **rust-reviewer** before commit.
