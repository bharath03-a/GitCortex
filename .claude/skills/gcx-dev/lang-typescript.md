---
name: lang-typescript
description: TypeScript/JavaScript AST expert for gcx-indexer: tree-sitter node types, NodeKind/EdgeKind mapping, interface vs class disambiguation, generics, arrow functions. Use when implementing or debugging the TypeScript parser in crates/gitcortex-indexer/src/parser/typescript.rs.
---

# TypeScript Language Expert — gcx-indexer

## tree-sitter grammar node types → gcx NodeKind

tree-sitter-typescript covers both `.ts` and `.tsx` files (with JSX). Use tree-sitter-javascript for `.js`/`.jsx` — they share most node type names.

| tree-sitter node | gcx NodeKind | Notes |
|------------------|-------------|-------|
| `program` | `File` | one per file |
| `class_declaration` | `Struct` | `class Foo {}` |
| `interface_declaration` | `Trait` | `interface Foo {}` — NOT Struct |
| `function_declaration` | `Function` | top-level `function foo() {}` |
| `method_definition` | `Method` | inside class body |
| `arrow_function` (assigned to `const`/`let`) | `Function` | see rules below |
| `type_alias_declaration` | `TypeAlias` | `type Foo = Bar` |
| `enum_declaration` | `Enum` | `enum Status {}` |
| `abstract_class_declaration` | `Struct` | same as class |
| `module` / `namespace_declaration` | `Module` | `namespace Foo {}` |
| `variable_declarator` (module-level const) | `Constant` | `const MAX = 100` |
| `import_statement` | produces `Imports` edges | |
| `export_statement` wrapping a declaration | pass-through — extract inner declaration | |

## NodeKind mapping rules

**`interface` MUST map to `Trait`**, not `Struct`. TypeScript interfaces are structural contracts, not implementations. Conflating them breaks `Implements` edge semantics.

**Arrow functions**:
- `const foo = () => {}` at module level → `Function` node named `foo`
- `const foo = async () => {}` → `Function` with `is_async: true`
- Arrow function as a class field (`foo = () => {}` inside class body) → `Method`
- Arrow functions NOT assigned to a name → skip (anonymous inline)

**`export` wrapper**: `export function foo() {}`, `export class Foo {}`, `export const Bar = ...` — the `export_statement` is a wrapper. Extract the inner declaration for NodeKind. The export fact can be stored in `visibility: Pub`.

**Generic type parameters**: Strip from node names. `class Foo<T>` → name is `Foo`, not `Foo<T>`. `function bar<K, V>` → name is `bar`. Generic params exist only in the signature string, not in `name` or `qualified_path`.

**`.d.ts` files**: Declaration-only files should be EXCLUDED from indexing entirely. They're type stubs, not implementations. Check by file extension: skip `*.d.ts`.

## EdgeKind mapping

| Relationship | EdgeKind | trigger |
|-------------|----------|---------|
| File contains top-level def | `Contains` | `program` → declaration |
| Class contains method | `Contains` | `class_body` → `method_definition` |
| Namespace contains decl | `Contains` | `namespace_body` → declaration |
| `import ... from` | `Imports` | `import_statement` |
| `class Foo implements Bar` | `Implements` | `implements_clause` |
| `class Foo extends Base` | `Uses` | `extends_clause` |
| Type annotation references | `Uses` | parameter types, return types |
| Function calls another | `Calls` | `call_expression` — best-effort |

## `qualified_path` format

```
<file_path_stem>::<namespace>::<ClassName>::<method>
```

- File path relative to repo root, slashes → `::`, extension stripped.
- Namespace/module names are inserted if present.
- Example: `src/router/index.ts` → `src::router::index::Router::use`

## Known tree-sitter quirks

1. **`export_statement` wraps real declarations**: The actual `class_declaration` or `function_declaration` is a child of `export_statement`. Navigate to child before extracting NodeKind.
2. **`export default`**: `export default class Foo {}` — name may be absent on anonymous default exports. Use the file stem as a fallback name.
3. **Optional chaining / nullish coalescing**: `?.` and `??` are not structural — ignore for graph purposes.
4. **Overloaded function signatures**: TypeScript allows multiple `function_declaration` with same name (overloads). The last one (with a body) is the implementation. Emit one `Function` node for the implementation; skip declaration-only overloads.
5. **`declare` statements**: `declare function foo(): void` — type-declaration-only, no implementation. Skip for node creation.
6. **`abstract` methods**: `abstract doSomething(): void` inside an abstract class — emit as `Method` with `is_unsafe: false` (no special flag; abstract is a TypeScript concept, not in `NodeMetadata`). At minimum, do not skip them.
7. **JSX elements in `.tsx`**: `<Component />` expressions are NOT nodes. JSX is syntax, not a definition. Ignore for graph purposes.
8. **`satisfies` operator**: `const x = { ... } satisfies Foo` — the `satisfies` produces a `Uses` edge to `Foo` but the left side determines the node kind.

## Test fixture checklist

A good TypeScript fixture (`tests/integration/fixtures/typescript/`) must exercise:
- [ ] `class` with methods
- [ ] `interface` (must be `Trait`, not `Struct`)
- [ ] Arrow function assigned to `const` (top-level `Function`)
- [ ] `async` function (both `async function` and `async () =>`)
- [ ] `type` alias (`TypeAlias`)
- [ ] `enum` (`Enum`)
- [ ] `import { X } from './y'` and `import * as Y from './z'`
- [ ] `class Foo implements Bar` (`Implements` edge)
- [ ] `class Foo extends Base` (`Uses` edge)
- [ ] Type annotations on params and return (`Uses` edges)

## Common bugs to watch

- `interface` mapped to `Struct` → breaks `Implements` semantics
- Generic params in node name → breaks lookup by name
- `.d.ts` files indexed → graph polluted with duplicate type-only nodes
- Arrow functions assigned to `const` skipped → half the codebase missing
- `async` arrow functions have `is_async: false` → metadata wrong
