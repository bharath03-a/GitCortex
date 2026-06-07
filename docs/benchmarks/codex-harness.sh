#!/usr/bin/env bash
# REAL Codex token benchmark: runs Codex twice per question and reads the
# actual token usage emitted by `codex exec --json`.
#
# Arm A (baseline): Codex may use shell search/read, but is instructed not to use
#                   GitCortex.
# Arm B (gcx):      Codex gets a compact GitCortex MCP server that exposes only
#                   the single dispatch tool (`gcx`) to reduce schema overhead.
#
# Usage: codex-harness.sh <repo-url> <output-json> [model] [n_questions]
#
# Env:
#   GCX   path to gcx release binary
#   WORK  scratch dir for clones
#   CODEX_BYPASS_APPROVALS=1 lets noninteractive Codex execute MCP tools.
set -u

REPO_URL="${1:?repo url required}"
OUT_JSON="${2:?output json path required}"
case "$OUT_JSON" in /*) ;; *) OUT_JSON="$PWD/$OUT_JSON" ;; esac
MODEL="${3:-gpt-5.4-mini}"
N_Q="${4:-4}"
GCX="${GCX:-/Users/bharathvelamala/Documents/Open Source/GitCortex/target/release/gcx}"
WORK="${WORK:-/tmp/gcx-bench/work}"
REASONING="${REASONING:-low}"
LOG_DIR="${LOG_DIR:-/tmp/gcx-bench/logs}"
CODEX_BYPASS_APPROVALS="${CODEX_BYPASS_APPROVALS:-1}"

mkdir -p "$WORK"
mkdir -p "$LOG_DIR"
REPO_NAME=$(basename "$REPO_URL" .git)
REPO_DIR="$WORK/$REPO_NAME"

if [ ! -d "$REPO_DIR" ]; then
  git clone --depth 1 --quiet "$REPO_URL" "$REPO_DIR" 2>&1 \
    || { echo "{\"error\":\"clone failed\",\"repo\":\"$REPO_NAME\"}" > "$OUT_JSON"; exit 0; }
fi
cd "$REPO_DIR" || exit 1

remove_generated_assistant_files() {
  rm -f .codex/config.toml .codex-q.json .real-q.json 2>/dev/null || true
  rmdir .codex 2>/dev/null || true
  if [ -f AGENTS.md ] && grep -q "GitCortex - Codex Guide" AGENTS.md 2>/dev/null; then
    rm -f AGENTS.md
  fi
}

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
remove_generated_assistant_files
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")

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

Q_LABELS=(search_concept tour_onboarding refactor_impact subgraph_around)
Q_TEXT=(
  "Where in this codebase is '$PICK_TERM' handled? List the relevant files and symbols."
  "Give me a concise tour of this codebase: what are the main components and how do they fit together?"
  "If I change '$SYM_FN', what breaks? List the direct callers and any important indirect callers."
  "Show everything directly connected to '$SYM_TYPE' — what calls it, what it calls, what it uses."
)

remove_codex_mcp() {
  rm -f .codex/config.toml 2>/dev/null || true
  rmdir .codex 2>/dev/null || true
}

write_codex_mcp() {
  mkdir -p .codex
  cat > .codex/config.toml <<EOF
[mcp_servers.gitcortex]
command = "$GCX"
args = ["serve", "--compact"]
startup_timeout_sec = 30
EOF
}

parse_codex_usage() {
  node -e '
let input = 0, cached = 0, output = 0, reasoning = 0, turns = 0, final = "", mcpCalls = 0, mcpErrors = 0;
const fs = require("fs");
const text = fs.readFileSync(0, "utf8");
for (const line of text.split(/\n/)) {
  if (!line.startsWith("{")) continue;
  let event;
  try { event = JSON.parse(line); } catch { continue; }
  if (event.type === "item.started" && event.item && event.item.type === "mcp_tool_call" &&
      event.item.server === "gitcortex" && event.item.tool === "gcx") {
    mcpCalls += 1;
  }
  if (event.type === "item.completed" && event.item && event.item.type === "mcp_tool_call" &&
      event.item.server === "gitcortex" && event.item.tool === "gcx" && event.item.error) {
    mcpErrors += 1;
  }
  if (event.type === "turn.completed" && event.usage) {
    turns += 1;
    input += event.usage.input_tokens || 0;
    cached += event.usage.cached_input_tokens || 0;
    output += event.usage.output_tokens || 0;
    reasoning += event.usage.reasoning_output_tokens || 0;
  }
  if (event.type === "item.completed" && event.item && event.item.type === "agent_message") {
    final += event.item.text || "";
  }
}
const total = input + output;
const uncached_total = Math.max(input - cached, 0) + output;
console.log(JSON.stringify({
  input, output, total, uncached_total, cached_input: cached, reasoning_output: reasoning,
  cost: 0, turns, error: turns === 0, final_chars: final.length,
  mcp_calls: mcpCalls, mcp_errors: mcpErrors
}));
'
}

run_arm() {
  local arm="$1" q="$2" label="$3" raw prompt mcp_action mcp_params
  if [ "$arm" = "gcx" ]; then
    write_codex_mcp
    case "$label" in
      search_concept)
        mcp_action="search_code"
        mcp_params="{\"query\":\"$PICK_TERM\",\"limit\":10,\"branch\":\"$BRANCH\"}"
        ;;
      tour_onboarding)
        mcp_action="start_tour"
        mcp_params="{\"limit\":12,\"branch\":\"$BRANCH\"}"
        ;;
      refactor_impact)
        mcp_action="find_callers"
        mcp_params="{\"function_name\":\"$SYM_FN\",\"depth\":2,\"branch\":\"$BRANCH\"}"
        ;;
      subgraph_around)
        mcp_action="get_subgraph"
        mcp_params="{\"seed_name\":\"$SYM_TYPE\",\"depth\":1,\"direction\":\"both\",\"limit\":30,\"branch\":\"$BRANCH\"}"
        ;;
      *)
        mcp_action="search_code"
        mcp_params="{\"query\":\"$PICK_TERM\",\"limit\":10,\"branch\":\"$BRANCH\"}"
        ;;
    esac
    prompt="You are benchmarking GitCortex. Before any shell search, call the MCP tool named mcp__gitcortex__gcx (server gitcortex, tool gcx) with this payload:
{\"action\":\"$mcp_action\",\"params\":$mcp_params}

Use that graph result to narrow exploration, then read source files only to verify details. Do not edit files. Keep the answer concise but complete.

Question: $q"
  else
    remove_codex_mcp
    prompt="You are benchmarking normal codebase exploration. Do not use GitCortex, gcx, or any graph database. Use ordinary source search/read commands only. Do not edit files. Keep the answer concise but complete.

Question: $q"
  fi

  local codex_args=(
    exec --json --ephemeral --ignore-rules
    -m "$MODEL"
    -c "model_reasoning_effort=\"$REASONING\""
  )
  if [ "$CODEX_BYPASS_APPROVALS" = "1" ]; then
    codex_args+=(--dangerously-bypass-approvals-and-sandbox)
  else
    codex_args+=(-s read-only)
  fi
  if [ "$arm" = "gcx" ]; then
    codex_args+=(
      -c "mcp_servers.gitcortex.command=\"$GCX\""
      -c 'mcp_servers.gitcortex.args=["serve","--compact"]'
      -c 'mcp_servers.gitcortex.startup_timeout_sec=30'
    )
  fi
  codex_args+=(-C "$REPO_DIR" "$prompt")

  raw=$(codex "${codex_args[@]}" 2>&1)
  printf "%s" "$raw" > "$LOG_DIR/codex-${REPO_NAME}-${arm}-${label:-question}.jsonl"
  printf "%s" "$raw" | parse_codex_usage
}

QUESTIONS_JSON=""
N=${#Q_LABELS[@]}
[ "$N_Q" -lt "$N" ] && N="$N_Q"
for ((i=0; i<N; i++)); do
  label="${Q_LABELS[$i]}"; text="${Q_TEXT[$i]}"
  echo "  [$((i+1))/$N] $label :: baseline ..." >&2
  base=$(run_arm baseline "$text" "$label")
  echo "  [$((i+1))/$N] $label :: gcx ..." >&2
  gcx=$(run_arm gcx "$text" "$label")
  q=$(node -e "
const b=JSON.parse(process.argv[1]), g=JSON.parse(process.argv[2]);
const ratio = g.total ? Math.round((b.total/g.total)*100)/100 : 0;
const uncached_ratio = g.uncached_total ? Math.round((b.uncached_total/g.uncached_total)*100)/100 : 0;
const saved = b.total - g.total;
const uncached_saved = b.uncached_total - g.uncached_total;
console.log(JSON.stringify({q:'$label', question:process.argv[3], baseline:b, gcx:g,
  token_ratio:ratio, tokens_saved:saved,
  uncached_token_ratio:uncached_ratio, uncached_tokens_saved:uncached_saved}));
" "$base" "$gcx" "$text")
  QUESTIONS_JSON="${QUESTIONS_JSON:+$QUESTIONS_JSON,}$q"
  base_total=$(node -e 'console.log(JSON.parse(process.argv[1]).total)' "$base")
  gcx_total=$(node -e 'console.log(JSON.parse(process.argv[1]).total)' "$gcx")
  echo "      base=${base_total} tok  gcx=${gcx_total} tok" >&2
done

write_codex_mcp
STATUS=$("$GCX" status 2>/dev/null || true)
NODES=$(echo "$STATUS" | awk '/^nodes:/{print $2; exit}')
EDGES=$(echo "$STATUS" | awk '/^edges:/{print $2; exit}')
export REPO_NAME REPO_URL BRANCH MODEL SYM_TYPE SYM_FN SYM_OTHER PICK_TERM NODES EDGES

printf '[%s]' "$QUESTIONS_JSON" > "$REPO_DIR/.codex-q.json"
node - "$OUT_JSON" "$REPO_DIR/.codex-q.json" <<'NODE'
const fs = require("fs");
const outPath = process.argv[2];
const qPath = process.argv[3];
const qs = JSON.parse(fs.readFileSync(qPath, "utf8"));
const sum = (fn) => qs.reduce((n, q) => n + fn(q), 0);
const tb = sum(q => q.baseline.total);
const tg = sum(q => q.gcx.total);
const ub = sum(q => q.baseline.uncached_total || q.baseline.total);
const ug = sum(q => q.gcx.uncached_total || q.gcx.total);
const ratios = qs.map(q => q.token_ratio).filter(v => v > 0);
const uncachedRatios = qs.map(q => q.uncached_token_ratio).filter(v => v > 0);
const geo = ratios.length
  ? Math.round(Math.exp(ratios.reduce((n, r) => n + Math.log(r), 0) / ratios.length) * 100) / 100
  : 0;
const uncachedGeo = uncachedRatios.length
  ? Math.round(Math.exp(uncachedRatios.reduce((n, r) => n + Math.log(r), 0) / uncachedRatios.length) * 100) / 100
  : 0;
const out = {
  repo: process.env.REPO_NAME,
  url: process.env.REPO_URL,
  branch: process.env.BRANCH,
  model: process.env.MODEL,
  measured: "real_codex_exec_usage",
  cost_note: "codex exec JSON usage does not include USD cost; cost fields are zero for report compatibility",
  symbols: {
    type: process.env.SYM_TYPE,
    fn: process.env.SYM_FN,
    other: process.env.SYM_OTHER,
    concept: process.env.PICK_TERM,
  },
  nodes: Number(process.env.NODES || 0),
  edges: Number(process.env.EDGES || 0),
  totals: {
    baseline_tokens: tb,
    gcx_tokens: tg,
    saved_tokens: tb - tg,
    saved_pct: tb ? Math.round(((tb - tg) / tb) * 10000) / 100 : 0,
    baseline_uncached_tokens: ub,
    gcx_uncached_tokens: ug,
    saved_uncached_tokens: ub - ug,
    saved_uncached_pct: ub ? Math.round(((ub - ug) / ub) * 10000) / 100 : 0,
    baseline_cost_usd: 0,
    gcx_cost_usd: 0,
    geomean_ratio: geo,
    uncached_geomean_ratio: uncachedGeo,
  },
  questions: qs,
};
fs.writeFileSync(outPath, JSON.stringify(out, null, 2));
console.log(`${out.repo} [${out.model}] baseline=${tb} gcx=${tg} saved=${tb-tg} (${out.totals.saved_pct}%) geomean=${geo}x`);
console.log(`${out.repo} [${out.model}] uncached baseline=${ub} gcx=${ug} saved=${ub-ug} (${out.totals.saved_uncached_pct}%) geomean=${uncachedGeo}x`);
NODE
