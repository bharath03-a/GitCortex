# Track D implementation summary — Framework/route-awareness

Implements Track D (items 8-12) from
`/Users/bharathvelamala/.claude/plans/prancy-leaping-sparrow.md`. All changes
are unstaged for review. No commits made, no destructive git ops.

Builds on top of Track C's Java changes (`crates/gitcortex-indexer/src/parser/java.rs`,
already modified on disk, untouched by this work) — this track touches
different files (`schema.rs`, `typescript.rs`, `mcp/tools.rs`, plus a few
one-line exhaustive-match fixups forced by the new `NodeKind`/`EdgeKind`
variants) and follows the same conventions Track C used: reuse existing
mechanisms (`make_node`, `Edge` construction, deferred-name resolution via
`fn_index`) rather than inventing parallel ones.

## Context / motivation

`docs/CODEGRAPH-COMPARISON.md` identifies framework-aware route detection as
the one capability colbymchenry/codegraph has that GitCortex fully lacked —
confirmed by grep: no `route` node kind existed in any parser or in the
schema (`crates/gitcortex-core/src/schema.rs`). This track closes that gap
for a single framework (Express.js), per the plan's explicit "one framework
per increment" scoping.

## Schema design (item 8)

**`NodeKind::Route`** (`crates/gitcortex-core/src/schema.rs`)

- Serializes as `"route"` (snake_case, matching every other kind).
- No dedicated attribute fields were added to `Node`/`NodeMetadata` — the
  schema has no generic key/value attribute bag on `Node` (only the
  established fixed fields: `is_async`, `is_static`, `annotations`, etc.),
  so adding one just for routes would be a wider schema change than this
  track's scope. Instead, following how every other kind encodes its
  identity in `name`/`qualified_name` (e.g. a `Constant`'s name *is* its
  identifier), a route's `name` and `qualified_name` are both
  `"METHOD /path"`, e.g. `"GET /users/:id"`. This keeps the path fully
  queryable through every existing tool/field that already reads `name` —
  `ast_search`'s `name_contains` filter, `list_definitions`, exports, etc. —
  with zero special-casing needed anywhere that handles nodes generically.

**`EdgeKind::HandledBy`** (`Route -[HandledBy]-> Function/Method`)

- Directionality follows the existing `Calls`/`Implements` convention
  exactly: the edge points from the "declaration" (the route registration)
  to what satisfies it (the handler), the same way `Implements` points
  Struct→Interface (the implementor → what it implements) and `Calls` points
  caller→callee. `Route -[HandledBy]-> Function` reads the same way: "this
  route is handled by this function."
- Serializes as `"handled_by"`.

**Store-layer registration** (read: `crates/gitcortex-core/src/graph.rs`,
`crates/gitcortex-store/src/kuzu/`)

Audited the storage layer the same way Track C's summary describes auditing
`java.rs` — checked whether `NodeKind`/`EdgeKind` need explicit registration
anywhere beyond the enum itself:

- **Node kind**: fully generic. KuzuDB stores `kind` as a string column
  (`node.kind.to_string()` on write); the *only* read-path decode is
  `conv::kind_from_str`, which is `s.parse().unwrap_or(NodeKind::Function)` —
  driven entirely by `NodeKind`'s `FromStr` impl in `schema.rs`. Adding
  `"route"` to that `FromStr` (done) was the only store-side change needed
  for nodes. There is one node table per branch (not one table per kind), so
  no table/schema migration was needed either — confirmed
  `SCHEMA_VERSION` did not need bumping.
- **Edge kind**: *not* fully generic — this was the one real trap. Writes
  use `edge.kind.to_string()` (generic, via `Display`), but reads go through
  `conv::edge_kind_from_str`, a **hand-written match with a silent fallback
  to `EdgeKind::Contains`** for anything unrecognized. Without adding an arm
  for `"handled_by"` there, every `HandledBy` edge would round-trip through
  the store and silently come back mislabeled as `Contains` — a real data
  correctness bug, not a compile error, so it would not have been caught by
  the build. Added the `"handled_by" => EdgeKind::HandledBy` arm to
  `crates/gitcortex-store/src/kuzu/conv.rs::edge_kind_from_str`.
