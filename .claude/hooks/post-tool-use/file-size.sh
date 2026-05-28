#!/usr/bin/env sh
# GitCortex PostToolUse hook — warns when a .rs file exceeds the 800-line limit.
# Zero output (zero cost) when file is under 600 lines.
set -e

input=$(cat)

file_path=$(printf '%s' "$input" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" \
  2>/dev/null || true)

[ -z "$file_path" ] && exit 0

case "$file_path" in
  *.rs) ;;
  *) exit 0 ;;
esac

[ -f "$file_path" ] || exit 0

lines=$(wc -l < "$file_path" | tr -d ' ')

if [ "$lines" -gt 800 ]; then
  printf '## File Size Warning\n\n⚠ `%s` has **%s lines** (limit: 800). Consider splitting by feature/domain per coding conventions.\n' \
    "$file_path" "$lines"
elif [ "$lines" -gt 600 ]; then
  printf '## File Size Notice\n\nℹ `%s` has **%s lines** (approaching the 800-line limit).\n' \
    "$file_path" "$lines"
fi
