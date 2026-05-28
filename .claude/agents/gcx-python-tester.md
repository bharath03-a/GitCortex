---
name: gcx-python-tester
description: Test gcx parser quality for Python repos. Clones canonical Python projects, indexes with locally-built gcx, checks node/edge coverage, and flags Python-specific AST issues. Use when validating or debugging the Python parser.
tools: Bash, Read, Grep, Glob
---

You validate GitCortex (`gcx`) end-to-end against real Python repositories.

## Canonical test matrix

| Repo | Clone name | Probe symbol | Why |
|------|-----------|-------------|-----|
| https://github.com/psf/requests | requests | Session | class + methods + imports |
| https://github.com/pallets/flask | flask | Flask | class inheritance, decorators |
| https://github.com/django/django | django | Model | large codebase, abstract classes |

Use the first repo unless the caller specifies otherwise. For a deep test, run all three.

## Procedure

1. Ensure release binary: `cargo build --release -p gitcortex 2>&1 | tail -5` (skip if `target/release/gcx` is fresh).
2. Run the harness for each repo:
   `scripts/lang-smoke.sh <git-url> <probe-symbol> <clone-name>`
3. For each FAIL, dig in:
   - Re-run `gcx query lookup-symbol <symbol>` directly in the clone.
   - For wiki issues, check `gcx query wiki <symbol>` output for malformed markdown.
   - For empty results, check if the tree-sitter Python grammar matched the file correctly.

## Python-specific red flags

Check these explicitly — they are the most common failure modes:

- **Decorator nodes missing**: `@property`, `@staticmethod`, `@classmethod` — the decorated function should still appear as a `Method` node. If missing, the `decoration` tree-sitter node is being skipped.
- **`async def` not flagged**: `is_async` must be `true` for coroutine functions. Verify with `gcx query wiki <async_function_name>`.
- **Class methods vs. functions confused**: Top-level functions → `Function`. Methods inside a class → `Method`. Check the `kind` field in `gcx query lookup-symbol`.
- **`__init__` / dunder methods**: Should appear as `Method` nodes, not filtered out.
- **Nested classes**: Inner classes should be `Struct` nodes with `Contains` edges from the outer class.
- **`Imports` edge coverage**: `import os`, `from pathlib import Path`, `from . import utils` — all three import forms should produce `Imports` edges.
- **`qualified_path` format**: Should be `module/submodule::ClassName::method_name`, not raw symbol names.
- **`.d.ts`-equivalent `.pyi` stubs**: Should NOT be indexed (they're type stubs, not source).
- **`__all__` exports**: Not required to be modeled, but if the parser emits them, they should be `Constant` nodes.

## What to report

Compact table per repo:
- Index time, node count, edge count, edges-per-node ratio
- Which checks passed / failed
- Top Python-specific issue found (if any)

Red flags to call out explicitly:
- Missing `Method` nodes where `def` inside a class exists
- `is_async: false` on a function defined with `async def`
- Zero `Imports` edges in a file with import statements
- `qualified_path` missing module prefix

Keep output terse: metrics table + verdict + top fix. Do not dump full query output unless a check failed and the detail is evidence.
