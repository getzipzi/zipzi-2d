#!/bin/bash
# Mesh properties through native GPUI controls and the real Metal compositor.
set -euo pipefail
cd "$(dirname "$0")/.."
export KASANE_BENCH_PROPERTIES=1
exec ./scripts/window-benchmark.sh \
    "${KASANE_BENCH_MODEL:-models/local/mao/runtime/mao_pro.model3.json}" \
    "${1:-target/properties-benchmark-$(date +%Y%m%d-%H%M%S)}" \
    "${2:-300}" "${3:-120}"
