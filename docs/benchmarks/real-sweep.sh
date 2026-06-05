#!/usr/bin/env bash
# Release-gate sweep: run the REAL token benchmark across one repo per language,
# then render the tool x language scorecard.
#
# Run this before every release. Compare the scorecard to the previous run; a
# tool flipping from win to lose is a regression to fix before shipping.
#
# Usage: real-sweep.sh [model] [n_questions]
#   model        default claude-haiku-4-5-20251001 (cheap, token volume is
#                roughly model-independent). Use a sonnet/opus id for a
#                production-fidelity run.
#   n_questions  default 7
#
# Env:
#   PARALLEL  how many repos to run concurrently (default 2). Higher = faster
#             but more API contention / cost spikes.
#   GCX       path to gcx release binary
#   WORK      scratch dir for clones
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL="${1:-claude-haiku-4-5-20251001}"
NQ="${2:-7}"
PARALLEL="${PARALLEL:-2}"
export GCX="${GCX:-$(cd "$HERE/../.." && pwd)/target/release/gcx}"
export WORK="${WORK:-/tmp/gcx-bench/work}"
export BUDGET="${BUDGET:-0.75}"

# One canonical repo per language.
REPOS=(
  "https://github.com/BurntSushi/ripgrep"   # Rust
  "https://github.com/django/django"        # Python
  "https://github.com/honojs/hono"          # TypeScript
  "https://github.com/spf13/cobra"          # Go
  "https://github.com/google/gson"          # Java
)

echo "Real sweep · model=$MODEL · questions=$NQ · parallel=$PARALLEL"
echo "Binary: $GCX"
[ -x "$GCX" ] || { echo "ERROR: gcx binary not found/executable at $GCX"; exit 1; }

run_one() {
  local url="$1" name
  name=$(basename "$url")
  echo ">>> $name"
  bash "$HERE/real-harness.sh" "$url" "$HERE/real-$name.json" "$MODEL" "$NQ" \
    > "$WORK/real-$name.log" 2>&1
  echo "<<< $name done"
}
export -f run_one
export HERE MODEL NQ

# Throttled parallelism: keep at most $PARALLEL jobs in flight.
running=0
for url in "${REPOS[@]}"; do
  run_one "$url" &
  running=$((running + 1))
  if [ "$running" -ge "$PARALLEL" ]; then
    wait -n 2>/dev/null || wait
    running=$((running - 1))
  fi
done
wait

echo "=== SWEEP DONE ==="
python3 "$HERE/real-report.py" "$HERE"
