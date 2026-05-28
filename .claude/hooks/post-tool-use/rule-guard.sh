#!/usr/bin/env sh
# GitCortex PostToolUse hook — enforces architectural rules from CLAUDE.md.
# Nudges (never blocks) when .unwrap() or async fn appear outside gitcortex-mcp.
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

# Rules only apply outside gitcortex-mcp
case "$file_path" in
  *gitcortex-mcp*) exit 0 ;;
esac

[ -f "$file_path" ] || exit 0

warnings=""

# Rule: no .unwrap() in lib code — use ? or explicit error handling
if grep -q '\.unwrap()' "$file_path" 2>/dev/null; then
  count=$(grep -c '\.unwrap()' "$file_path" 2>/dev/null || echo 0)
  warnings="${warnings}⚠ \`.unwrap()\` found (${count} occurrence(s)) — use \`?\` or explicit error handling per convention.\n"
fi

# Rule: async only in gitcortex-mcp
if grep -q 'async fn' "$file_path" 2>/dev/null; then
  count=$(grep -c 'async fn' "$file_path" 2>/dev/null || echo 0)
  warnings="${warnings}⚠ \`async fn\` found (${count} occurrence(s)) outside \`gitcortex-mcp\` — async is only allowed in the mcp crate per convention.\n"
fi

[ -z "$warnings" ] && exit 0

printf '## Rule Guard\n\n%b\nFile: `%s`\n' "$warnings" "$file_path"