- **Exhaustive `match` fallout**: adding a new `NodeKind` variant broke two
  *exhaustive* (no wildcard) matches found by build error, both cosmetic
  (colour palettes, not data-path logic) — fixed both:
  - `crates/gitcortex-cli/src/style.rs::kind_color` — added
    `NodeKind::Route => AnsiColor::BrightMagenta`.
  - `crates/gitcortex-viz/src/lib.rs::kind_dot_color` — added
    `NodeKind::Route => "#f2cdcd"` (also added `"route"` to the viz's
    `parse_node_kind` string→kind match, which already had a `_ => None`
    fallback so wasn't strictly required, but keeps the CLI/viz filter UI
    consistent with the new kind). Every other `NodeKind`/`EdgeKind` match
    site in the workspace (`gitcortex-mcp`, `gitcortex-cli/cmd/*`,
    `gitcortex-indexer/indexer.rs`) already used `matches!`, an explicit
    subset check, or a `_ =>` wildcard, so needed no changes.

## Increment 1 (item 9) — Express.js route detection

File: `crates/gitcortex-indexer/src/parser/typescript.rs`

- New pass `collect_routes`, invoked once from `parse_source` alongside the
  existing `collect_names` / `visit_program` / `collect_imports` passes.
  It's a **separate pass from `collect_calls`**, not a reuse of it, for a
  concrete reason: `collect_calls` is only ever invoked on a function/method
  *body* (its signature takes a `caller_id`), but Express route
  registrations (`app.get('/x', handler)`) are overwhelmingly **top-level
  statements**, not inside any indexed function. `collect_routes` instead
  mirrors `collect_calls`'s *shape* — the same recursive-descent,
  visit-every-`call_expression` traversal pattern — walking the whole
  program tree (`tree.root_node()`), which is the closest match to "follow
  the existing call-expression visiting pattern" the task called for without
  silently missing top-level route setup.
- `try_route` matches `<object>.<verb>('<path>', ...args)`: the call's
  `function` field must be a `member_expression`, its `property` must be one
  of `get/post/put/delete/patch/head/options/all` (case-insensitive), and
  the first argument must be a `string` node (template literals / dynamic
  paths are explicitly out of scope, matching the plan's "string-literal
  first argument" instruction). This intentionally doesn't check the
  object's name (`app`/`router`/anything) — Express conventionally uses
  either, and constraining to specific identifier names would miss
  `const server = express(); server.get(...)`.
- Path extraction strips the surrounding quote characters
  (`string_literal_value`); no escape-sequence unescaping is done (out of
  scope — real Express paths essentially never need it).
- Handler resolution (`resolve_handler`) covers three shapes actually seen
  in real Express code:
  - **Named function reference** (`app.get('/x', listUsers)`) — resolved
    against `fn_index`, the same map `collect_calls`/`record_call` already
    use for intra-file call resolution. `fn_index` is fully populated by
    `collect_names` (pass 1) before `collect_routes` (pass 3) runs, so this
    is a synchronous, always-correct lookup — no deferred/cross-file
    resolution was added for this increment (see "Left out" below).
  - **Bound method reference** (`router.put('/x', ctrl.update)`) — resolves
    the `.property` name against the same `fn_index` (methods share the
    index with functions, matching existing behavior elsewhere in this
    file).
  - **Inline handler** (`app.get('/x', (req, res) => {...})`) — gets a
    synthetic anonymous `NodeKind::Function` node (name `"<route handler>"`)
    built via the same `make_node` helper every other node in this file
    uses, with its body run back through `collect_calls` so calls made
    *inside* an inline handler still get indexed. This guarantees the
    `HandledBy` edge always lands on a real node, per the task's "linked via
    the new edge to the handler function symbol" requirement, even when
    there's no named symbol to link to.
- Middleware arguments (`router.put('/x', auth, handler)`) are handled by
  taking the **last** argument as the handler — Express's own calling
  convention (`(...middlewares, handler)`), verified by the
  `detects_express_router_with_middleware_and_inline_handler` test below.

## Increment 2 (item 10) — surfaced through `ast_search`, not a new tool

Extended **`ast_search`** (`crates/gitcortex-mcp/src/mcp/tools.rs`) rather
than `list_definitions` or a new tool:

- `ast_search`'s `kind` filter (`AttributeFilter.kind: Option<NodeKind>`,
  `crates/gitcortex-core/src/store.rs`) already dispatches purely on
  `node.kind == k` with **zero per-kind special-casing** in
  `AttributeFilter::matches` or in `store.search_by_attributes`. Adding
  `"route" => Ok(NodeKind::Route)` to `NodeKind::FromStr` (schema-level, one
  place) was the *entire* change needed to make `kind='route'` work as a
  filter — no new branch, no new query path.
