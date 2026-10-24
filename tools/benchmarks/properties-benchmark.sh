#!/bin/bash
# Mesh properties through native GPUI controls and the real Metal compositor.
set -euo pipefail
tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$tools_dir/workspace/workspace.sh"
export KASANE_BENCH_PROPERTIES=1
exec "$repo_root/tools/benchmarks/window-benchmark.sh" \
    "$(caller_path "${KASANE_BENCH_MODEL:-$repo_root/models/local/mao/runtime/mao_pro.model3.json}")" \
    "$(caller_path "${1:-$repo_root/target/properties-benchmark-$(date +%Y%m%d-%H%M%S)}")" \
    "${2:-300}" "${3:-120}"
