#!/usr/bin/env bash
# Benchmark harness: measure token savings using GitCortex vs raw file reads.
#
# Usage: bench-harness.sh <repo-url> <output-json>
#
# Assumes:
#   $GCX = path to gcx release binary
#   $WORK = scratch dir for clones
#
# For each of N hand-picked questions per repo we:
#   1. Find the file(s) that contain the answer (via grep — ground truth)
#   2. Compute baseline_tokens = sum of file char counts / 4 (rough tiktoken proxy)
#   3. Run gcx query <subcmd> <arg>, capture output
#   4. Compute gcx_tokens = output char count / 4
#   5. Emit ratio = baseline_tokens / gcx_tokens
#
# Output: JSON object per repo with per-question metrics + geomean ratio.

set -u
REPO_URL="${1:?repo url required}"
OUT_JSON="${2:?output json path required}"
GCX="${GCX:-/Users/bharathvelamala/Documents/Open Source/GitCortex/target/release/gcx}"
WORK="${WORK:-/tmp/gcx-bench/work}"

mkdir -p "$WORK"
REPO_NAME=$(basename "$REPO_URL" .git)
REPO_DIR="$WORK/$REPO_NAME"

if [ ! -d "$REPO_DIR" ]; then
  git clone --depth 1 --quiet "$REPO_URL" "$REPO_DIR" 2>&1 || { echo "{\"error\":\"clone failed\",\"repo\":\"$REPO_NAME\"}" > "$OUT_JSON"; exit 0; }
fi

cd "$REPO_DIR" || exit 1

# Write minimal config to enable all langs (gcx detects from file extensions anyway).
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

# Index (suppress hook installer noise).
"$GCX" init >/dev/null 2>&1 || true

# Token proxy: chars / 4. Good enough for ratios.
toks() { local f="$1"; [ -f "$f" ] || { echo 0; return; }; local c=$(wc -c < "$f" | tr -d ' '); echo $((c / 4)); }
toks_str() { local s="$1"; local c=${#s}; echo $((c / 4)); }

# Get current branch name (gcx uses git's branch name).
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")

# Run a benchmark question.
#   $1 = question label
#   $2 = ground-truth file glob (whatever files would have to be read raw)
#   $3 = gcx subcommand + args
run_q() {
  local label="$1"; local files_glob="$2"; shift 2
  # baseline: sum of token counts of all matching files
  local base=0
  for f in $(eval "echo $files_glob" 2>/dev/null | tr ' ' '\n' | sort -u); do
    [ -f "$f" ] || continue
    base=$((base + $(toks "$f")))
  done
  # gcx: run command, capture, count
  local out
  out=$("$GCX" query "$@" --branch "$BRANCH" 2>&1 || true)
  local gcx_t
  gcx_t=$(toks_str "$out")
  # Avoid div by zero.
  [ "$gcx_t" -eq 0 ] && gcx_t=1
  local ratio
  ratio=$(awk -v b="$base" -v g="$gcx_t" 'BEGIN{ if(g==0) print 0; else printf "%.2f", b/g }')
  printf '{"q":"%s","baseline_tokens":%d,"gcx_tokens":%d,"ratio":%s}' \
    "$label" "$base" "$gcx_t" "$ratio"
}

# Pick a representative symbol that we know exists.
# Strategy: look for the most-defined function/struct name in the index.
PICK=$("$GCX" status 2>/dev/null | head -1 || true)

# Pick ground-truth symbols dynamically from repo. Use ripgrep if available.
RG=$(command -v rg || echo grep)
pick_symbol() {
  # Find first non-trivial function/struct name to query against.
  if [ "$RG" = "rg" ]; then
    rg -INo '^(pub |func |def |class |interface |type |fn )[A-Z][A-Za-z0-9_]+' --no-heading \
       -g '*.rs' -g '*.go' -g '*.py' -g '*.ts' -g '*.java' . 2>/dev/null \
       | awk '{print $NF}' | sort | uniq -c | sort -rn | head -5 | awk '{print $NF}'
  else
    grep -rEho '\b[A-Z][A-Za-z0-9_]{4,}' --include='*.rs' --include='*.go' --include='*.py' --include='*.ts' --include='*.java' . 2>/dev/null \
       | sort | uniq -c | sort -rn | head -5 | awk '{print $NF}'
  fi
}

SYMBOLS=($(pick_symbol))
SYM1="${SYMBOLS[0]:-main}"
SYM2="${SYMBOLS[1]:-init}"

# Pick a file with most definitions.
FILE_PICK=$(find . -type f \( -name '*.rs' -o -name '*.go' -o -name '*.py' -o -name '*.ts' -o -name '*.java' \) \
  -not -path '*/node_modules/*' -not -path '*/target/*' -not -path '*/.git/*' 2>/dev/null \
  | head -50 | xargs wc -l 2>/dev/null | sort -rn | awk 'NR>1 && $2!="total"{print $2; exit}')
FILE_PICK="${FILE_PICK:-./README.md}"

# Files used as baseline for each question (rough: grep for the symbol).
files_with() {
  local sym="$1"
  if [ "$RG" = "rg" ]; then
    rg -l -g '*.rs' -g '*.go' -g '*.py' -g '*.ts' -g '*.java' \
       "\\b${sym}\\b" . 2>/dev/null | head -10 | tr '\n' ' '
  else
    grep -rl --include='*.rs' --include='*.go' --include='*.py' --include='*.ts' --include='*.java' \
       -E "\\b${sym}\\b" . 2>/dev/null | head -10 | tr '\n' ' '
  fi
}

Q1=$(run_q "lookup_${SYM1}" "$(files_with "$SYM1")" lookup-symbol "$SYM1")
Q2=$(run_q "callers_${SYM1}" "$(files_with "$SYM1")" find-callers "$SYM1")
Q3=$(run_q "file_def_${FILE_PICK}" "$FILE_PICK" list-definitions "$FILE_PICK")
Q4=$(run_q "context_${SYM2}" "$(files_with "$SYM2")" symbol-context "$SYM2")
Q5=$(run_q "implementors_${SYM1}" "$(files_with "$SYM1")" find-implementors "$SYM1")

# Aggregate geomean.
GEOMEAN=$(printf '%s\n%s\n%s\n%s\n%s\n' "$Q1" "$Q2" "$Q3" "$Q4" "$Q5" \
  | awk -F'"ratio":' '/ratio/{gsub(/[^0-9.]/,"",$2); if($2+0>0){sum+=log($2); n++}} END{if(n>0) printf "%.2f", exp(sum/n); else print 0}')

# Pull index stats.
STATUS=$("$GCX" status 2>/dev/null || true)
NODES=$(echo "$STATUS" | awk '/^nodes:/{print $2; exit}')
EDGES=$(echo "$STATUS" | awk '/^edges:/{print $2; exit}')

# Emit final JSON.
cat > "$OUT_JSON" <<EOF
{
  "repo": "$REPO_NAME",
  "url": "$REPO_URL",
  "branch": "$BRANCH",
  "symbol_used": "$SYM1",
  "file_used": "$FILE_PICK",
  "nodes": ${NODES:-0},
  "edges": ${EDGES:-0},
  "geomean_ratio": $GEOMEAN,
  "questions": [$Q1,$Q2,$Q3,$Q4,$Q5]
}
EOF

echo "Wrote $OUT_JSON (geomean ratio: ${GEOMEAN}x)"
