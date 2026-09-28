#!/bin/bash
# Compare batched GPUI paths with a dedicated Metal overlay plus GPUI composition.
set -euo pipefail
cd "$(dirname "$0")/.."
output="${1:-target/mesh-overlay-$(date +%Y%m%d-%H%M%S)}"
samples="${2:-100}"
grids="${3:-8,32,64,128,256}"
sizes="${4:-1280x800,2560x1600}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
if [[ -e "$output/samples.jsonl" ]]; then
    echo "Refusing to overwrite $output/samples.jsonl" >&2
    exit 1
fi
cargo build --locked --release -p kasane-editor --example mesh_overlay_benchmark > "$output/build.log" 2>&1
{
    sw_vers
    sysctl hw.model machdep.cpu.brand_string hw.memsize
    git rev-parse HEAD
    git status --short
} > "$output/environment.txt"
unset MTL_HUD_ENABLED
caffeinate -d -i ./target/release/examples/mesh_overlay_benchmark "$output" "$samples" "$grids" "$sizes" > "$output/run.log" 2>&1
