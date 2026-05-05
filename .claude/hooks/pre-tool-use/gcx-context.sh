#!/usr/bin/env sh
# GitCortex PreToolUse hook — appends call-graph context when Claude reads a source file.
set -e
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

input=$(cat)

# Extract file_path from the JSON input (uses python3 for reliable JSON parsing)
file_path=$(printf '%s' "$input" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" \
  2>/dev/null || true)

[ -z "$file_path" ] && exit 0
command -v gcx >/dev/null 2>&1 || exit 0

# Silent — only prints when the file is indexed; ignored otherwise
gcx query context "$file_path" 2>/dev/null || true
