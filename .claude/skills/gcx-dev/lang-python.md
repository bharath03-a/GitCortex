---
name: lang-python
description: Python AST expert for gcx-indexer: tree-sitter node types, NodeKind/EdgeKind mapping, decorator and async edge cases. Use when implementing or debugging the Python parser in crates/gitcortex-indexer/src/parser/python.rs.
---

# Python Language Expert — gcx-indexer

## tree-sitter grammar node types → gcx NodeKind

| tree-sitter node | gcx NodeKind | Notes |
|------------------|-------------|-------|
| `module` | `File` | top-level; one per file |
| `class_definition` | `Struct` | covers `class Foo:` and `class Foo(Base):` |
| `function_definition` | `Function` (top-level) or `Method` (inside class) | determined by parent scope |
| `async_function_definition` | same as above + `is_async: true` | |
| `decorated_definition` | transparent wrapper — unwrap to inner `function_definition` or `class_definition` | decorators are NOT a separate NodeKind |
| `import_statement` | produces `Imports` edges | `import os` |
| `import_from_statement` | produces `Imports` edges | `from pathlib import Path` |
| `assignment` (module-level) | `Constant` | for `FOO = 42`, `BAR: int = 0` |
| `type_alias_statement` | `TypeAlias` | Python 3.12+ `type Alias = ...` |

## NodeKind mapping rules

**Function vs Method distinction** — critical:
- Walk up the CST. If the nearest enclosing scope is a `class_definition` body, → `Method`.
- If top-level or inside a plain `block` (e.g. inside `if __name__ == "__main__"`), → `Function`.

**Decorated functions/classes**:
- `decorated_definition` wraps the real node. Extract the inner `function_definition` or `class_definition` to determine kind.
- The decorator name (e.g. `property`, `staticmethod`, `classmethod`) can be stored in metadata but does NOT change `NodeKind`.
- Do NOT skip decorated definitions — they are the most common Python pattern.

**`__dunder__` methods**: Include as `Method` nodes. `__init__`, `__repr__`, `__eq__` are important for callers analysis.

**Nested classes**: A `class_definition` inside another class body → `Struct` with `Contains` edge from the outer `Struct`.

## EdgeKind mapping

| Relationship | EdgeKind | tree-sitter trigger |
|-------------|----------|---------------------|
| File contains top-level def | `Contains` | `module` → function/class |
| Class contains method | `Contains` | `class_definition` body → `function_definition` |
| Class contains nested class | `Contains` | `class_definition` body → `class_definition` |
| `import X` / `from X import Y` | `Imports` | `import_statement`, `import_from_statement` |
| Function calls another | `Calls` | `call` node — best-effort name resolution |
| Class inherits (base classes) | `Uses` | `argument_list` of `class_definition` |
| Type annotation references | `Uses` | `type` in `parameters`, `return_type` |

## `qualified_path` format

```
<module_path>::<ClassName>::<method_name>
```

- Module path derives from the file path relative to repo root, slashes → `::`, `.py` stripped.
- Example: `src/requests/sessions.py` → module path `src::requests::sessions`
- Full qualified_path: `src::requests::sessions::Session::send`

## Known tree-sitter quirks

1. **`decorated_definition` wraps the real node**: Always unwrap — the name and body are on the inner node.
2. **Multiline strings as docstrings**: The first `expression_statement` in a function/class body whose value is a `string` is the docstring. Extract it for `NodeMetadata` doc field.
3. **`async for` / `async with`**: These are `for_statement` / `with_statement` under an `async_statement` — not `async_function_definition`. Do not set `is_async` from these.
4. **Lambda**: `lambda x: x + 1` — a `lambda` expression does NOT produce a `Function` node (it's anonymous and inline).
5. **Comprehensions**: `[x for x in ...]` — do NOT produce nodes. They're expressions.
6. **`@dataclass` classes**: Still a `class_definition` — treat as `Struct`. Fields become `Constant` children if they have type annotations at class body level.
7. **`__all__` list**: Module-level `__all__ = [...]` → `Constant` node. Do not special-case it beyond that.
8. **Relative imports**: `from . import utils`, `from ..auth import Token` — produce `Imports` edges. The path is relative; emit the literal import string as the target name.

## Test fixture checklist

A good Python fixture (`tests/integration/fixtures/python/`) must exercise:
- [ ] Top-level function
- [ ] Class with methods (including `__init__`)
- [ ] Async function (`async def`)
- [ ] Decorated function (`@property`, `@staticmethod`)
- [ ] Nested class inside a class
- [ ] `import X` and `from X import Y`
- [ ] Type annotations on function params and return type
- [ ] Module-level constant (`FOO = 42`)
- [ ] Class that inherits from another class

## Common bugs to watch

- Parser skips `decorated_definition` and misses the function/class inside → zero nodes for decorated code
- `async_function_definition` treated as `Function` with `is_async: false` → metadata wrong
- Methods inside classes emitted as `Function` instead of `Method` → callers/wiki broken
- `import_from_statement` producing no `Imports` edges → search misses dependencies
