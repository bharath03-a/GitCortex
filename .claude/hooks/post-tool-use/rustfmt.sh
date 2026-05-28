#!/usr/bin/env sh
# GitCortex PostToolUse hook — runs `rustfmt` on .rs files after Edit/Write.
# Prevents `cargo fmt --check` drift in CI.
set -e
export PATH="$HOME/.cargo/bin:/usr/local/bin:$PATH"

input=$(cat)

file_path=$(printf '%s' "$input" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" \
  2>/dev/null || true)

[ -z "$file_path" ] && exit 0
case "$file_path" in
  *.rs) ;;
  *) exit 0 ;;
esac

command -v rustfmt >/dev/null 2>&1 || exit 0

# Format silently; never fail the tool call on a fmt error
rustfmt --edition 2021 "$file_path" 2>/dev/null || true
