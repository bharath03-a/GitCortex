# gitcortex-core

Shared types and the `GraphStore` trait for the GitCortex ecosystem.

This crate is the protocol layer — it defines every data structure that flows between the indexer, the store, and the MCP server, and the trait that any storage backend must implement. It has no I/O and no async code, making it safe to use in any context.

## Add to your project

```toml
[dependencies]
gitcortex-core = "0.2"
```

## What's in this crate

### Graph types

The core graph is made of `Node`s and `Edge`s.

```rust
use gitcortex_core::graph::{Node, NodeId, Edge, Span, NodeMetadata};
use gitcortex_core::schema::{NodeKind, EdgeKind, Visibility};

// Every named entity in the codebase is a Node
let node = Node {
    id: NodeId::new(),
    kind: NodeKind::Function,
    name: "process_request".into(),
    qualified_name: "crate::handler::process_request".into(),
    file: "src/handler.rs".into(),
    span: Span { start_line: 12, end_line: 34 },
    metadata: NodeMetadata {
        visibility: Visibility::Pub,
        is_async: true,
        ..Default::default()
    },
};
```

### Node kinds

| Kind | What it represents |
|---|---|
| `Function` | Free-standing function |
| `Method` | Function inside a class / impl block |
| `Struct` | Struct, class, data type |
| `Trait` | Rust trait |
| `Interface` | TypeScript / Go / Java interface |
| `Enum` | Enum declaration |
| `EnumMember` | Variant inside an enum |
| `Module` | Module, package, namespace |
| `TypeAlias` | Type alias |
| `Constant` | Constant or static value |
| `Macro` | Rust macro |
| `Property` | Class property, getter/setter |
| `Annotation` | Decorator or annotation declaration |
| `File` | Source file |
| `Folder` | Directory in the repo tree |

### Edge kinds

| Kind | What it represents |
|---|---|
| `Calls` | Function calls another function |
| `Contains` | File → Module, Struct → Method, etc. |
| `Implements` | Struct/class implements a trait/interface |
| `Inherits` | Class extends another class |
| `Uses` | A type is used as a parameter or return type |
| `Imports` | Import / use declaration |
| `Annotated` | Symbol is decorated by an annotation |
| `Throws` | Method throws an exception type |

### Node metadata

Every node carries structured flags collected during AST parsing:

```rust
pub struct NodeMetadata {
    pub loc: u32,               // lines of code
    pub visibility: Visibility, // Pub | PubCrate | Private
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_const: bool,
    pub is_property: bool,
    pub is_generator: bool,
    pub generic_bounds: Vec<String>, // e.g. ["T: Send", "T: 'static"]
    pub lld: LldLabels,              // pass-2 annotations (SOLID hints, patterns, smells)
}
```

### GraphDiff — the unit of incremental change

The indexer produces a `GraphDiff` for each commit. Applying it to the store brings the graph up to date.

```rust
use gitcortex_core::graph::GraphDiff;

let mut base = GraphDiff::default();
let per_file_diff = GraphDiff { added_nodes: vec![node], ..Default::default() };
base.merge(per_file_diff); // combine per-file diffs before a single store write
```

Unresolved cross-file edges are carried as deferred lists (`deferred_calls`, `deferred_uses`, etc.). The store resolves them against its full existing data after the new nodes are inserted.

### GraphStore trait

Any storage backend implements this trait. The included `KuzuGraphStore` (in `gitcortex-store`) is the local embedded backend.

```rust
use gitcortex_core::store::GraphStore;

// Write
store.apply_diff("main", &diff)?;
store.set_last_indexed_sha("main", &head_sha)?;

// Read
let nodes = store.lookup_symbol("main", "process_request", false)?;
let callers = store.find_callers("main", "process_request")?;
let callees = store.find_callees("main", "process_request", 2)?;
let defs = store.list_definitions("main", Path::new("src/handler.rs"))?;
let path = store.trace_path("main", "main", "validate_token")?;
let ctx = store.symbol_context("main", "process_request")?;
let subgraph = store.get_subgraph("main", "UserService", 2, "both")?;
let unused = store.find_unused_symbols("main", None)?;
let diff = store.branch_diff("main", "feat/auth")?;
let sha = store.last_indexed_sha("main")?;
```

All methods take a `branch` parameter — each branch has an independent graph.

### Error type

All public APIs return `Result<T, GitCortexError>`. The error type covers git failures, parse errors, store errors, and I/O errors.

```rust
use gitcortex_core::error::GitCortexError;

match result {
    Err(GitCortexError::Git(msg)) => { /* git operation failed */ }
    Err(GitCortexError::Parse { file, message }) => { /* AST parse error */ }
    Err(GitCortexError::Store(msg)) => { /* database error */ }
    _ => {}
}
```

## Building a custom backend

Implement `GraphStore` to swap in any storage system:

```rust
use gitcortex_core::store::GraphStore;

struct MyBackend { /* ... */ }

impl GraphStore for MyBackend {
    fn apply_diff(&mut self, branch: &str, diff: &GraphDiff) -> Result<()> {
        // write nodes + edges to your storage
        todo!()
    }
    // ... implement all trait methods
}
```

The indexer and MCP server are decoupled from the backend through this trait. Swapping the backend requires no changes to either.

## Supported languages

Node and edge kinds are language-neutral. Language-specific semantics are mapped to these kinds by the parsers in `gitcortex-indexer`:

- **Rust** — structs, traits, impl blocks, macros
- **Python** — classes, protocols, decorators, generators
- **TypeScript / JavaScript** — classes, interfaces, arrow functions, JSX
- **Go** — structs, interfaces, methods, packages
- **Java** — classes, interfaces, annotations, throws clauses

## License

MIT — free for commercial and open-source use.

[GitHub](https://github.com/bharath03-a/GitCortex) · [Issues](https://github.com/bharath03-a/GitCortex/issues)
