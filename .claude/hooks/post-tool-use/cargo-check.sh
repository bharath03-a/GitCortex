#!/usr/bin/env sh
# GitCortex PostToolUse hook — runs cargo check on the affected crate after editing .rs files.
# Injects compile errors as context so Claude sees them immediately (red-squiggly equivalent).
set -e
export PATH="$HOME/.cargo/bin:/usr/local/bin:$PATH"

input=$(cat)

file_path=$(printf '%s' "$input" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" \
  2>/dev/null || true)

[ -z "$file_path" ] && exit 0

# Only .rs files
case "$file_path" in
  *.rs) ;;
  *) exit 0 ;;
esac

# Extract crate name from path: crates/<crate>/... or absolute .../crates/<crate>/...
case "$file_path" in
  crates/*)
    crate=$(printf '%s' "$file_path" | cut -d'/' -f2)
    ;;
  */crates/*)
    crate=$(printf '%s' "$file_path" | sed 's|.*/crates/\([^/]*\)/.*|\1|')
    ;;
  *)
    exit 0
    ;;
esac

[ -z "$crate" ] && exit 0
command -v cargo >/dev/null 2>&1 || exit 0

# Single-crate check is 2–5s vs 10–30s for --workspace
output=$(timeout 30 cargo check -p "$crate" --message-format short 2>&1 | head -30 || true)

# Only inject context if there are errors (zero output = zero cost)
case "$output" in
  *"error["*)
    printf '## Cargo Check (%s)\n\n```\n%s\n```\n' "$crate" "$output"
    ;;
esac