- `list_definitions` was the other candidate, but it's keyed on `file` (list
  everything defined in one file) — a reasonable place to *see* routes
  incidentally, but it doesn't answer "show me all routes in the repo,"
  which is what a route-awareness feature needs to be useful for a PR/impact
  or architecture-overview use case. `ast_search`'s attribute-driven,
  cross-file design matches the actual use case directly.
- Since a route's full identity is already carried in `name`/`qualified_name`
  (see schema design above), `ast_search`'s existing result-mapping closure
  (`kind`, `name`, `qualified_name`, `file`, `start_line`, `visibility`,
  `is_async`, `complexity`, `annotations`) needed **no new field** to surface
  the path — `kind='route'` results already show `name: "GET /users/:id"`.
  This is the "minimal special-casing" the task asked to prefer.
- Changed, both purely additive:
  - The `ast_search` tool description now mentions `kind='route'` and
    documents that `name` is `"METHOD /path"` for route results.
  - The "unknown kind" error message's list of valid kinds now includes
    `route`.

Handler linkage (the `HandledBy` edge) is not separately surfaced through
`ast_search` — the edge is queryable via other existing tools that already
walk edges generically (e.g. `symbol_context`, `trace_path`), consistent
with how e.g. `Annotated` edges aren't special-cased in `ast_search` either
(they're exposed as the `annotations` list on the node, and the edge itself
is walkable elsewhere). No new code was needed for this since edge querying
was already generic.

## Increment 3 (item 11) — tests

File: `crates/gitcortex-indexer/src/parser/typescript.rs`, `#[cfg(test)]
mod tests` — followed the file's existing convention exactly (inline source
strings via `parse_js`/`parse_ts` helpers, assertion-based, no fixture
files or snapshot framework, matching every other test in this file and
Track C's approach in `java.rs`).

- **`detects_express_routes_with_named_handlers`** — a small Express app
  with `require('express')`, 3 named handler functions, and 3 routes
  (`app.get('/users', listUsers)`, `app.post('/users', createUser)`,
  `app.get('/users/:id', getUser)`). Asserts:
  - Exactly 3 `Route` nodes, with the correct `"METHOD /path"` names.
  - Exactly 3 `HandledBy` edges (one per route).
  - The `GET /users` route's edge points at the actual `listUsers` function
    node's id (not just "some edge exists") — verifies handler linkage is
    correct, not just present.
- **`detects_express_router_with_middleware_and_inline_handler`** — a
  `router.put('/users/:id', auth, (req, res) => {...})` registration.
  Asserts exactly 1 route (`PUT /users/:id`), that the middleware argument
  is correctly skipped in favor of the trailing inline handler, and that the
  inline handler resolves to a synthetic `Function` node (verifying the
  "always lands on a real symbol" guarantee above).

### Test results

