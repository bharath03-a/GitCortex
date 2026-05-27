---
name: lang-go
description: Go AST expert for gcx-indexer: tree-sitter node types, NodeKind/EdgeKind mapping, struct embedding, interface satisfaction, receiver methods, package-level functions. Use when implementing or debugging the Go parser in crates/gitcortex-indexer/src/parser/go.rs.
---

# Go Language Expert — gcx-indexer

## tree-sitter grammar node types → gcx NodeKind

| tree-sitter node | gcx NodeKind | Notes |
|------------------|-------------|-------|
| `source_file` | `File` | one per file |
| `type_declaration` → `type_spec` with `struct_type` | `Struct` | `type Foo struct {}` |
| `type_declaration` → `type_spec` with `interface_type` | `Trait` | `type Foo interface {}` |
| `type_declaration` → `type_spec` with other types | `TypeAlias` | `type MyInt int`, `type Alias = Other` |
| `function_declaration` | `Function` | top-level `func Foo() {}` |
| `method_declaration` | `Method` | `func (r *Receiver) Foo() {}` |
| `const_declaration` → `const_spec` | `Constant` | each spec is a separate `Constant` node |
| `var_declaration` (module-level) | `Constant` | module-level vars |
| `import_declaration` | produces `Imports` edges | |

## NodeKind mapping rules

**`Function` vs `Method` distinction** — determined by presence of receiver:
- `function_declaration` has no receiver → `Function`
- `method_declaration` has a `parameter_list` as the receiver (the "receiver parameter") → `Method`
- The receiver type (dereferenced, e.g. `*Router` → `Router`) is the parent node for `Contains`

**Receiver pointer vs. value**: `func (r Router) F()` and `func (r *Router) F()` both belong to `Router`. Strip `*` when determining the parent `Struct` node.

**Struct embedding**:
- `type Server struct { http.Server }` — the embedded field `http.Server` has no field name.
- Emit a `Uses` edge from `Server` → `http.Server` (or `Server` if local) to represent embedding.
- Do NOT create a `Contains` edge for the embedded type — it is not a method, it's a field.

**Interface satisfaction**:
- Go uses structural typing — no `implements` keyword.
- Emit `Implements` edges only when a type has a method set that matches an interface **and** both are declared in the same file/package (conservative approach).
- At minimum, emit `Uses` edges for interface types used in function signatures.

**`const` blocks**:
```go
const (
    A = 1
    B = 2
)
```
Each `const_spec` → separate `Constant` node. Do not collapse the block into one node.

## EdgeKind mapping

| Relationship | EdgeKind | trigger |
|-------------|----------|---------|
| File contains top-level def | `Contains` | `source_file` → declaration |
| Struct contains method (via receiver) | `Contains` | `method_declaration` receiver type → method |
| `import "pkg"` | `Imports` | `import_spec` |
| Struct embedding | `Uses` | anonymous field in `struct_type` |
| Interface in function signature | `Uses` | type in `parameter_list` or `result` |
| Function/method calls | `Calls` | `call_expression` — best-effort |

## `qualified_path` format

Go packages have import paths. Use the module path from `go.mod` as the root:

```
<module_path>::<package>::<TypeName>::<method>
```

- Read `go.mod` `module` directive for the root.
- Package name comes from `package <name>` declaration at top of file.
- Example: module `github.com/gin-gonic/gin`, package `gin`, type `Engine` → `github.com/gin-gonic/gin::gin::Engine::ServeHTTP`
- For top-level functions: `github.com/gin-gonic/gin::gin::New`

## Known tree-sitter quirks

1. **`type_declaration` wraps `type_spec`**: The actual name and underlying type are inside `type_spec`. Navigate to child.
2. **Multiple `type_spec` in one `type_declaration`**: `type ( A struct{}; B interface{} )` — each `type_spec` is a separate node.
3. **`method_declaration` receiver is a `parameter_list`**: The first (and only) parameter in the receiver list is the receiver. Its type determines the parent struct.
4. **`_` blank identifier**: `func (_ Foo) Bar()` — valid Go, the receiver has no name. Still a method on `Foo`.
5. **`iota` in const blocks**: `const ( A = iota; B )` — both `A` and `B` are `Constant` nodes. Don't skip `iota`-valued consts.
6. **`init()` function**: Valid to have multiple `init()` per package (one per file). Emit each as a `Function` node; qualify by file to avoid name collision.
7. **Short variable declarations**: `x := foo()` inside a function body — NOT a `Constant` node. Only module-level `var`/`const` become nodes.
8. **`go:generate` comments**: `//go:generate protoc ...` — ignore, not AST nodes.
9. **Build tag comments**: `//go:build linux` at file top — may cause parser to see empty file on wrong platform. Handle by indexing regardless of build tags.

## Generated file exclusion

Go projects often have generated files that should be excluded:
- `*.pb.go` (protobuf)
- `*_generated.go`
- `zz_generated_*.go` (controller-gen)
- `mock_*.go` (mockery)

These should be covered by `.gitcortex/ignore`. Verify they're not bloating the graph.

## Test fixture checklist

A good Go fixture (`tests/integration/fixtures/go/`) must exercise:
- [ ] Package declaration
- [ ] `struct` type with fields
- [ ] `interface` type with methods (`Trait`)
- [ ] Top-level function (`Function`)
- [ ] Method with value receiver (`Method`, `Contains` from struct)
- [ ] Method with pointer receiver (`Method`, parent is same struct)
- [ ] Struct embedding (anonymous field, `Uses` edge)
- [ ] `const` block with multiple constants
- [ ] `import` single and block form (`Imports` edges)
- [ ] Type alias (`TypeAlias`)

## Common bugs to watch

- Receiver methods emitted as `Function` → breaks containment and callers
- Pointer receiver `*Router` creating a separate node from `Router` → duplicate structs
- Struct embedding fields producing zero edges → embedding invisible to graph
- `const` block collapsed to single node → symbol lookup fails for individual consts
- `go.mod` module path not used in `qualified_path` → paths not unique across repos
