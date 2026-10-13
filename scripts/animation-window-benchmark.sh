#!/bin/bash
# Native display callbacks, GPU workers and compositor presentation timestamps.
set -euo pipefail
cd "$(dirname "$0")/.."
model="${1:?usage: scripts/animation-window-benchmark.sh MODEL [OUTPUT_DIR] [SAMPLES]}"
output="${2:-target/animation-window-benchmark-$(date +%Y%m%d-%H%M%S)}"
export KASANE_BENCH_ANIMATION=1 KASANE_BENCH_PARAMETERS=0 KASANE_BENCH_DEFORMATIONS=0
# The 60 below converts samples to duration; it does not set playback cadence.
exec bash scripts/window-benchmark.sh "$model" "$output" "${3:-600}" 60
