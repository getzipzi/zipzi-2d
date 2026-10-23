#!/bin/bash
# Real GPUI input and SDK/Metal canvas acceptance; excludes compositor presentation.
set -euo pipefail
. "$(dirname "$0")/workspace.sh"
output=$(caller_path "${1:-$repo_root/target/timeline-benchmark-$(date +%Y%m%d-%H%M%S)}")
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
} > "$output/environment.txt"
unset MTL_HUD_ENABLED KASANE_DEV_SESSION_DIR KASANE_WINDOW_BENCH
export KASANE_TIMELINE_OUTPUT="$output"
export KASANE_TIMELINE_SAMPLES="$samples"
export KASANE_TIMELINE_REQUIRE_ASSET=1
if [[ -n "${KASANE_TIMELINE_MODEL:-}" ]]; then
    export KASANE_TIMELINE_MODEL="$(caller_path "$KASANE_TIMELINE_MODEL")"
fi
python3 "$repo_root/tools/workspace_tools.py" provenance "$output/provenance.json" --model "${KASANE_TIMELINE_MODEL:-$repo_root/models/local/mao/runtime/mao_pro.model3.json}"
cargo test --locked --release -p kasane-editor --features test-support --test timeline_acceptance > "$output/run.log" 2>&1
cat "$output/results.json"
