# ADR 0001 — `GraphStore` as a trait, not a concrete type

**Status:** Accepted (since v0.1)
**Date:** 2026-04-23

## Context

GitCortex needs to persist a code knowledge graph. The initial implementation is KuzuDB-backed, embedded and local. We expect to later need:

- A remote-backed store for teams sharing one graph
- An in-memory store for fast unit tests
- Possibly a SQLite or DuckDB fallback for platforms where KuzuDB doesn't build cleanly

If we hard-coded `KuzuGraphStore` everywhere, swapping the backend would be a workspace-wide refactor.

## Decision

Define a `GraphStore: Send + Sync` trait in `gitcortex-core/src/store.rs`. The trait owns all read and write methods (`apply_diff`, `find_callers`, `branch_diff`, etc.). Concrete implementations live in `gitcortex-store` (today: `KuzuGraphStore`; tomorrow: `MemoryGraphStore`, `RemoteGraphStore`).

The indexer never references a concrete store. The MCP layer takes any `T: GraphStore`. The binary picks the implementation.

## Consequences

**Good**
- Swapping backends is one line in `main.rs`
- Tests can use an in-memory store without linking KuzuDB's C++ statics
- The trait acts as a documented spec for what "a code graph store" means in this project
- Feature flags can compile out unused backends

**Bad**
- Adding a new method requires updating every implementation
- Trait dispatch has a small cost vs direct calls (negligible at our scale)
- `dyn GraphStore` requires methods to be object-safe, which constrains generics

## Alternatives considered

- **Concrete type with `cfg` switches:** rejected — proliferates `#[cfg]` across the indexer
- **Generic over `S: GraphStore` everywhere:** rejected — monomorphisation cost in the binary and ugly turbofish in tests; we use `Box<dyn GraphStore>` at the binary boundary instead

## See also

- `crates/gitcortex-core/src/store.rs` — the trait
- `crates/gitcortex-store/src/kuzu.rs` — the impl