- `cargo build -p gitcortex-indexer` — clean, no warnings.
- `cargo test -p gitcortex-indexer` — **91 passed, 0 failed** (89 pre-existing
  + the 2 new route tests above; includes all Rust/Python/TypeScript/Go/Java/
  Markdown parser tests, confirming no regression from either this track or
  Track C's already-landed Java changes).
- `cargo build -p gitcortex-mcp` — clean, no warnings (also exercises
  `gitcortex-core`, `gitcortex-store`, `gitcortex-indexer`, `gitcortex-cli`,
  `gitcortex-viz` as transitive dependencies, confirming the schema change
  and the two exhaustive-match fixups compile clean workspace-wide).
- `cargo test -p gitcortex-mcp` — offline test suite, no network/API calls,
  safe to run without budget concerns (unlike Track C's deferred
  `real-harness.sh`). **56 unit tests + 15 `full_pipeline.rs` integration
  tests, 0 failed** — confirms the `ast_search`/schema changes didn't
  regress anything in the MCP layer (search ranking, symbol context, tour
  generation, blast-radius, etc., all of which pattern-match on `NodeKind`).

## What's explicitly left for future increments (item 12, out of scope here)

Per the plan's explicit instruction ("Increment 4+: expand to FastAPI
(`python.rs`), then others, one framework per increment — do not attempt all
frameworks in one change"), the following are **not** implemented:

- FastAPI / Flask / Django route detection in `python.rs`.
- Any other framework (NestJS decorators, Spring `@RequestMapping`, Go
  `net/http`/gin/echo routers, Rust `axum`/`actix` route macros, etc.).
- Template-literal or regex-based Express paths (`app.get(\`/users/${id}\`,
  ...)`)  — only string-literal paths are modeled, per the task's explicit
  scope.
- Cross-file handler resolution — if a route's handler is imported from
  another module (`import { listUsers } from './handlers'; app.get('/x',
  listUsers)`), the identifier won't be in this file's `fn_index` and the
  `Route` node is still created but with **no** `HandledBy` edge (silently
  dropped, not a deferred/cross-file-resolved edge like `deferred_calls`
  handles for `Calls`). This mirrors the same-file-only resolution most
  existing deferred mechanisms in this file already have as a starting
  point, but wiring a `deferred_handled_by` field through
  `ParseResult`/`indexer.rs` (which would require touching every language
  parser's `ParseResult` construction, not just `typescript.rs`) was judged
  out of this track's scope — flagged here as the natural next increment if
  cross-file Express apps (route files separate from handler files, a very
  common real-world layout) turn out to matter in practice.
- `app.use('/api', router)` sub-router mounting is not modeled — mounted
  sub-router paths are not prefixed onto the routes registered on the
  sub-router, so `router.get('/users', ...)` inside a router mounted at
  `/api` is recorded as `GET /users`, not `GET /api/users`. Flagged as a
  known precision gap for the FastAPI/generalization increment to consider
  fixing across frameworks at once (many frameworks have an equivalent
  concept — FastAPI's `include_router(prefix=...)`, NestJS module prefixes).

## Files touched

- `crates/gitcortex-core/src/schema.rs` — `NodeKind::Route`,
  `EdgeKind::HandledBy` (variant + `Display` + `FromStr` where applicable).
- `crates/gitcortex-store/src/kuzu/conv.rs` — `edge_kind_from_str` arm for
  `"handled_by"`.
- `crates/gitcortex-cli/src/style.rs` — `kind_color` arm for `Route`
  (exhaustive match fixup).
- `crates/gitcortex-viz/src/lib.rs` — `kind_dot_color` arm for `Route`
  (exhaustive match fixup) + `parse_node_kind` arm for `"route"`.
- `crates/gitcortex-indexer/src/parser/typescript.rs` — `collect_routes`,
  `try_route`, `resolve_handler`, `string_literal_value`, wired into
  `parse_source`; two new tests.
- `crates/gitcortex-mcp/src/mcp/tools.rs` — `ast_search`'s description and
  "unknown kind" error message updated to include `route`.
