#!/bin/bash
# Run a real-window benchmark with an isolated app configuration.
set -euo pipefail
. "$(dirname "$0")/workspace.sh"
model=$(caller_path "${1:?usage: window-benchmark.sh MODEL [OUTPUT_DIR] [SAMPLES] [HZ]}")
output=$(caller_path "${2:-$repo_root/target/window-benchmark-$(date +%Y%m%d-%H%M%S)}")
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
python3 "$repo_root/tools/workspace_tools.py" provenance "$output/provenance.json" --model "$model"
if ioreg -n Root -d1 | rg '"IOConsoleLocked" = Yes' > /dev/null; then
    echo "Unlock the Mac before running the window benchmark." >&2
    exit 2
fi
if [[ "${KASANE_BENCH_PROPERTIES:-0}" == 1 ]]; then
    features=test-support
    if [[ "${KASANE_BENCH_CHAIN_STAGES:-0}" == 1 ]]; then
        features=test-support,bench-instrumentation
    fi
    cargo build --locked --message-format=json-render-diagnostics --profile "$cargo_profile" -p kasane-editor --features "$features" > "$output/build.jsonl" 2> "$output/build.log"
else
    cargo build --locked --message-format=json-render-diagnostics --profile "$cargo_profile" -p kasane-editor > "$output/build.jsonl" 2> "$output/build.log"
fi
{
    sw_vers
    sysctl hw.model machdep.cpu.brand_string hw.memsize
    system_profiler SPDisplaysDataType
} > "$output/environment.txt"
# GPU HUD changes the workload. No external input automation is used.
unset MTL_HUD_ENABLED KASANE_DEV_BACKGROUND KASANE_DEV_PID_FILE
export KASANE_CONFIG_DIR="$output/config"
export KASANE_FRAME_TRACE="$output/trace.jsonl"
export KASANE_WINDOW_BENCH=1 KASANE_BENCH_MODEL="$model"
export KASANE_BENCH_SAMPLES="$samples" KASANE_BENCH_HZ="$hz"
binary=$(python3 "$repo_root/tools/workspace_tools.py" executable "$output/build.jsonl")
caffeinate -d -i "$binary" > "$output/application.log" 2>&1
if [[ "${KASANE_BENCH_PROPERTIES:-0}" == 1 ]]; then
    python3 "$repo_root/tools/properties-trace-report.py" "$output/trace.jsonl" --output "$output"
elif [[ "${KASANE_BENCH_ANIMATION:-0}" == 1 ]]; then
    python3 "$repo_root/tools/animation-trace-report.py" "$output/trace.jsonl" > "$output/summary.json"
    cat "$output/summary.json"
else
    python3 "$repo_root/tools/frame-trace-report.py" "$output/trace.jsonl" --output "$output"
fi
