#!/usr/bin/env bash
# lang-smoke.sh — clone a repo, index it with the locally-built `gcx`, run a
# battery of queries, and print a PASS/FAIL report with metrics.
#
# Reusable across languages: pass a repo URL and a probe symbol that should
# exist in that repo. The script is deliberately dependency-free (bash + git +
# python3 for the MCP round-trip).
#
# Usage:
#   scripts/lang-smoke.sh <git-url> <probe-symbol> [clone-name]
#
# Examples:
#   scripts/lang-smoke.sh https://github.com/psf/requests          Session
#   scripts/lang-smoke.sh https://github.com/gin-gonic/gin         Engine
#   scripts/lang-smoke.sh https://github.com/expressjs/express     Router
#   scripts/lang-smoke.sh https://github.com/spring-projects/spring-petclinic Owner
#
# Exit code: 0 if all checks pass, 1 otherwise.

set -u

REPO_URL="${1:?usage: lang-smoke.sh <git-url> <probe-symbol> [clone-name]}"
PROBE="${2:?need a probe symbol that exists in the repo}"
NAME="${3:-$(basename "$REPO_URL" .git)}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GCX="$ROOT/target/release/gcx"
WORK="/tmp/gcx-smoke/$NAME"

pass=0; fail=0
ok()   { echo "  PASS  $1"; pass=$((pass+1)); }
bad()  { echo "  FAIL  $1"; fail=$((fail+1)); }
hdr()  { echo; echo "== $1 =="; }

[ -x "$GCX" ] || { echo "build first: cargo build --release -p gitcortex"; exit 1; }

hdr "Clone $NAME"
rm -rf "$WORK"; mkdir -p "$(dirname "$WORK")"
if git clone --depth 1 "$REPO_URL" "$WORK" 2>/dev/null; then ok "cloned"; else bad "clone"; exit 1; fi
cd "$WORK" || exit 1
BR="$(git rev-parse --abbrev-ref HEAD)"
PYFILES=$(find . \( -name '*.py' -o -name '*.go' -o -name '*.rs' -o -name '*.ts' -o -name '*.js' -o -name '*.java' \) | wc -l | tr -d ' ')
echo "  files=$PYFILES branch=$BR"

hdr "Index (timed)"
"$GCX" clean >/dev/null 2>&1
T0=$(date +%s)
INIT_OUT="$("$GCX" init 2>&1)"
T1=$(date +%s)
echo "$INIT_OUT" | grep -E "Graph:|nodes|edges" || true
SECS=$((T1-T0))
NODES=$(echo "$INIT_OUT" | grep -oE '[0-9]+ nodes' | grep -oE '[0-9]+' | head -1)
echo "  elapsed=${SECS}s nodes=${NODES:-?}"
[ "${NODES:-0}" -gt 0 ] 2>/dev/null && ok "indexed (${SECS}s)" || bad "index produced no nodes"

hdr "Queries"
q() { "$GCX" query "$@" --branch "$BR" 2>&1; }
LU="$(q lookup-symbol "$PROBE")"
echo "$LU" | grep -q "$PROBE" && ok "lookup-symbol $PROBE" || bad "lookup-symbol $PROBE"
SE="$(q search "$PROBE" --limit 5)"
echo "$SE" | grep -q "$PROBE" && ok "search $PROBE" || bad "search $PROBE"
WK="$(q wiki "$PROBE")"
echo "$WK" | grep -q "^# " && ok "wiki $PROBE" || bad "wiki $PROBE"
# Docstring/newline integrity: no collapsed `.\n` -> `n` artifact heuristic.
echo "$WK" | grep -qE '[a-z]\.nn[A-Z]' && bad "wiki doc newline collapse detected" || ok "wiki doc newlines intact"
TR="$(q tour --limit 5)"
echo "$TR" | grep -q "Tour (" && ok "tour" || bad "tour"

hdr "MCP round-trip (tools/list + one call)"
python3 - "$GCX" "$PROBE" <<'PY'
import subprocess, json, threading, time, sys
gcx, probe = sys.argv[1], sys.argv[2]
p=subprocess.Popen([gcx,"serve"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,bufsize=1)
resp={}
def rd():
    for ln in p.stdout:
        ln=ln.strip()
        if ln:
            try:
                m=json.loads(ln)
                if "id" in m: resp[m["id"]]=m
            except: pass
threading.Thread(target=rd,daemon=True).start()
def s(o): p.stdin.write(json.dumps(o)+"\n"); p.stdin.flush()
s({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}})
time.sleep(0.4); s({"jsonrpc":"2.0","method":"notifications/initialized"}); time.sleep(0.2)
s({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
s({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_code","arguments":{"query":probe,"limit":3}}})
time.sleep(1.2)
tools=[t["name"] for t in resp.get(2,{}).get("result",{}).get("tools",[])]
print(f"  PASS  mcp tools/list ({len(tools)} tools)" if len(tools)>=10 else "  FAIL  mcp tools/list")
hit=resp.get(3,{}).get("result",{}).get("structuredContent",{}).get("count",0)
print("  PASS  mcp search_code call" if hit else "  FAIL  mcp search_code call")
p.terminate()
PY

hdr "Result: $NAME"
echo "  pass=$pass fail=$fail  (index ${SECS}s, ${NODES:-?} nodes)"
[ "$fail" -eq 0 ] && { echo "  ALL GREEN"; exit 0; } || { echo "  HAS FAILURES"; exit 1; }
