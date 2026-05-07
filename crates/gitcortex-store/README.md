# gitcortex-store

KuzuDB-backed graph store for GitCortex. Implements the `GraphStore` trait from `gitcortex-core` with a local embedded database — one database file per repository, with independent per-branch node and edge tables inside it. No server process, no external dependencies.

## Add to your project

```toml
[dependencies]
gitcortex-core  = "0.2"
gitcortex-store = "0.2"
```

## Quick start

```rust
use gitcortex_store::kuzu::KuzuGraphStore;
use gitcortex_core::store::GraphStore;
use std::path::Path;

// Open or create the database for the repo at this path
let mut store = KuzuGraphStore::open(Path::new("/path/to/your/repo"))?;

// Apply a diff produced by gitcortex-indexer
store.apply_diff("main", &diff)?;
store.set_last_indexed_sha("main", &head_sha)?;

// Query
let nodes = store.lookup_symbol("main", "process_request", false)?;
let callers = store.find_callers("main", "process_request")?;
```

## Where data is stored

The database lives in your home directory, never in the repo:

```
~/.local/share/gitcortex/{repo_id}/
    graph.kuzu        # KuzuDB database (all branches in one file)
    schema_version    # schema version marker for auto-migration
    main.sha          # last indexed SHA for branch "main"
    feat__auth.sha    # last indexed SHA for branch "feat/auth"
```

These files are machine-local and should never be committed.

## Branch isolation

Each branch gets its own node and edge tables inside the single database file. Switching branches in the query API is as simple as passing a different branch name — no file switching, no re-opening the database.

```rust
// Query the same symbol across two branches
let on_main = store.lookup_symbol("main", "AuthService", false)?;
let on_feat = store.lookup_symbol("feat/auth", "AuthService", false)?;
```

Branch names are normalised automatically: `/` becomes `__`, leading digits are prefixed, so `feat/my-branch` becomes the table prefix `feat__my_branch` internally.

## Schema versioning

If the on-disk schema doesn't match the current version, the store automatically wipes all data for that repo and prints a message:

```
gitcortex: schema version mismatch (expected 4); wiping graph store for re-index
```

The next `gcx hook` (or `store.apply_diff`) triggers a full re-index from scratch.

## Full API

All methods are from the `GraphStore` trait in `gitcortex-core`:

```rust
use gitcortex_core::store::GraphStore;
use std::path::Path;

// Write
store.apply_diff("main", &diff)?;
store.set_last_indexed_sha("main", &sha)?;

// Basic lookups
let nodes  = store.lookup_symbol("main", "MyStruct", false)?; // exact match
let nodes  = store.lookup_symbol("main", "My", true)?;        // fuzzy (substring)
let defs   = store.list_definitions("main", Path::new("src/auth.rs"))?;
let all    = store.list_all_nodes("main")?;
let edges  = store.list_all_edges("main")?;

// Caller / callee traversal
let callers  = store.find_callers("main", "validate_token")?;      // 1 hop
let deep     = store.find_callers_deep("main", "validate_token", 3)?; // up to 3 hops
let callees  = store.find_callees("main", "handle_request", 2)?;   // forward, 2 hops

// Structural queries
let impls  = store.find_implementors("main", "AuthProvider")?;
let path   = store.trace_path("main", "main", "validate_token")?;   // shortest BFS path
let sub    = store.get_subgraph("main", "UserService", 2, "both")?; // 2-hop neighbourhood
let ctx    = store.symbol_context("main", "process_request")?;      // callers + callees + used_by
let range  = store.list_symbols_in_range("main", Path::new("src/auth.rs"), 10, 50)?;
let dead   = store.find_unused_symbols("main", None)?;

// Branch diff
let diff   = store.branch_diff("main", "feat/auth")?; // nodes/edges added or removed

// Index state
let sha    = store.last_indexed_sha("main")?; // None if never indexed
```

### Multi-hop results

`find_callers_deep`, `find_callees` return a `CallersDeep` struct:

```rust
pub struct CallersDeep {
    pub hops: Vec<Vec<Node>>, // hops[0] = direct callers (hop 1), hops[1] = hop 2, ...
    pub risk_level: &'static str, // "LOW" | "MEDIUM" | "HIGH"
}
```

### Symbol context

`symbol_context` gives a 360° view of a single symbol:

```rust
pub struct SymbolContext {
    pub definition: Node,    // the node itself
    pub callers: Vec<Node>,  // what calls this symbol
    pub callees: Vec<Node>,  // what this symbol calls
    pub used_by: Vec<Node>,  // what references this symbol via Uses edges
}
```

### Subgraph

`get_subgraph` returns all nodes and edges within N hops of a seed symbol:

```rust
let sub = store.get_subgraph("main", "UserService", 2, "both")?;
println!("{} nodes, {} edges", sub.nodes.len(), sub.edges.len());

// direction options: "in" (callers only), "out" (callees only), "both"
```

## Using without gitcortex-indexer

The store is independent of the indexer. You can build `GraphDiff` values manually and apply them directly — useful for testing or custom import pipelines:

```rust
use gitcortex_core::graph::{GraphDiff, Node, NodeId, Span, NodeMetadata};
use gitcortex_core::schema::{NodeKind, Visibility};
use gitcortex_store::kuzu::KuzuGraphStore;
use gitcortex_core::store::GraphStore;

let mut store = KuzuGraphStore::open(Path::new("/tmp/test-repo"))?;

let node = Node {
    id: NodeId::new(),
    kind: NodeKind::Function,
    name: "my_func".into(),
    qualified_name: "crate::my_func".into(),
    file: "src/lib.rs".into(),
    span: Span { start_line: 1, end_line: 5 },
    metadata: NodeMetadata { visibility: Visibility::Pub, ..Default::default() },
};

let diff = GraphDiff { added_nodes: vec![node], ..Default::default() };
store.apply_diff("main", &diff)?;
```

## Testing

The round-trip tests in `tests/round_trip.rs` demonstrate the full write→read cycle:

```
cargo test -p gitcortex-store
```

Each test uses a `tempfile::TempDir` so tests are fully isolated. Because KuzuDB cannot open the same database path from multiple processes simultaneously, tests that open different databases run in parallel safely; tests sharing a path must be serialised.

## Dependencies

| Crate | Purpose |
|---|---|
| `gitcortex-core` | Shared graph types and `GraphStore` trait |
| `kuzu` | Embedded graph database |
| `dashmap` | Concurrent map for in-memory branch metadata caching |
| `tracing` | Structured logging for slow query diagnostics |

## License

MIT — free for commercial and open-source use.

[GitHub](https://github.com/bharath03-a/GitCortex) · [Issues](https://github.com/bharath03-a/GitCortex/issues)
