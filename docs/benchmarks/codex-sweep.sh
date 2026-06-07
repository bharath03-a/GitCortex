#!/usr/bin/env bash
# Release-gate Codex sweep: run the real Codex token benchmark across one repo
# per language, then render the existing HTML scorecard over codex-*.json by
# copying them into an isolated report directory as real-*.json.
#
# Usage: codex-sweep.sh [model] [n_questions]
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL="${1:-gpt-5.4-mini}"
NQ="${2:-4}"
PARALLEL="${PARALLEL:-1}"
export GCX="${GCX:-$(cd "$HERE/../.." && pwd)/target/release/gcx}"
export WORK="${WORK:-/tmp/gcx-bench/work}"
export REASONING="${REASONING:-low}"

REPOS=(
  "https://github.com/BurntSushi/ripgrep"
  "https://github.com/tiangolo/fastapi"
  "https://github.com/honojs/hono"
  "https://github.com/spf13/cobra"
  "https://github.com/google/gson"
)

echo "Codex sweep · model=$MODEL · questions=$NQ · parallel=$PARALLEL · reasoning=$REASONING"
echo "Binary: $GCX"
[ -x "$GCX" ] || { echo "ERROR: gcx binary not found/executable at $GCX"; exit 1; }

run_one() {
  local url="$1" name
  name=$(basename "$url")
  echo ">>> $name"
  bash "$HERE/codex-harness.sh" "$url" "$HERE/codex-$name.json" "$MODEL" "$NQ" \
    > "$WORK/codex-$name.log" 2>&1
  echo "<<< $name done"
}
export -f run_one
export HERE MODEL NQ

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

REPORT_DIR="$HERE/codex-report-data"
rm -rf "$REPORT_DIR"
mkdir -p "$REPORT_DIR"
for f in "$HERE"/codex-*.json; do
  [ -f "$f" ] || continue
  base=$(basename "$f")
  case "$base" in
    *-smoke.json) continue ;;
  esac
  cp "$f" "$REPORT_DIR/real-${base#codex-}"
done

echo "=== CODEX SWEEP DONE ==="
python3 "$HERE/real-report.py" "$REPORT_DIR" -o "$HERE/codex-report.html"
