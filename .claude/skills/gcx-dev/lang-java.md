---
name: lang-java
description: Java AST expert for gcx-indexer: tree-sitter node types, NodeKind/EdgeKind mapping, inner classes, annotations, generics, interface vs abstract class. Use when implementing or debugging the Java parser in crates/gitcortex-indexer/src/parser/java.rs.
---

# Java Language Expert — gcx-indexer

## tree-sitter grammar node types → gcx NodeKind

| tree-sitter node | gcx NodeKind | Notes |
|------------------|-------------|-------|
| `program` | `File` | one per file |
| `class_declaration` | `Struct` | concrete and abstract classes |
| `interface_declaration` | `Trait` | `interface Foo {}` |
| `enum_declaration` | `Enum` | `enum Status {}` |
| `annotation_type_declaration` | `Trait` | `@interface Foo {}` annotation types |
| `method_declaration` | `Method` | inside class/interface/enum body |
| `constructor_declaration` | `Method` | `public Foo() {}` — named after class |
| `field_declaration` (class-level, static final) | `Constant` | `static final int MAX = 100` |
| `field_declaration` (non-static) | skip or `Constant` | instance fields are lower value; conservative: skip unless static final |
| `import_declaration` | produces `Imports` edges | |
| `record_declaration` | `Struct` | Java 16+ records |
| `module_declaration` | `Module` | Java 9+ module-info.java |

## NodeKind mapping rules

**Inner classes** — critical for Java:
- `class_declaration` inside another class body → `Struct` node
- Must emit `Contains` edge from outer class to inner class
- This applies to: inner classes, static nested classes, anonymous classes (skip anonymous), local classes
- Anonymous classes (`new Foo() { ... }`) → skip (no stable name)

**`interface` MUST map to `Trait`**, not `Struct`. Java interfaces are contracts. `abstract class` → `Struct` (it has implementation potential).

**Annotations**:
- `@Override`, `@Autowired`, `@Entity` applied to a method/class → do NOT suppress the annotated node
- Store annotation presence as part of metadata if needed, but the primary node (the method/class) must be emitted
- `@interface FooAnnotation {}` → `Trait` node (annotation type declaration)

**Constructors**:
- `constructor_declaration` → `Method` node with name equal to the class name
- `Contains` edge from enclosing class

**Generics**:
- `class Foo<T>` → name is `Foo`, not `Foo<T>`. Strip type params.
- `ImmutableList<E>` in signatures → `Uses` edge to `ImmutableList` (without `<E>`)
- Type bounds (`<T extends Comparable<T>>`) → strip entirely from names

**Enum constants**:
- `enum Status { ACTIVE, INACTIVE }` → enum constants are `Constant` nodes with `Contains` edge from the `Enum`

## EdgeKind mapping

| Relationship | EdgeKind | trigger |
|-------------|----------|---------|
| File contains top-level class | `Contains` | `program` → `class_declaration` |
| Class contains method/constructor | `Contains` | class body → `method_declaration` |
| Class contains inner class | `Contains` | class body → `class_declaration` |
| Class contains enum constants | `Contains` | `enum_declaration` body → `enum_constant` |
| `import X` | `Imports` | `import_declaration` |
| `class Foo implements Bar` | `Implements` | `super_interfaces` clause |
| `class Foo extends Base` | `Uses` | `superclass` clause |
| Return/param type reference | `Uses` | `type_identifier` in method signature |
| Method calls | `Calls` | `method_invocation` — best-effort |

## `qualified_path` format

Java uses package paths:

```
<package>::<ClassName>::<method>
```

- Package from `package_declaration` at top of file.
- Inner class: `com.example::Outer::Inner::method`
- Example: `package com.example.service` → `com.example.service::UserService::findById`

## Known tree-sitter quirks

1. **`class_declaration` nesting**: Java allows multiple levels of inner class nesting. Handle recursively — each class body may contain more class declarations.
2. **`annotation_declaration` vs `annotation_type_declaration`**: tree-sitter-java uses `annotation_type_declaration` for `@interface`. Don't confuse with method/class annotations (which are `marker_annotation`, `normal_annotation`).
3. **`modifiers` node**: Visibility (`public`, `private`, `protected`) and other modifiers (`static`, `final`, `abstract`) are children of a `modifiers` node. Walk into it to extract visibility for `NodeMetadata`.
4. **Wildcard imports**: `import java.util.*` → emit one `Imports` edge to `java.util` (the package). Do not expand the wildcard.
5. **`throws` clause**: `void foo() throws IOException` — `IOException` should produce a `Uses` edge.
6. **`default` interface methods**: Java 8+ interfaces can have `default void foo() {}`. These are `Method` nodes on the `Trait` node with full implementation. Emit them.
7. **`sealed` classes/interfaces**: Java 17+ `sealed class Foo permits Bar, Baz` — emit `Implements` or `Uses` edges to the permitted types.
8. **Lambdas**: `list.forEach(x -> ...)` — anonymous, no stable name. Skip for node creation. May produce `Calls` edges to the method receiving the lambda.
9. **`var` (local variable inference)**: `var x = new Foo()` inside a method — not a node. Skip.
10. **Multi-line `import`**: Java imports are single-line; no edge cases here.

## Generated code exclusion

Java projects generate significant code that should be excluded:
- `target/generated-sources/`
- `build/generated/`
- `*_pb.java` (protobuf)
- `*.generated.java`

Verify `.gitcortex/ignore` covers these patterns.

## Test fixture checklist

A good Java fixture (`tests/integration/fixtures/java/`) must exercise:
- [ ] Top-level class with fields and methods
- [ ] `interface` (must be `Trait`, not `Struct`)
- [ ] `abstract class` (`Struct`)
- [ ] Inner class (non-static, `Contains` edge from outer)
- [ ] Static nested class (`Contains` edge from outer)
- [ ] `enum` with constants
- [ ] Constructor (`Method` named after class)
- [ ] `import` single and static
- [ ] `implements` clause (`Implements` edge)
- [ ] `extends` clause (`Uses` edge)
- [ ] Annotations on methods (`@Override` — must not suppress the method node)
- [ ] Generic class (`class Foo<T>` — name must be `Foo`, not `Foo<T>`)

## Common bugs to watch

- Inner class nodes missing → most common Java parser gap, breaks search for inner types
- `interface` mapped to `Struct` → `Implements` edge semantics broken
- Generic params in node name → lookup by name fails
- Annotations suppressing the annotated node → annotated methods/classes disappear
- Constructor not emitted → `new Foo()` callers have no target
- Enum constants not emitted as `Constant` nodes → enum members invisible
