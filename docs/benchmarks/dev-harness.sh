#!/usr/bin/env bash
# Developer-style benchmark: simulates the 7 questions a real developer
# asks an AI editor when opening an unfamiliar repo.
#
# Each question's "baseline" is the set of files an LLM would have to read
# (via grep + cat) to answer manually. Token proxy = chars / 4.
#
# Usage: dev-harness.sh <repo-url> <output-json>

set -u
REPO_URL="${1:?repo url required}"
OUT_JSON="${2:?output json path required}"
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

# Cross-platform file search helper. We can't rely on `rg` being on PATH inside
# automated environments, so use BSD/GNU grep with --include globs and emit a
# null-separated list to handle paths with spaces.
files_matching() {
  # $1 = regex (already escaped)
  grep -lr --include='*.rs' --include='*.go' --include='*.py' --include='*.ts' --include='*.java' \
       --exclude-dir=node_modules --exclude-dir=target --exclude-dir=.git --exclude-dir=vendor \
       -E "$1" . 2>/dev/null
}

# tokens = chars/4
toks_str() { local c=${#1}; echo $((c / 4)); }
toks_files() {
  # Accept newline-separated file list on stdin (more robust than $@ when
  # rg output contains spaces / many entries).
  local total=0 f c
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    [ -f "$f" ] || continue
    c=$(wc -c < "$f" | tr -d ' ')
    total=$((total + c / 4))
  done
  echo $total
}

# Top symbols — pull from gcx tour (centrality-ranked, real graph nodes).
pick_symbols() {
  "$GCX" query tour --branch "$BRANCH" --limit 30 2>/dev/null \
    | sed -nE 's/^[0-9]+\. `([^`]+)`.*/\1/p' \
    | awk '!seen[$0]++' | head -10
}
SYMBOLS=($(pick_symbols))
SYM_TYPE="${SYMBOLS[0]:-Main}"
SYM_FN="${SYMBOLS[1]:-${SYMBOLS[0]:-init}}"
SYM_OTHER="${SYMBOLS[2]:-${SYMBOLS[1]:-${SYMBOLS[0]:-run}}}"

# Pick a concept term based on repo language commonality.
PICK_TERM="parse"
[ -n "$(files_matching 'auth' | head -1)" ] && PICK_TERM="auth"

# JSON helpers.
json_q() {
  printf '{"q":"%s","question":"%s","baseline_tokens":%d,"gcx_tokens":%d,"ratio":%s}' \
    "$1" "$2" "$3" "$4" "$5"
}

# ── Q1: Onboarding tour ────────────────────────────────────────────────────────
# Real dev opens repo first time, asks "what is this codebase about?"
# Baseline: LLM would read top 10 largest source files to map architecture.
BASELINE_FILES=$(find . -type f \( -name '*.rs' -o -name '*.go' -o -name '*.py' -o -name '*.ts' -o -name '*.java' \) \
  -not -path '*/node_modules/*' -not -path '*/target/*' -not -path '*/.git/*' -not -path '*/vendor/*' 2>/dev/null \
  | xargs wc -c 2>/dev/null | sort -rn | head -11 | awk 'NR>1 && $2!="total"{print $2}')
BASE_Q1=$(echo "$BASELINE_FILES" | toks_files)
GCX_OUT=$("$GCX" query tour --branch "$BRANCH" --limit 10 2>&1 || true)
GCX_Q1=$(toks_str "$GCX_OUT"); [ "$GCX_Q1" -eq 0 ] && GCX_Q1=1
R_Q1=$(awk -v b="$BASE_Q1" -v g="$GCX_Q1" 'BEGIN{printf "%.1f", b/g}')

# ── Q2: Find anything related to <concept> ─────────────────────────────────────
# "Where in the codebase is X handled?" — semantic search.
# Baseline: grep -l <term> then cat all matched files.
Q2_FILES=$(files_matching "$PICK_TERM" | head -10)
BASE_Q2=$(echo "$Q2_FILES" | toks_files)
GCX_OUT=$("$GCX" query search --branch "$BRANCH" --limit 15 "$PICK_TERM" 2>&1 || true)
GCX_Q2=$(toks_str "$GCX_OUT"); [ "$GCX_Q2" -eq 0 ] && GCX_Q2=1
R_Q2=$(awk -v b="$BASE_Q2" -v g="$GCX_Q2" 'BEGIN{printf "%.1f", b/g}')

# ── Q3: "Explain symbol X" (deep understanding) ────────────────────────────────
# Baseline: file containing definition + every file that mentions the symbol.
Q3_FILES=$(files_matching "\\b${SYM_TYPE}\\b" | head -10)
BASE_Q3=$(echo "$Q3_FILES" | toks_files)
GCX_OUT=$("$GCX" query wiki --branch "$BRANCH" "$SYM_TYPE" 2>&1 || true)
GCX_Q3=$(toks_str "$GCX_OUT"); [ "$GCX_Q3" -eq 0 ] && GCX_Q3=1
R_Q3=$(awk -v b="$BASE_Q3" -v g="$GCX_Q3" 'BEGIN{printf "%.1f", b/g}')

# ── Q4: "If I refactor X, what breaks?" (deep callers) ─────────────────────────
# Baseline: must read every file mentioning X recursively (proxy: all mention files).
Q4_FILES=$(files_matching "\\b${SYM_FN}\\b" | head -15)
BASE_Q4=$(echo "$Q4_FILES" | toks_files)
GCX_OUT=$("$GCX" query find-callers --branch "$BRANCH" --depth 3 "$SYM_FN" 2>&1 || true)
GCX_Q4=$(toks_str "$GCX_OUT"); [ "$GCX_Q4" -eq 0 ] && GCX_Q4=1
R_Q4=$(awk -v b="$BASE_Q4" -v g="$GCX_Q4" 'BEGIN{printf "%.1f", b/g}')

# ── Q5: "How does data flow from A to B?" (trace path) ─────────────────────────
# Baseline: must read files containing either name.
Q5_FILES=$(files_matching "\\b${SYM_FN}\\b|\\b${SYM_OTHER}\\b" | head -15)
BASE_Q5=$(echo "$Q5_FILES" | toks_files)
GCX_OUT=$("$GCX" query trace-path --branch "$BRANCH" "$SYM_FN" "$SYM_OTHER" 2>&1 || true)
GCX_Q5=$(toks_str "$GCX_OUT"); [ "$GCX_Q5" -eq 0 ] && GCX_Q5=1
R_Q5=$(awk -v b="$BASE_Q5" -v g="$GCX_Q5" 'BEGIN{printf "%.1f", b/g}')

# ── Q6: "Show me the neighborhood around X (2 hops)" ───────────────────────────
# Baseline: file containing X + all imports — approximate as Q3 files.
BASE_Q6=$BASE_Q3
GCX_OUT=$("$GCX" query get-subgraph --branch "$BRANCH" --depth 2 "$SYM_TYPE" 2>&1 || true)
GCX_Q6=$(toks_str "$GCX_OUT"); [ "$GCX_Q6" -eq 0 ] && GCX_Q6=1
R_Q6=$(awk -v b="$BASE_Q6" -v g="$GCX_Q6" 'BEGIN{printf "%.1f", b/g}')

# ── Q7: "What dead code exists?" ───────────────────────────────────────────────
# Baseline: must read the entire codebase (impractical, but that's the point).
ALL_FILES=$(find . -type f \( -name '*.rs' -o -name '*.go' -o -name '*.py' -o -name '*.ts' -o -name '*.java' \) \
  -not -path '*/node_modules/*' -not -path '*/target/*' -not -path '*/.git/*' -not -path '*/vendor/*' 2>/dev/null | head -200)
BASE_Q7=$(echo "$ALL_FILES" | toks_files)
GCX_OUT=$("$GCX" query find-unused --branch "$BRANCH" --limit 30 2>&1 || true)
GCX_Q7=$(toks_str "$GCX_OUT"); [ "$GCX_Q7" -eq 0 ] && GCX_Q7=1
R_Q7=$(awk -v b="$BASE_Q7" -v g="$GCX_Q7" 'BEGIN{printf "%.1f", b/g}')

# ── Aggregate ──────────────────────────────────────────────────────────────────
TOTAL_BASE=$((BASE_Q1 + BASE_Q2 + BASE_Q3 + BASE_Q4 + BASE_Q5 + BASE_Q6 + BASE_Q7))
TOTAL_GCX=$((GCX_Q1 + GCX_Q2 + GCX_Q3 + GCX_Q4 + GCX_Q5 + GCX_Q6 + GCX_Q7))
[ "$TOTAL_GCX" -eq 0 ] && TOTAL_GCX=1
TOTAL_RATIO=$(awk -v b="$TOTAL_BASE" -v g="$TOTAL_GCX" 'BEGIN{printf "%.1f", b/g}')

# Geomean across all 7 ratios.
GEOMEAN=$(awk -v r1="$R_Q1" -v r2="$R_Q2" -v r3="$R_Q3" -v r4="$R_Q4" -v r5="$R_Q5" -v r6="$R_Q6" -v r7="$R_Q7" \
  'BEGIN{n=0; s=0; for(i=1;i<=7;i++){v=ARGV[i]+0; if(v>0){s+=log(v); n++}} if(n>0) printf "%.1f", exp(s/n); else print 0}' \
  "$R_Q1" "$R_Q2" "$R_Q3" "$R_Q4" "$R_Q5" "$R_Q6" "$R_Q7")

# Sums for SAVED column.
SAVED_TOKENS=$((TOTAL_BASE - TOTAL_GCX))
SAVED_PCT=$(awk -v s="$SAVED_TOKENS" -v t="$TOTAL_BASE" 'BEGIN{if(t==0) print 0; else printf "%.2f", 100*s/t}')

STATUS=$("$GCX" status 2>/dev/null || true)
NODES=$(echo "$STATUS" | awk '/^nodes:/{print $2; exit}')
EDGES=$(echo "$STATUS" | awk '/^edges:/{print $2; exit}')

Q1=$(json_q "tour_onboarding"  "Give me a tour of this codebase"                     "$BASE_Q1" "$GCX_Q1" "$R_Q1")
Q2=$(json_q "search_concept"   "Find code related to '$PICK_TERM'"                   "$BASE_Q2" "$GCX_Q2" "$R_Q2")
Q3=$(json_q "wiki_explain"     "Explain $SYM_TYPE"                                    "$BASE_Q3" "$GCX_Q3" "$R_Q3")
Q4=$(json_q "refactor_impact"  "If I change $SYM_FN, what breaks? (3 hops)"           "$BASE_Q4" "$GCX_Q4" "$R_Q4")
Q5=$(json_q "trace_flow"       "How does $SYM_FN reach $SYM_OTHER?"                   "$BASE_Q5" "$GCX_Q5" "$R_Q5")
Q6=$(json_q "subgraph_around"  "Show 2-hop neighborhood around $SYM_TYPE"             "$BASE_Q6" "$GCX_Q6" "$R_Q6")
Q7=$(json_q "find_dead_code"   "What dead code exists?"                               "$BASE_Q7" "$GCX_Q7" "$R_Q7")

cat > "$OUT_JSON" <<EOF
{
  "repo": "$REPO_NAME",
  "url": "$REPO_URL",
  "branch": "$BRANCH",
  "symbols": {"type": "$SYM_TYPE", "fn": "$SYM_FN", "other": "$SYM_OTHER", "concept": "$PICK_TERM"},
  "nodes": ${NODES:-0},
  "edges": ${EDGES:-0},
  "totals": {
    "baseline_tokens": $TOTAL_BASE,
    "gcx_tokens": $TOTAL_GCX,
    "saved_tokens": $SAVED_TOKENS,
    "saved_pct": $SAVED_PCT,
    "sum_ratio": $TOTAL_RATIO,
    "geomean_ratio": $GEOMEAN
  },
  "questions": [$Q1,$Q2,$Q3,$Q4,$Q5,$Q6,$Q7]
}
EOF

echo "$REPO_NAME  baseline=${TOTAL_BASE} gcx=${TOTAL_GCX} saved=${SAVED_TOKENS} (${SAVED_PCT}%)  geomean=${GEOMEAN}x"
