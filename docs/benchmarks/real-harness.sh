#!/usr/bin/env bash
# REAL token benchmark: runs Claude Code twice per question and reads the actual
# token usage the API reports — no chars/4 proxy.
#
#   Arm A (baseline): Claude with grep/read tools only. It explores the repo by
#                     hand to answer, the way it does today.
#   Arm B (gcx):      Claude with the GitCortex MCP graph tools (+Read). It
#                     answers via graph queries instead of grepping.
#
# For each arm we capture usage.{input,cache_creation,cache_read,output}_tokens,
# total_cost_usd and num_turns from `claude -p --output-format json`.
#
# Usage: real-harness.sh <repo-url> <output-json> [model] [n_questions]
#
# Env:
#   GCX   path to gcx release binary
#   WORK  scratch dir for clones
set -u

REPO_URL="${1:?repo url required}"
OUT_JSON="${2:?output json path required}"
case "$OUT_JSON" in /*) ;; *) OUT_JSON="$PWD/$OUT_JSON" ;; esac
MODEL="${3:-claude-haiku-4-5-20251001}"
N_Q="${4:-7}"
BUDGET="${BUDGET:-1.50}"          # hard per-session $ cap (haiku: ~$0.08/q; sonnet: raise to 5.00)
GCX="${GCX:-/Users/bharathvelamala/Documents/Open Source/GitCortex/target/release/gcx}"
WORK="${WORK:-/tmp/gcx-bench/work}"

mkdir -p "$WORK"
REPO_NAME=$(basename "$REPO_URL" .git)
REPO_DIR="$WORK/$REPO_NAME"

if [ ! -d "$REPO_DIR" ]; then
  git clone --depth 1 --quiet "$REPO_URL" "$REPO_DIR" 2>&1 \
    || { echo "{\"error\":\"clone failed\",\"repo\":\"$REPO_NAME\"}" > "$OUT_JSON"; exit 0; }
fi
cd "$REPO_DIR" || exit 1

# Index the repo so the gcx arm has a graph to query.
mkdir -p .gitcortex
cat > .gitcortex/config.toml <<EOF
[index]
languages = ["rust", "go", "python", "typescript", "java"]
max_file_size_kb = 500
[lld]
enabled = false
[store]
backend = "local"
EOF
"$GCX" init >/dev/null 2>&1 || true
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")

# MCP config that exposes the GitCortex graph as the "gcx" server.
MCP_GCX="$REPO_DIR/.mcp-gcx.json"
# COMPACT=1 → only expose the single gcx dispatch tool (lower schema overhead)
if [ "${COMPACT:-0}" = "1" ]; then
  MCP_ARGS='["serve","--compact"]'
else
  MCP_ARGS='["serve"]'
fi
cat > "$MCP_GCX" <<EOF
{"mcpServers":{"gcx":{"command":"$GCX","args":$MCP_ARGS}}}
EOF
MCP_EMPTY='{"mcpServers":{}}'

# Pick real, central symbols from the graph to fill the question templates.
pick_symbols() {
  "$GCX" query tour --branch "$BRANCH" --limit 30 2>/dev/null \
    | sed -nE 's/^[0-9]+\. `([^`]+)`.*/\1/p' \
    | awk '!seen[$0]++' | head -10
}
SYMBOLS=($(pick_symbols))
SYM_TYPE="${SYMBOLS[0]:-Main}"
SYM_FN="${SYMBOLS[1]:-${SYMBOLS[0]:-init}}"
SYM_OTHER="${SYMBOLS[2]:-${SYMBOLS[1]:-${SYMBOLS[0]:-run}}}"
PICK_TERM="parse"
grep -qrI --include='*.rs' --include='*.go' --include='*.py' --include='*.ts' --include='*.java' \
  -e 'auth' . 2>/dev/null && PICK_TERM="auth"

# 4 developer questions chosen for a balanced story:
#   Q1 search  — gcx consistently wins (graph beats grep on discovery)
#   Q2 tour    — gcx wins (structured summary vs. raw file list)
#   Q3 refactor — marginal / honest (shows limits on high-fan-out symbols)
#   Q4 subgraph — sometimes loses (honest: dumping a big neighbourhood)
Q_LABELS=(search_concept tour_onboarding refactor_impact subgraph_around)
Q_TEXT=(
  "Where in this codebase is '$PICK_TERM' handled? List the relevant files and symbols."
  "Give me a concise tour of this codebase: what are the main components and how do they fit together?"
  "If I change '$SYM_FN', what breaks? List the direct callers and any important indirect callers."
  "Show everything directly connected to '$SYM_TYPE' — what calls it, what it calls, what it uses."
)

