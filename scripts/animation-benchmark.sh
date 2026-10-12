#!/bin/bash
# Real headless EditorView + SDK/Metal workers; does not measure compositor presentation.
set -euo pipefail
cd "$(dirname "$0")/.."
output="${1:-target/animation-benchmark-$(date +%Y%m%d-%H%M%S)}"
samples="${2:-600}"
shift "$(( $# < 2 ? $# : 2 ))"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
if [[ -e "$output/trace.jsonl" ]]; then
    echo "Refusing to overwrite $output/trace.jsonl" >&2
    exit 1
fi
{
    sw_vers
    sysctl hw.model machdep.cpu.brand_string hw.memsize
    system_profiler SPDisplaysDataType
    git rev-parse HEAD
    git status --short
    git -C ../kasane-2d rev-parse HEAD
    git -C ../kasane-2d status --short
} > "$output/environment.txt"
unset MTL_HUD_ENABLED KASANE_DEV_SESSION_DIR KASANE_WINDOW_BENCH
export KASANE_ANIMATION_OUTPUT="$output"
export KASANE_ANIMATION_SAMPLES="$samples"
export KASANE_FRAME_TRACE="$output/trace.jsonl"
cargo test --locked --release -p kasane-editor --test animation_acceptance -- "$@" > "$output/run.log" 2>&1
python3 scripts/animation-trace-report.py "$output/trace.jsonl" > "$output/summary.json"
cat "$output/summary.json"
