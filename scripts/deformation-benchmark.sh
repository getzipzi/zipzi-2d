#!/bin/bash
# Real-window mesh and deformer editing; shares the window benchmark trace/report.
set -euo pipefail
cd "$(dirname "$0")/.."
model="${1:?usage: scripts/deformation-benchmark.sh MODEL [OUTPUT_DIR] [SAMPLES] [HZ]}"
output="${2:-target/deformation-benchmark-$(date +%Y%m%d-%H%M%S)}"
export KASANE_BENCH_DEFORMATIONS=1
export KASANE_BENCH_PARAMETERS=0
exec bash scripts/window-benchmark.sh "$model" "$output" "${3:-300}" "${4:-120}"
