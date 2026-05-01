# GitCortex — CLAUDE.md

## What is this

GitCortex (`gcx`) builds and maintains a branch-aware knowledge graph of a Git repository. It fires on every local HEAD change via git hooks, incrementally re-indexes only changed files using tree-sitter AST parsing, persists the graph in an embedded KuzuDB database namespaced per branch (stored locally, designed for remote backend swap-in), and exposes the graph to AI coding assistants via an MCP server.

---

## Workspace layout

```
gitcortex/
├── Cargo.toml                          # workspace root, resolver = "2"
├── .gitcortex/                         # repo-level config — committed to repo
│   ├── config.toml                     # indexing config (languages, LLD thresholds, backend)
│   └── ignore                          # .gitignore-syntax exclusion patterns
├── crates/
│   ├── gitcortex-core/                 # shared types + GraphStore trait — NO I/O, NO async
│   ├── gitcortex-indexer/              # AST parsing, git diff, incremental indexing — sync only
│   ├── gitcortex-store/                # KuzuDB backend (implements GraphStore trait)
│   └── gitcortex-mcp/                  # MCP server + gcx CLI binary
├── hooks/
│   ├── post-commit                     # shell stub: gcx hook
│   ├── post-merge                      # shell stub: gcx hook
│   ├── post-rewrite                    # shell stub: gcx hook
│   └── post-checkout                   # shell stub: gcx hook --branch-switch
└── tests/
    └── integration/
        └── fixtures/                   # small git repos for integration tests
```

**Machine-local data (never committed):**
```
~/.local/share/gitcortex/{repo_id}/
    {branch}/
        graph.kz      # KuzuDB database for this branch
        last_sha      # last indexed commit SHA (used for diffing)
```

---

## Crate responsibilities

**`gitcortex-core`** — shared types + `GraphStore` trait. No I/O, no async.
Key crates: `petgraph`, `serde`, `thiserror`, `uuid`

**`gitcortex-indexer`** — reads repo, parses changed files, produces a `GraphDiff`.
Key crates: `tree-sitter`, `tree-sitter-rust`, `git2`, `ignore`, `rayon`

**`gitcortex-store`** — implements `GraphStore` trait with KuzuDB backend, branch-namespaced.
Key crates: `kuzu`, `dashmap`

**`gitcortex-mcp`** — MCP server + `gcx` CLI binary. Only crate that uses `tokio`.
Key crates: `rmcp`, `axum`, `clap`, `tracing`, `notify`

**Crate dependency order:**
```
gitcortex-core  (no internal deps)
      ↑               ↑
gitcortex-indexer  gitcortex-store
      ↑               ↑
         gitcortex-mcp
```

---

## Key architecture decisions

- **`GraphStore` is a trait** (in `gitcortex-core`) — `KuzuGraphStore` is the v0.1 local backend. Swap to remote backend without touching indexer or MCP layer.
- **KuzuDB** — embedded graph DB, zero server process, ships in the binary
- **tree-sitter** — one parsing API across all languages, incremental re-parsing built in
- **git2** — type-safe, no subprocess overhead; the hook path must be near-instant
- **Async only at the boundary** — `tokio` lives in `gitcortex-mcp` only; indexer and store are sync
- **Two-pass indexing** — Pass 1 (structural, <500ms, sync) + Pass 2 (LLD annotation, async background, v0.2)
- **`.gitcortex/ignore`** — `.gitignore`-syntax exclusion file, read by the `ignore` crate

---

## Graph schema

### NodeKind — everything named and referenceable is a node

```rust
pub enum NodeKind {
    File,
    Module,      // mod foo { }
    Struct,      // struct Foo { }
    Enum,        // enum Bar { }
    Trait,       // trait Baz { }
    TypeAlias,   // type Alias = ...
    Function,    // free-standing fn
    Method,      // fn inside impl block
    Constant,    // const / static
    Macro,       // macro_rules! or proc-macro
}
```

### EdgeKind

```rust
pub enum EdgeKind {
    Contains,    // Module→Struct, Struct→Method, File→Module
    Calls,       // Function→Function at call sites
    Implements,  // Struct→Trait (from impl Trait for Struct)
    Uses,        // fn param/return type references a Struct or Trait
    Imports,     // use path::to::Thing
}
```

### NodeMetadata — typed, not `HashMap`

