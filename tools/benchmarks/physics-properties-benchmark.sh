#!/bin/bash
# Physics inspector controls through native GPUI input and Metal composition.
set -euo pipefail
benchmark_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$benchmark_dir/../.." && pwd)
export ZIPZI_BENCH_PROPERTY_SUITE=physics
export ZIPZI_BENCH_WARMUP="${ZIPZI_BENCH_WARMUP:-5}"
samples="${2:-40}"
if [[ ! "$samples" =~ ^[0-9]+$ || ! "$ZIPZI_BENCH_WARMUP" =~ ^[0-9]+$ ]]; then
    echo "Physics samples and warmup must be nonnegative integers" >&2
    exit 2
fi
if (( 10#$samples + 10#$ZIPZI_BENCH_WARMUP > 50 )); then
    echo "Physics samples plus warmup must be at most 50 to preserve Undo history" >&2
    exit 2
fi
exec "$benchmark_dir/properties-benchmark.sh" \
    "${1:-$repo_root/target/physics-properties-benchmark-$(date +%Y%m%d-%H%M%S)}" \
    "$samples" "${3:-120}"
