#!/bin/bash
# Channel-number input, candidate CPU stages and authoring comparison on real Metal.
set -euo pipefail
cd "$(dirname "$0")/.."
output="${1:-target/timeline-number-benchmark-$(date +%Y%m%d-%H%M%S)}"
samples="${2:-96}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
if [[ -e "$output/number-probe.json" || -e "$output/trace.jsonl" ]]; then
    echo "Refusing to overwrite earlier number-probe measurements in $output" >&2
    exit 1
fi
{
    sw_vers
    sysctl hw.model machdep.cpu.brand_string hw.memsize
    git rev-parse HEAD
    git status --short
    git -C ../kasane-2d rev-parse HEAD
    git -C ../kasane-2d status --short
} > "$output/environment.txt"
unset MTL_HUD_ENABLED KASANE_DEV_SESSION_DIR KASANE_WINDOW_BENCH
export KASANE_TIMELINE_OUTPUT="$output" KASANE_TIMELINE_SAMPLES="$samples"
export KASANE_TIMELINE_REQUIRE_ASSET=1
export KASANE_TIMELINE_NUMBER_PROBE="${KASANE_TIMELINE_NUMBER_PROBE:-source}"
export KASANE_FRAME_TRACE="$output/trace.jsonl"
cargo test --locked --release -p kasane-editor --features test-support --test timeline_acceptance > "$output/run.log" 2>&1
python3 scripts/animation-trace-report.py "$output/trace.jsonl" > "$output/trace-summary.json"
cat "$output/number-probe.json"
