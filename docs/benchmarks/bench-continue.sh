#!/usr/bin/env bash
# bench-continue.sh — run the next missing benchmark round for stable-v063.
#
# Each invocation runs ONE round (all 5 repos) and exits. Call every 6 hours
# via cron to accumulate rounds until 3 complete rounds exist.
#
# Usage: bench-continue.sh [model] [n_questions]
# Env:   GCX, WORK, THROTTLE, COMPACT (all passed through to real-harness.sh)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL="${1:-claude-haiku-4-5-20251001}"
NQ="${2:-4}"
export GCX="${GCX:-$(cd "$HERE/../.." && pwd)/target/release/gcx}"
export WORK="${WORK:-/tmp/gcx-bench/work}"
export THROTTLE="${THROTTLE:-5}"
export COMPACT="${COMPACT:-1}"
OUT="$HERE/stable-v063"

REPOS=(
  "https://github.com/BurntSushi/ripgrep"
  "https://github.com/psf/requests"
  "https://github.com/honojs/hono"
  "https://github.com/spf13/cobra"
  "https://github.com/google/gson"
)

mkdir -p "$OUT" "$WORK"

[ -x "$GCX" ] || { echo "ERROR: gcx binary not found at $GCX"; exit 1; }

# Find next round: lowest R where any canonical repo is missing r{R}-{repo}.json
MAX_ROUNDS=3
next_round=0
for ((r=1; r<=MAX_ROUNDS; r++)); do
  missing=0
  for url in "${REPOS[@]}"; do
    name=$(basename "$url")
    [[ -f "$OUT/r$r-$name.json" ]] || { missing=1; break; }
  done
  if [[ $missing -eq 1 ]]; then
    next_round=$r
    break
  fi
done

if [[ $next_round -eq 0 ]]; then
  echo "All $MAX_ROUNDS rounds complete in $OUT — nothing to do."
  echo "Aggregating:"
  python3 "$HERE/stable-agg.py" "$OUT"
  exit 0
fi

echo "=== bench-continue: running round $next_round/$MAX_ROUNDS ==="
echo "model=$MODEL  q=$NQ  compact=$COMPACT  out=$OUT"

for url in "${REPOS[@]}"; do
  name=$(basename "$url")
  out_file="$OUT/r$next_round-$name.json"
  if [[ -f "$out_file" ]]; then
    echo ">>> r$next_round $name — already exists, skipping"
    continue
  fi
  echo ">>> r$next_round $name"
  log="$WORK/bench-continue-r$next_round-$name.log"
  bash "$HERE/real-harness.sh" "$url" "$out_file" "$MODEL" "$NQ" \
    >"$log" 2>&1
  tail -1 "$log"
  echo "<<< r$next_round $name done"
done

echo "=== round $next_round done ==="
echo ""
echo "--- Aggregate so far ---"
python3 "$HERE/stable-agg.py" "$OUT"
