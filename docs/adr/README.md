# Architecture Decision Records

Lightweight design notes capturing *why* we made notable architectural choices, written at the time of the choice. New ADRs are added when:

- A decision is irreversible-ish (changing it would require touching many files)
- A decision will surprise a future contributor
- We rejected an obvious-looking alternative

## Format

Each ADR is one Markdown file:

```
0NNN-short-slug.md
```

With sections:
- **Status** (Proposed / Accepted / Superseded by ADR-XXXX)
- **Context** — what problem made this decision necessary
- **Decision** — the choice we made
- **Consequences** — good + bad outcomes we accepted
- **Alternatives considered** (optional)
- **See also** — file paths, external docs

## Index

| ADR | Title |
|---|---|
| [0001](0001-graphstore-trait.md) | `GraphStore` as a trait, not a concrete type |
| [0002](0002-tokio-async-boundary.md) | `tokio` lives only at the I/O boundary |
| [0003](0003-cosmograph-renderer.md) | Cosmograph (GPGPU) for the viz renderer |
| [0004](0004-duckdb-cdn-runtime-dep.md) | DuckDB-WASM is loaded from a CDN at runtime |
