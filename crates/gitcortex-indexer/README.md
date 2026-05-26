# gitcortex-indexer

Incremental AST indexer for Git repositories. Reads only the files that changed since the last indexed commit, parses them with tree-sitter, resolves cross-file edges, and produces a `GraphDiff` ready to be written to any `GraphStore` backend.

## Add to your project

```toml
[dependencies]
gitcortex-core    = "0.2"
gitcortex-indexer = "0.2"
```

## Quick start

```rust
use gitcortex_indexer::IncrementalIndexer;
use std::path::Path;

// Open an indexer rooted at the repo
let indexer = IncrementalIndexer::new(Path::new("/path/to/your/repo"))?;

// Full index on first run — pass None for last_sha
let (diff, head_sha) = indexer.run(None)?;

println!("{} nodes, {} edges", diff.added_nodes.len(), diff.added_edges.len());

// Incremental update — pass the last indexed SHA
let (diff2, new_sha) = indexer.run(Some(&head_sha))?;
// diff2 is empty if nothing changed
```

## How it works

1. Opens the repository with `git2` to compute which files changed between `last_sha` and `HEAD`.
2. Filters to supported extensions and respects `.gitcortex/ignore` patterns.
3. Parses each changed file in parallel (via `rayon`) using the appropriate tree-sitter parser.
4. Resolves cross-file call edges, uses edges, implements edges, and more within the changed set.
5. Unresolved edges (where the target lives in an unchanged file) are returned as deferred lists — the store resolves them against its full existing data.
6. Synthesises `File` and `Folder` structural nodes from the parsed file paths.
7. Returns the combined `GraphDiff` and the new HEAD SHA to persist.

## Supported languages

| Language | Extensions | What is extracted |
|---|---|---|
| **Rust** | `.rs` | structs, traits, impl blocks, functions, methods, macros, calls, implements, uses, imports |
| **Python** | `.py` | classes, functions, methods, decorators, properties, generators, calls, inherits, implements (Protocol), imports |
| **TypeScript** | `.ts`, `.tsx` | classes, interfaces, functions, arrow functions, methods, calls, implements, inherits, imports, type annotations |
| **JavaScript** | `.js`, `.jsx`, `.mjs`, `.cjs` | same as TypeScript (without type annotations) |
| **Go** | `.go` | packages, structs, interfaces, methods, functions, calls, implements, imports |
| **Java** | `.java` | classes, interfaces, enums, annotations, methods, calls, implements, inherits, throws, imports |

## Ignore patterns

Create `.gitcortex/ignore` in your repo root (`.gitignore` syntax) to exclude files from indexing:

```gitignore
target/
dist/
node_modules/
**/*.generated.ts
**/*.pb.rs
```

The `ignore` crate evaluates these patterns — the same engine that backs `ripgrep`.

## File size limit

Files larger than 512 KB are skipped silently. Override the limit by building a custom indexer wrapper.

## Using a parser directly

Each language parser implements the `LanguageParser` trait and can be used standalone:

```rust
use gitcortex_indexer::parser::{parser_for_path, LanguageParser};
use std::path::Path;

let source = r#"
    fn greet(name: &str) -> String {
        format!("Hello, {name}!")
    }
"#;

let parser = parser_for_path(Path::new("greet.rs")).unwrap();
let result = parser.parse(Path::new("greet.rs"), source)?;

println!("{} nodes", result.nodes.len());
for node in &result.nodes {
    println!("  {} ({:?}) at line {}", node.name, node.kind, node.span.start_line);
}
```

`ParseResult` contains:
- `nodes` — all named entities found in the file
- `edges` — intra-file edges (calls, contains, implements resolved within the same file)
- `deferred_calls` / `deferred_uses` / `deferred_implements` / ... — cross-file references as `(src_id, target_name)` pairs

## Integrating with a store

The `GraphDiff` returned by `IncrementalIndexer::run` is passed directly to any `GraphStore::apply_diff`:

```rust
use gitcortex_indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;
use gitcortex_core::store::GraphStore;
use std::path::Path;

let repo_root = Path::new("/path/to/repo");
let indexer = IncrementalIndexer::new(repo_root)?;
let mut store = KuzuGraphStore::open(repo_root)?;

let branch = "main";
let last_sha = store.last_indexed_sha(branch)?;
let (diff, head_sha) = indexer.run(last_sha.as_deref())?;

if !diff.is_empty() {
    store.apply_diff(branch, &diff)?;
    store.set_last_indexed_sha(branch, &head_sha)?;
}
```

## Differ

`Differ` wraps `git2::Repository` and can be used independently if you only need file-level change detection:

```rust
use gitcortex_indexer::differ::Differ;
use std::path::Path;

let differ = Differ::open(Path::new("/path/to/repo"))?;
let head = differ.head_sha()?;
let changes = differ.changed_files(Some(&last_sha), &["rs", "ts", "py"])?;

for change in changes {
    println!("{change:?}");
}
```

`FileChange` is an enum with three variants: `Added`, `Modified`, `Deleted`, each carrying the repo-relative path.

## Dependencies

| Crate | Purpose |
|---|---|
| `gitcortex-core` | Shared graph types and `GraphStore` trait |
| `tree-sitter` | Parser runtime |
| `tree-sitter-{rust,python,typescript,javascript,go,java}` | Language grammars |
| `git2` | Git diff and SHA resolution |
| `rayon` | Parallel file parsing |
| `ignore` | `.gitcortex/ignore` pattern matching |

## License

MIT — free for commercial and open-source use.

[GitHub](https://github.com/bharath03-a/GitCortex) · [Issues](https://github.com/bharath03-a/GitCortex/issues)
