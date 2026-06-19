#!/usr/bin/env bash
# Stable token benchmark: run R rounds across one repo per language, then
# aggregate with the MEDIAN per (repo, question) to kill run-to-run noise.
#
# Runs sequentially (one session at a time) with throttling — rate limits are
# the dominant failure mode, and the harness already retries per session. This
# trades wall-clock for trustworthy data.
#
# Usage: stable-sweep.sh [model] [n_questions] [rounds]
# Env:   GCX, WORK, THROTTLE (inter-session sleep, default 3s), COMPACT (default 1)
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL="${1:-claude-haiku-4-5-20251001}"
NQ="${2:-4}"
ROUNDS="${3:-3}"
export GCX="${GCX:-$(cd "$HERE/../.." && pwd)/target/release/gcx}"
export WORK="${WORK:-/tmp/gcx-bench/work}"
export THROTTLE="${THROTTLE:-3}"
export COMPACT="${COMPACT:-1}"

REPOS=(
  "https://github.com/BurntSushi/ripgrep"
  "https://github.com/psf/requests"
  "https://github.com/honojs/hono"
  "https://github.com/spf13/cobra"
  "https://github.com/google/gson"
)

OUT="$HERE/stable"
mkdir -p "$OUT" "$WORK"
echo "Stable sweep · model=$MODEL · q=$NQ · rounds=$ROUNDS · compact=$COMPACT · throttle=${THROTTLE}s"
[ -x "$GCX" ] || { echo "ERROR: gcx binary not found at $GCX"; exit 1; }

for ((round=1; round<=ROUNDS; round++)); do
  echo "=== ROUND $round/$ROUNDS ==="
  for url in "${REPOS[@]}"; do
    name=$(basename "$url")
    echo ">>> r$round $name"
    bash "$HERE/real-harness.sh" "$url" "$OUT/r$round-$name.json" "$MODEL" "$NQ" \
      > "$WORK/stable-r$round-$name.log" 2>&1
    echo "<<< r$round $name done"
  done
  echo "=== ROUND $round DONE ==="
done

echo "=== ALL STABLE ROUNDS DONE ==="
python3 "$HERE/stable-agg.py" "$OUT"
