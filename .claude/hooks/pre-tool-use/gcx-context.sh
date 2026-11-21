#!/usr/bin/env sh
# GitCortex: adds call-graph context when the agent reads a source file.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"
command -v gcx >/dev/null 2>&1 || exit 0
# Exit 2 would block the read; an older gcx without this command exits 2.
gcx __hook-context 2>/dev/null || true