```rust
pub struct NodeMetadata {
    pub loc: u32,
    pub visibility: Visibility,   // Pub | PubCrate | Private
    pub is_async: bool,
    pub is_unsafe: bool,
    pub lld: LldLabels,           // populated in Pass 2
}

pub struct LldLabels {
    pub solid_hints: Vec<SolidHint>,   // SRP, OCP, LSP, ISP, DIP
    pub patterns: Vec<DesignPattern>,  // Builder, Factory, Observer...
    pub smells: Vec<CodeSmell>,        // GodStruct, LongMethod, DeepNesting
    pub complexity: Option<u32>,       // cyclomatic complexity (future)
}
```

### GraphStore trait

```rust
pub trait GraphStore: Send + Sync {
    fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()>;
    fn lookup_symbol(&self, branch: &str, name: &str) -> Result<Vec<Node>>;
    fn find_callers(&self, branch: &str, name: &str) -> Result<Vec<Node>>;
    fn list_definitions(&self, branch: &str, file: &Path) -> Result<Vec<Node>>;
    fn branch_diff(&self, from: &str, to: &str) -> Result<GraphDiff>;
    fn last_indexed_sha(&self, branch: &str) -> Result<Option<String>>;
    fn set_last_indexed_sha(&mut self, branch: &str, sha: &str) -> Result<()>;
}
```

---

## Hook design — drift-proof

`post-commit` alone misses `git pull --ff`, `git rebase`, `git commit --amend`.
`gcx init` installs all four hooks:

| Hook | Trigger | Action |
|---|---|---|
| `post-commit` | Local commit | `gcx hook` |
| `post-merge` | `git pull` / `git merge` | `gcx hook` |
| `post-rewrite` | `git rebase`, `--amend` | `gcx hook` |
| `post-checkout` | `git switch`, `git checkout` | `gcx hook --branch-switch` |

**`gcx hook` logic (always diffs from `last_indexed_sha`, not `HEAD~1`):**
```
sha = read last_indexed_sha for current branch
if sha == HEAD: exit (idempotent no-op)
diff = git diff sha..HEAD  (filtered by .gitcortex/ignore)
apply diff to GraphStore
write HEAD → last_indexed_sha
```

`--branch-switch` only updates the active-branch pointer — no re-index; each branch has its own graph.

---

## Config files

### `.gitcortex/config.toml` (committed — team-shared)

```toml
[index]
languages = ["rust"]           # v0.1. v0.2 adds "typescript", "python"
max_file_size_kb = 500

[lld]
enabled = false                # pass-2 LLD annotation (v0.2)
srp_method_threshold = 10
isp_method_threshold = 7

[store]
backend = "local"              # "local" | "remote" (future)
```

### `.gitcortex/ignore` (committed — .gitignore syntax)

```gitignore
target/
build/
dist/
vendor/
**/*.generated.rs
**/*.pb.rs
```

---

## Conventions

- All public APIs return `Result<T, GitCortexError>` — no `.unwrap()` in library code
- Crates: `kebab-case` · Types/Traits: `PascalCase` · Functions/modules: `snake_case`
- MCP tool names: `snake_case`
- Never commit KuzuDB files (`*.kz`) or `last_sha` files — these are machine-local

---

## Behavioral guidelines

### Think before coding
- State assumptions explicitly. If uncertain, ask — don't guess silently.
- If multiple interpretations exist, present them.
- If a simpler approach exists, say so and push back.
- If something is unclear, stop and name what's confusing.

### Simplicity first
- Minimum code that solves the problem. Nothing speculative.
- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" that wasn't requested.
- Ask: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

### Surgical changes
- Touch only what you must. Don't improve adjacent code unless asked.
- Match existing style, even if you'd do it differently.
- Remove imports/variables made unused by **your** changes. Leave pre-existing dead code alone.
- Every changed line should trace directly to the request.

### Goal-driven execution
- Transform tasks into verifiable goals before starting.
- For multi-step tasks, state a brief plan with verify steps:
  ```
  1. [step] → verify: [check]
  2. [step] → verify: [check]
  ```
- Strong success criteria let you loop independently. Weak ones ("make it work") require clarification — ask upfront.

## GitCortex Knowledge Graph

This repo is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
Use the MCP server (`gcx serve`, configured in `.mcp.json`) or these slash commands:

- `/gcx-lookup <name>` — find all definitions matching a name
- `/gcx-callers <name>` — find all callers of a function
- `/gcx-file <path>` — list all definitions in a file
- `/gcx-blast-radius` — show blast radius of changes vs main
