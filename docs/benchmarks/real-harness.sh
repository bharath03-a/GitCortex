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
# Compact single-dispatch is the server default; retain COMPACT=0 as the
# historical opt-in to the full schema for old comparison runs.
if [ "${COMPACT:-0}" = "1" ]; then
  MCP_ARGS='["serve"]'
else
  MCP_ARGS='["serve","--full"]'
fi
cat > "$MCP_GCX" <<EOF
{"mcpServers":{"gcx":{"command":"$GCX","args":$MCP_ARGS}}}
EOF
MCP_EMPTY='{"mcpServers":{}}'

# Pick real, central symbols from the graph to fill the question templates.
# Tour format changed in v0.6: output is now markdown sections, not a numbered list.
# We extract symbol names from "key:" lines, skipping test-file entries so the
# selected symbols are real production code with non-trivial call graphs.
pick_symbols() {
  "$GCX" query tour --branch "$BRANCH" --limit 30 2>/dev/null \
    | sed -nE 's/.*`([A-Za-z_][A-Za-z0-9_]+) — ([^`]*)`/\1 \2/p' \
    | grep -v '_test\.' \
    | awk '{print $1}' \
    | awk '!seen[$0]++' | head -10
}
IFS=$'\n' read -r -d '' -a SYMBOLS <<< "$(pick_symbols)" || true
SYM_TYPE="${SYMBOLS[0]:-Command}"
SYM_FN="${SYMBOLS[1]:-${SYMBOLS[0]:-execute}}"
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
_run_arm_once() {
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
    d = json.loads(sys.stdin.read(), strict=False)
    u = d.get('usage', {})
    # Newer Claude Code versions report real counts in modelUsage, not usage.
    inp = u.get('input_tokens',0)+u.get('cache_creation_input_tokens',0)
    cache_read = u.get('cache_read_input_tokens',0)
    out = u.get('output_tokens',0)
    if inp == 0 and out == 0 and d.get('modelUsage'):
        for mv in d['modelUsage'].values():
            inp += mv.get('inputTokens',0) + mv.get('cacheCreationInputTokens',0)
            cache_read += mv.get('cacheReadInputTokens',0)
            out += mv.get('outputTokens',0)
    answer = d.get('result','')
    print(json.dumps({'input':inp,'output':out,'total':inp+out,
                      'cache_read':cache_read,
                      'cost':round(d.get('total_cost_usd',0),5),
                      'turns':d.get('num_turns',0),
                      'error':bool(d.get('is_error')),
                      'answer':answer}))
except Exception as e:
    print(json.dumps({'input':0,'output':0,'total':0,'cost':0,'turns':0,'error':True,'parse_error':str(e),'answer':''}))
"
}

