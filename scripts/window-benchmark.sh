#!/bin/bash
# Run a real-window benchmark with an isolated app configuration.
set -euo pipefail
cd "$(dirname "$0")/.."
model="${1:?usage: scripts/window-benchmark.sh MODEL [OUTPUT_DIR] [SAMPLES] [HZ]}"
output="${2:-target/window-benchmark-$(date +%Y%m%d-%H%M%S)}"
samples="${3:-300}"
hz="${4:-120}"
profile="${KASANE_BENCH_PROFILE:-release}"
case "$profile" in
    release) cargo_profile=release ;;
    debug) cargo_profile=dev ;;
    *) echo "KASANE_BENCH_PROFILE must be release or debug" >&2; exit 2 ;;
esac
mkdir -p "$output"
output="$(cd "$output" && pwd)"
if [[ -e "$output/trace.jsonl" ]]; then
    echo "Refusing to overwrite $output/trace.jsonl" >&2
    exit 1
fi
if ioreg -n Root -d1 | rg '"IOConsoleLocked" = Yes' > /dev/null; then
    echo "Unlock the Mac before running the window benchmark." >&2
    exit 2
fi
cargo build --locked --profile "$cargo_profile" -p kasane-editor > "$output/build.log" 2>&1
{
    sw_vers
    sysctl hw.model machdep.cpu.brand_string hw.memsize
    system_profiler SPDisplaysDataType
    git rev-parse HEAD
    git status --short
    git -C ../kasane-2d rev-parse HEAD
    git -C ../kasane-2d status --short
} > "$output/environment.txt"
# GPU HUD changes the workload. No external input automation is used.
unset MTL_HUD_ENABLED KASANE_DEV_BACKGROUND KASANE_DEV_PID_FILE
export KASANE_CONFIG_DIR="$output/config"
export KASANE_FRAME_TRACE="$output/trace.jsonl"
export KASANE_WINDOW_BENCH=1 KASANE_BENCH_MODEL="$model"
export KASANE_BENCH_SAMPLES="$samples" KASANE_BENCH_HZ="$hz"
caffeinate -d -i "./target/$profile/kasane-editor" > "$output/application.log" 2>&1
if [[ "${KASANE_BENCH_ANIMATION:-0}" == 1 ]]; then
    python3 scripts/animation-trace-report.py "$output/trace.jsonl" > "$output/summary.json"
    cat "$output/summary.json"
else
    python3 scripts/frame-trace-report.py "$output/trace.jsonl" --output "$output"
fi