# Run one Claude session. Echoes a JSON usage object to stdout.
#   $1 = arm tag (baseline|gcx)  $2 = question text
run_arm() {
  local arm="$1" q="$2" raw
  local common=(-p "$q" --output-format json --no-session-persistence
                --model "$MODEL" --max-budget-usd "$BUDGET" --strict-mcp-config)
  if [ "$arm" = "gcx" ]; then
    raw=$(env -u CLAUDECODE -u CLAUDE_CODE_SSE_PORT claude "${common[@]}" \
      --mcp-config "$MCP_GCX" \
      --allowed-tools "Read mcp__gcx" \
      --disallowed-tools "Grep Glob Bash Edit Write WebSearch WebFetch" 2>/dev/null)
  else
    raw=$(env -u CLAUDECODE -u CLAUDE_CODE_SSE_PORT claude "${common[@]}" \
      --mcp-config "$MCP_EMPTY" \
      --allowed-tools "Read Grep Glob Bash(grep:*) Bash(rg:*) Bash(find:*) Bash(cat:*) Bash(ls:*) Bash(head:*) Bash(sed:*)" \
      --disallowed-tools "Edit Write WebSearch WebFetch mcp__gcx" 2>/dev/null)
  fi
  printf '%s' "$raw" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    u = d.get('usage', {})
    # 'input' = unique context loaded (fresh prompt + tokens written to cache once).
    # cache_read is excluded: it is cheap re-reads of context already counted,
    # and summing it across turns double-counts. Tracked separately for reference.
    inp = u.get('input_tokens',0)+u.get('cache_creation_input_tokens',0)
    cache_read = u.get('cache_read_input_tokens',0)
    out = u.get('output_tokens',0)
    print(json.dumps({'input':inp,'output':out,'total':inp+out,
                      'cache_read':cache_read,
                      'cost':round(d.get('total_cost_usd',0),5),
                      'turns':d.get('num_turns',0),
                      'error':bool(d.get('is_error'))}))
except Exception as e:
    print(json.dumps({'input':0,'output':0,'total':0,'cost':0,'turns':0,'error':True,'parse_error':str(e)}))
"
}

QUESTIONS_JSON=""
N=${#Q_LABELS[@]}
[ "$N_Q" -lt "$N" ] && N="$N_Q"
for ((i=0; i<N; i++)); do
  label="${Q_LABELS[$i]}"; text="${Q_TEXT[$i]}"
  echo "  [$((i+1))/$N] $label :: baseline ..." >&2
  base=$(run_arm baseline "$text")
  echo "  [$((i+1))/$N] $label :: gcx ..." >&2
  gcx=$(run_arm gcx "$text")
  q=$(python3 -c "
import json,sys
b=json.loads('''$base'''); g=json.loads('''$gcx''')
ratio = round(b['total']/g['total'],2) if g['total'] else 0
saved = b['total']-g['total']
print(json.dumps({'q':'$label','question':'''$text''','baseline':b,'gcx':g,
                  'token_ratio':ratio,'tokens_saved':saved}))
")
  QUESTIONS_JSON="${QUESTIONS_JSON:+$QUESTIONS_JSON,}$q"
  echo "      base=$(echo "$base" | python3 -c 'import json,sys;print(json.load(sys.stdin)["total"])') tok  gcx=$(echo "$gcx" | python3 -c 'import json,sys;print(json.load(sys.stdin)["total"])') tok" >&2
done

STATUS=$("$GCX" status 2>/dev/null || true)
NODES=$(echo "$STATUS" | awk '/^nodes:/{print $2; exit}')
EDGES=$(echo "$STATUS" | awk '/^edges:/{print $2; exit}')

NODES_VAL="${NODES:-0}"
EDGES_VAL="${EDGES:-0}"
echo "[$QUESTIONS_JSON]" > "$REPO_DIR/.real-q.json"
python3 - "$OUT_JSON" "$REPO_DIR/.real-q.json" <<PY
import json, sys
qs = json.load(open(sys.argv[2]))
tb = sum(q['baseline']['total'] for q in qs)
tg = sum(q['gcx']['total'] for q in qs)
cb = round(sum(q['baseline']['cost'] for q in qs),5)
cg = round(sum(q['gcx']['cost'] for q in qs),5)
import math
ratios = [q['token_ratio'] for q in qs if q['token_ratio']>0]
geo = round(math.exp(sum(math.log(r) for r in ratios)/len(ratios)),2) if ratios else 0
out = {
  "repo": "$REPO_NAME", "url": "$REPO_URL", "branch": "$BRANCH",
  "model": "$MODEL", "measured": "real_claude_usage", "compact": ${COMPACT:-0},
  "symbols": {"type": "$SYM_TYPE", "fn": "$SYM_FN", "other": "$SYM_OTHER", "concept": "$PICK_TERM"},
  "nodes": $NODES_VAL, "edges": $EDGES_VAL,
  "totals": {
    "baseline_tokens": tb, "gcx_tokens": tg,
    "saved_tokens": tb-tg,
    "saved_pct": round(100*(tb-tg)/tb,2) if tb else 0,
    "baseline_cost_usd": cb, "gcx_cost_usd": cg,
    "geomean_ratio": geo,
  },
  "questions": qs,
}
json.dump(out, open(sys.argv[1],"w"), indent=2)
print(f"$REPO_NAME [{'$MODEL'}]  baseline={tb} gcx={tg} saved={tb-tg} "
      f"({out['totals']['saved_pct']}%)  geomean={geo}x  cost \${cb}->\${cg}")
PY