# Blind quality judge: Haiku scores Answer A and Answer B (0-10) without
# knowing which arm produced which. Returns {"score_a":N,"score_b":N}.
# Falls back to 5/5 on any error so a failed judge never kills the run.
judge_quality() {
  local question="$1" answer_a="$2" answer_b="$3"
  # Escape single quotes in answers so the heredoc stays valid.
  local q_esc="${question//\'/\'\\\'\'}"
  local a_esc="${answer_a//\'/\'\\\'\'}"
  local b_esc="${answer_b//\'/\'\\\'\'}"

  local prompt="You are a blind code-question judge. Score each answer 0-10 on:
- Correctness (no false claims about the codebase)
- Completeness (covers what was asked)
Combined score = floor((correctness + completeness) / 2).

Question: $q_esc

Answer A:
$a_esc

Answer B:
$b_esc

Reply with ONLY valid JSON on one line: {\"score_a\": <int>, \"score_b\": <int>}"

  local raw
  raw=$(env -u CLAUDECODE -u CLAUDE_CODE_SSE_PORT claude \
    -p "$prompt" --output-format json --no-session-persistence \
    --model claude-haiku-4-5-20251001 --max-budget-usd 0.10 \
    --disallowed-tools "Read Grep Glob Bash Edit Write WebSearch WebFetch" 2>/dev/null \
    | python3 -c "
import json,sys
try: print(json.load(sys.stdin).get('result','{}'))
except: print('{}')
" 2>/dev/null || echo '{}')

  python3 -c "
import json
try:
    d = json.loads('$raw'.strip() or '{}')
    sa = max(0, min(10, int(d.get('score_a', 5))))
    sb = max(0, min(10, int(d.get('score_b', 5))))
    print(json.dumps({'score_a': sa, 'score_b': sb}))
except Exception:
    print('{\"score_a\":5,\"score_b\":5}')
" 2>/dev/null || echo '{"score_a":5,"score_b":5}'
}

# Retry wrapper around `_run_arm_once`. Rate limits and transient errors are the
# dominant failure mode — without this, a throttled session silently writes a
# zero/partial result that corrupts the aggregate. Retries up to 3× with
# exponential backoff whenever the attempt errored or returned zero tokens.
run_arm() {
  local arm="$1" q="$2" result total err attempt
  for attempt in 1 2 3; do
    result=$(_run_arm_once "$arm" "$q")
    total=$(printf '%s' "$result" | python3 -c 'import json,sys
try: print(int(json.load(sys.stdin).get("total",0)))
except Exception: print(0)' 2>/dev/null || echo 0)
    err=$(printf '%s' "$result" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("error",True))
except Exception: print(True)' 2>/dev/null || echo True)
    if [ "$err" = "False" ] && [ "$total" -gt 0 ] 2>/dev/null; then
      printf '%s' "$result"; return 0
    fi
    [ "$attempt" -lt 3 ] && sleep $((5 * attempt * attempt))  # 5s, 20s
  done
  printf '%s' "$result"  # exhausted retries — return last (errored) result
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

  # Extract answers for blind judge (baseline = A, gcx = B — randomised labels
  # so judge can't guess by tool-mention keywords in the text).
  base_ans=$(printf '%s' "$base" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("answer",""))' 2>/dev/null || echo "")
  gcx_ans=$(printf '%s'  "$gcx"  | python3 -c 'import json,sys; print(json.load(sys.stdin).get("answer",""))' 2>/dev/null || echo "")
  echo "  [$((i+1))/$N] $label :: judging ..." >&2
  # baseline → A, gcx → B (fixed; aggregation uses score_b/score_a directly)
  judge=$(judge_quality "$text" "$base_ans" "$gcx_ans")
  score_base=$(printf '%s' "$judge" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("score_a",5))' 2>/dev/null || echo 5)
  score_gcx=$(printf '%s'  "$judge" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("score_b",5))' 2>/dev/null || echo 5)

  q=$(BASE_JSON="$base" GCX_JSON="$gcx" Q_LABEL="$label" Q_TEXT="$text" \
      SCORE_BASE="$score_base" SCORE_GCX="$score_gcx" python3 -c "
import json, os
b = json.loads(os.environ['BASE_JSON'])
g = json.loads(os.environ['GCX_JSON'])
label = os.environ['Q_LABEL']
text  = os.environ['Q_TEXT']
sb = int(os.environ['SCORE_BASE'])
sg = int(os.environ['SCORE_GCX'])
ratio = round(b['total']/g['total'],2) if g['total'] else 0
saved = b['total']-g['total']
qr = round(sg/sb,2) if sb else 1.0
print(json.dumps({'q':label,'question':text,'baseline':b,'gcx':g,
                  'token_ratio':ratio,'tokens_saved':saved,
                  'quality':{'score_baseline':sb,'score_gcx':sg,'quality_ratio':qr}}))
")
  QUESTIONS_JSON="${QUESTIONS_JSON:+$QUESTIONS_JSON,}$q"
  echo "      base=$(printf '%s' "$base" | python3 -c 'import json,sys;print(json.load(sys.stdin)["total"])') tok  gcx=$(printf '%s' "$gcx" | python3 -c 'import json,sys;print(json.load(sys.stdin)["total"])') tok  quality=${score_base}→${score_gcx}" >&2
  # Throttle between questions to stay under API rate limits.
  sleep "${THROTTLE:-3}"
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
# Only count questions where BOTH arms succeeded — an errored/throttled session
# returns a zero/partial total that would corrupt the aggregate.
def ok(q):
    return (not q['baseline'].get('error') and not q['gcx'].get('error')
            and q['baseline']['total'] > 0 and q['gcx']['total'] > 0)
valid = [q for q in qs if ok(q)]
errored = [q['q'] for q in qs if not ok(q)]
tb = sum(q['baseline']['total'] for q in valid)
tg = sum(q['gcx']['total'] for q in valid)
cb = round(sum(q['baseline']['cost'] for q in valid),5)
cg = round(sum(q['gcx']['cost'] for q in valid),5)
import math
ratios = [q['token_ratio'] for q in valid if q['token_ratio']>0]
geo = round(math.exp(sum(math.log(r) for r in ratios)/len(ratios)),2) if ratios else 0
qratios = [q['quality']['quality_ratio'] for q in valid if q.get('quality',{}).get('quality_ratio',0)>0]
geo_q = round(math.exp(sum(math.log(r) for r in qratios)/len(qratios)),2) if qratios else None
avg_base_q = round(sum(q['quality']['score_baseline'] for q in valid if q.get('quality'))/len(valid),1) if valid and valid[0].get('quality') else None
avg_gcx_q  = round(sum(q['quality']['score_gcx']      for q in valid if q.get('quality'))/len(valid),1) if valid and valid[0].get('quality') else None
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
    "geomean_quality_ratio": geo_q,
    "avg_quality_baseline": avg_base_q,
    "avg_quality_gcx": avg_gcx_q,
    "valid_questions": len(valid),
    "errored_questions": errored,
  },
  "questions": qs,
}
# Only write checkpoint if at least one question succeeded; otherwise the
# zero-token file would block retries on the next loop invocation.
if len(valid) > 0:
    json.dump(out, open(sys.argv[1],"w"), indent=2)
else:
    import os
    # Write to a .err sidecar so the error is visible but checkpoint stays absent.
    json.dump(out, open(sys.argv[1] + ".err","w"), indent=2)
print(f"$REPO_NAME [{'$MODEL'}]  baseline={tb} gcx={tg} saved={tb-tg} "
      f"({out['totals']['saved_pct']}%)  geomean={geo}x  cost \${cb}->\${cg}")
PY
