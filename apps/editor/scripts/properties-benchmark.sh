#!/bin/bash
# Mesh properties through native GPUI controls and the real Metal compositor.
set -euo pipefail
. "$(dirname "$0")/workspace.sh"
export KASANE_BENCH_PROPERTIES=1
exec "$editor_root/scripts/window-benchmark.sh" \
    "$(caller_path "${KASANE_BENCH_MODEL:-$repo_root/models/local/mao/runtime/mao_pro.model3.json}")" \
    "$(caller_path "${1:-$repo_root/target/properties-benchmark-$(date +%Y%m%d-%H%M%S)}")" \
    "${2:-300}" "${3:-120}"
