#!/usr/bin/env bash
# Checkpoint-aware benchmark loop for v0.6.0
#
# Output files act as checkpoints: if r{N}-{repo}.json exists, skip it.
# Rate-limit failures leave the file absent, so the next invocation retries.
# Exit 0 when all rounds complete (also runs aggregation).
# Exit 1 when partial — scheduled trigger will resume in 6 hours.
#
# Usage: loop-sweep.sh [model] [n_questions] [rounds]
# Env:   GCX, WORK, THROTTLE (inter-session sleep, default 5s), COMPACT (default 1)
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL="${1:-claude-haiku-4-5-20251001}"
NQ="${2:-4}"
ROUNDS="${3:-3}"
export GCX="${GCX:-$(cd "$HERE/../.." && pwd)/target/release/gcx}"
export WORK="${WORK:-/tmp/gcx-bench/work}"
export THROTTLE="${THROTTLE:-90}"
export COMPACT="${COMPACT:-1}"

REPOS=(
  "cobra:https://github.com/spf13/cobra"
  "gson:https://github.com/google/gson"
  "ripgrep:https://github.com/BurntSushi/ripgrep"
  "requests:https://github.com/psf/requests"
  "hono:https://github.com/honojs/hono"
)

OUT="$HERE/stable-v062"
mkdir -p "$OUT" "$WORK"

echo "[loop-sweep] START · model=$MODEL · q=$NQ · rounds=$ROUNDS · $(date '+%Y-%m-%d %H:%M %Z')"

if [ ! -x "$GCX" ]; then
  echo "[loop-sweep] ERROR: gcx binary not found at $GCX — rebuilding"
  (cd "$HERE/../.." && cargo build --release --quiet 2>&1 | tail -5)
  [ -x "$GCX" ] || { echo "[loop-sweep] ABORT: build failed"; exit 2; }
fi

total=0 done_count=0 ran=0 failed=0

for ((round=1; round<=ROUNDS; round++)); do
  for entry in "${REPOS[@]}"; do
    name="${entry%%:*}"
    url="${entry##*:}"
    ((total++))
    out="$OUT/r${round}-${name}.json"
    if [ -f "$out" ]; then
      echo "[skip] round=$round repo=$name (checkpoint exists)"
      ((done_count++))
      continue
    fi
    echo "[run] round=$round repo=$name"
    bash "$HERE/real-harness.sh" "$url" "$out" "$MODEL" "$NQ" \
      > "$WORK/loop-r${round}-${name}.log" 2>&1
    if [ -f "$out" ]; then
      echo "[ok]   round=$round repo=$name"
      ((done_count++))
      ((ran++))
    else
      echo "[fail] round=$round repo=$name — log: $WORK/loop-r${round}-${name}.log"
      ((failed++))
    fi
    sleep "$THROTTLE"
  done
done

echo "[loop-sweep] SUMMARY · total=$total done=$done_count ran_this_run=$ran failed=$failed"

if [ "$done_count" -eq "$total" ]; then
  echo "[loop-sweep] ALL ROUNDS COMPLETE — aggregating"
  python3 "$HERE/stable-agg.py" "$OUT"
  echo "[loop-sweep] DONE"
  exit 0
else
  remaining=$((total - done_count))
  echo "[loop-sweep] PARTIAL — $remaining sessions still pending, resuming next trigger"
  exit 1
fi
