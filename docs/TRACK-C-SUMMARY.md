# Track C implementation summary — Java correctness

Implements Track C (items 4-7) from
`/Users/bharathvelamala/.claude/plans/prancy-leaping-sparrow.md`. All changes
are unstaged for review. No commits made, no destructive git ops.

## Context / motivation

GitCortex's own benchmark shows Java is the only supported language with
*negative* token savings (-2.2%) vs. a plain grep baseline. README's coverage
matrix already named the two concrete, known gaps behind this:

> Java member annotations & fields not modeled. `@Override` / `@SerializedName`
> on members and `static final` fields don't yet produce nodes/edges, so
> annotation-target and field-level queries are incomplete.

## Audit (item 4) — what was actually true in `java.rs` before this change

Read `crates/gitcortex-indexer/src/parser/java.rs` fully, alongside
`typescript.rs` and `python.rs` for the established pattern, plus
`crates/gitcortex-indexer/src/parser/mod.rs` (the `LanguageParser` trait /
`ParseResult` shape) and `indexer.rs` (how `deferred_annotated` gets resolved
and mirrored onto `node.metadata.annotations`).

Findings, checked against tree-sitter-java 0.23.5's `node-types.json`
directly (not assumed):

- Annotation extraction (`extract_annotation_uses`, reading `annotation` /
  `marker_annotation` children of a node's `modifiers`) **already existed**
  and was already wired up for classes, nested classes, interfaces, nested
  interfaces, and methods/constructors (`visit_method`). So `@Override` on a
  method was, in fact, already captured into `deferred_annotated` and (via
  `indexer.rs`'s generic `ann_by_id` mirroring) `node.metadata.annotations` —
  contrary to the literal README wording. What was genuinely missing:
  - **Enums and records never called `extract_annotation_uses` at all** — a
    real, confirmed gap (`visit_enum`, `visit_record`, `visit_record_nested`
    had no annotation extraction).
  - **Fields had zero node/annotation treatment of any kind.**
    `extract_field_uses` (the only field-handling code) emitted `Uses` edges
    for the field's declared type and nothing else — no `Node` was ever
    created for a field, static/final or not, so there was no id to attach an
    `Annotated` edge or `metadata.annotations` entry to even if extraction
    had been attempted. This is the real root of the "member annotations …
    not modeled" claim for fields specifically.
- `static final` fields: confirmed there is **no code path anywhere** in
  `java.rs` that creates a `Node` for any field. `field_declaration` (Java
  grammar: `modifiers? type declarator+`, where `declarator` is a repeatable
  field on `field_declaration` and each `variable_declarator` has a `name`
  field) was only ever used to walk into the declared `type` for `Uses`
  edges. Confirmed via tree-sitter-java's `src/node-types.json` that
  `field_declaration.declarator` is `multiple: true`, i.e. `children_by_field_name("declarator", ...)` is the correct traversal for `static final int A = 1, B = 2;`.

## Increment 1 (item 5) — annotations as node attributes

File: `crates/gitcortex-indexer/src/parser/java.rs`

- Added `self.extract_annotation_uses(node, &id)` calls to `visit_enum`,
  `visit_record`, and `visit_record_nested` — closing the confirmed gap where
  enum/record-level annotations (e.g. `@Deprecated` on an enum) were silently
  dropped, unlike classes/interfaces/methods which already had this.
- Did **not** invent a new node/edge convention — reused the existing
  `extract_annotation_uses` → `deferred_annotated` → `EdgeKind::Annotated` +
  `NodeMetadata.annotations` mechanism verbatim (the same mechanism
  `typescript.rs`'s `extract_decorator_annotated` and `python.rs`'s decorator
  handling already use), per the plan's explicit instruction to follow the
  established pattern rather than build a parallel one.
- Field-level annotations are handled as part of Increment 2 below, since
  attaching an annotation to a field requires a field `Node` to exist first —
  which didn't exist until this change.

Build after Increment 1: `cargo build -p gitcortex-indexer` — clean, no
warnings introduced.

## Increment 2 (item 6) — `static final` fields as Def symbols

File: `crates/gitcortex-indexer/src/parser/java.rs`

- Replaced `extract_field_uses` with `visit_field_declaration`, extending
  (not rewriting) the existing field-handling pattern:
  - Unchanged behavior: still emits `Uses` edges from the declared field type
    back to the container class, for every field regardless of modifiers.
  - New behavior: for fields whose `modifiers` text contains both `static`
    and `final`, iterates every `variable_declarator` under the field's
    `declarator` field (handles `static final int A = 1, B = 2;` correctly —
    two separate `Constant` nodes) and, per declarator:
    - Emits a `NodeKind::Constant` node via the existing `make_node` helper
      (same helper every other node kind in this file uses — correct
      `qualified_name`, `span`, `loc`, `is_static`/`is_final` metadata,
      `capture_definition` signature/body capture all come for free).
    - Emits a `Contains` edge from the containing class to the new constant
      node, matching `rust.rs`'s `visit_const` pattern (the closest existing
      analogue to a Java compile-time constant).
    - Calls `extract_annotation_uses` on the field, so `@Deprecated` /
      `@SerializedName` on a `static final` field is now captured exactly
      like it already is for methods/classes.
  - Plain (non-static-final) instance fields still get **no** Def node —
    intentionally unchanged scope, matching the literal README wording
    ("static final fields don't yet produce nodes/edges") rather than
    silently expanding scope to all fields.
- Updated both call sites (`visit_class`'s and `visit_class_nested`'s
  `field_declaration` match arms) to call `visit_field_declaration` with the
  correct scope (`class_scope` / `nested_scope`) so constant qualified names
  are correctly namespaced under their class.

Build after Increment 2: `cargo build -p gitcortex-indexer` — clean, no
warnings introduced.

## Tests added (item 5/6 verification, item 7 test coverage)

File: `crates/gitcortex-indexer/src/parser/java.rs`, `#[cfg(test)] mod tests`
— followed the existing per-language convention in this file exactly
(`parse()` / `parse_full()` helpers already present, assertion-based, no
snapshot framework used anywhere in this crate).

New tests:

- `detects_method_annotation` — `@Override` on a method still captured
  (regression guard for the pre-existing behavior, since it was
  undocumented/untested before this change).
- `detects_class_level_annotation` — `@Deprecated` on a class.
- `detects_enum_level_annotation` — `@Deprecated` on an enum (new coverage,
  Increment 1).
- `detects_record_level_annotation` — `@Deprecated` on a record (new
  coverage, Increment 1).
- `static_final_field_becomes_constant_node` — a `public static final int
  MAX_RETRIES = 3` field becomes exactly one `NodeKind::Constant` node with
  `is_static && is_final` metadata and a `Contains` edge from the class; a
  plain instance field (`instanceCounter`) produces no node at all, asserting
  the scope boundary is intentional and precise.
- `static_final_field_multi_declarator` — `static final int A = 1, B = 2;`
  produces two separate constant nodes (`A`, `B`), verifying the
  `children_by_field_name("declarator", …)` traversal handles the repeatable
  field correctly.
- `static_final_field_annotation_captured` — `@Deprecated` on a `static
  final` field is captured (Increment 1 + Increment 2 working together).

### Before/after test results

- Before: 8 existing tests in `parser::java::tests`, all passing.
- After: 14 tests in `parser::java::tests` (8 pre-existing + 6 new), all
  passing:
  ```
  test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 75 filtered out
  ```
- Full crate regression check: `cargo test -p gitcortex-indexer` —
  **89 passed; 0 failed** (includes all Rust/Python/TypeScript/Go/Markdown
  parser tests, unaffected by this change).
- `cargo build -p gitcortex-indexer` (the crate actually touched) — clean
  after both increments, no warnings introduced.
- `cargo build --workspace` — also confirmed clean (exit code 0,
  `Finished dev profile [unoptimized + debuginfo] target(s) in 8m 41s`,
  compiling all crates including `gitcortex-cli`, `gitcortex-mcp`,
  `gitcortex-viz`, `gitcortex-store` on top of the changed `gitcortex-indexer`
  — confirms no downstream break from the `java.rs` changes).

## Regression check (item 7) — SKIPPED, explicitly

The plan calls for `bash docs/benchmarks/real-harness.sh <gson-repo-url> ...`
against gson (pinned in `tools/agent-bench/suite.toml`:
`https://github.com/google/gson` @ `c9f3fd55854a743b66f857ace3c7b268ea3e2ef7`)
to confirm the -2.2% Java token-savings regression moves toward positive.

**I did not run this.** `real-harness.sh` invokes `claude -p` twice per
question against the real Anthropic API and costs real money (~$1-1.5 per
the plan's own estimate) on top of this already-flagged session (the
environment surfaced repeated "COST CRITICAL: session total over $50"
warnings throughout this task). Spending additional real API budget without
an explicit go-ahead for that specific spend felt like the wrong call to make
unilaterally, so I stopped short of running it rather than guess at (or
fabricate) a result.

**To run it**, from the repo root:

```bash
# Build the release gcx binary the harness invokes:
cargo build --release -p gitcortex-cli

bash docs/benchmarks/real-harness.sh \
  https://github.com/google/gson \
  docs/benchmarks/track-c-gson-result.json
```

(Defaults to `claude-haiku-4-5-20251001`, 7 questions, $1.50 budget cap per
the script; override via the 3rd/4th positional args and `BUDGET` env var if
a different model/question count is wanted.) The script clones gson into
`/tmp/gcx-bench/work/gson` and indexes it via `gcx init`, so both increments
above are exercised by the actual gson class hierarchy (constants, annotated
methods, etc.) rather than just the unit-test fixtures.

## Files touched

- `crates/gitcortex-indexer/src/parser/java.rs` — all Increment 1 + 2 code
  changes and new tests, as detailed above. No other files touched.

## What's explicitly out of scope / not done

- Annotations on enum constants (`enum_constant` nodes) and interface
  `constant_declaration` fields — grammar supports it (`enum_constant` has a
  `modifiers` child) but neither is mentioned in the README's stated gap and
  was left alone to keep this change scoped to the two named items.
- Def nodes for non-static-final instance fields — deliberately out of scope
  per the README's precise wording; would be a reasonable Track C follow-up
  but is a scope expansion beyond what was asked.
- The full `real-harness.sh` regression run — see above.
