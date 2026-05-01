# Impact Analysis Before Making Changes

Before modifying a function, struct, or trait — understand everything that depends on it.

## Workflow

1. **Look up the symbol** — `lookup_symbol(name: "YourSymbol")`
2. **Find direct callers** — `find_callers(function_name: "your_function")`
3. **Walk the blast radius** — repeat `find_callers` on each caller; stop when callers are entry points
4. **After changes** — run `gcx blast-radius --base main --head HEAD` for a full risk report

## Risk heuristic
| Caller count | Risk | Recommended action |
|---|---|---|
| 0–2 | LOW | Safe to refactor directly |
| 3–10 | MEDIUM | Add tests for callers before changing |
| 10+ | HIGH | Plan carefully, consider a compatibility shim |
| Core trait method | CRITICAL | All implementors must change — audit every impl |

## When to use
- Before renaming a public function or struct
- Before changing a function signature
- Before modifying a trait definition that has multiple implementors
