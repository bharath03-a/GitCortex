# Safe Refactoring with Dependency Mapping

Use the knowledge graph to plan refactors in the right order and avoid breaking changes.

## Workflow

1. **Map current structure** — `list_definitions` on every file in the module being refactored
2. **Find all dependents** — `find_callers` and `lookup_symbol` to identify callers and uses
3. **Check trait implementations** — look for structs that implement traits you're changing
4. **Plan the order** — change leaf nodes first (no callers), then work toward roots
5. **Verify after** — `branch_diff_graph(from: "main", to: "HEAD")` to confirm only intended nodes changed

## Patterns safe to refactor
- Private functions with zero external callers
- Structs used in only one file
- Methods on a struct with a single `impl` block

## Patterns that need care
- Public trait methods — every implementor must be updated
- Functions called from many files — run impact analysis first
- Structs that implement multiple traits — changing fields affects all trait impls

## When to use
- Extracting a module into a separate crate
- Renaming a public API across many files
- Changing a function signature that many callers depend on
