#!/bin/bash
# Real GPUI input and SDK/Metal canvas acceptance; excludes compositor presentation.
set -euo pipefail
cd "$(dirname "$0")/.."
output="${1:-target/timeline-benchmark-$(date +%Y%m%d-%H%M%S)}"
samples="${2:-600}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
if [[ -e "$output/results.json" ]]; then
    echo "Refusing to overwrite $output/results.json" >&2
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
export KASANE_TIMELINE_OUTPUT="$output"
export KASANE_TIMELINE_SAMPLES="$samples"
export KASANE_TIMELINE_REQUIRE_ASSET=1
cargo test --locked --release -p kasane-editor --features test-support --test timeline_acceptance > "$output/run.log" 2>&1
cat "$output/results.json"
