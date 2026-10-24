#!/bin/bash
# Channel-number input, candidate CPU stages and authoring comparison on real Metal.
set -euo pipefail
tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$tools_dir/workspace/workspace.sh"
output=$(caller_path "${1:-$repo_root/target/timeline-number-benchmark-$(date +%Y%m%d-%H%M%S)}")
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
} > "$output/environment.txt"
unset MTL_HUD_ENABLED KASANE_DEV_SESSION_DIR KASANE_WINDOW_BENCH
export KASANE_TIMELINE_OUTPUT="$output" KASANE_TIMELINE_SAMPLES="$samples"
export KASANE_TIMELINE_REQUIRE_ASSET=1
export KASANE_TIMELINE_NUMBER_PROBE="${KASANE_TIMELINE_NUMBER_PROBE:-source}"
export KASANE_FRAME_TRACE="$output/trace.jsonl"
if [[ -n "${KASANE_TIMELINE_MODEL:-}" ]]; then
    export KASANE_TIMELINE_MODEL="$(caller_path "$KASANE_TIMELINE_MODEL")"
fi
python3 "$repo_root/tools/workspace/workspace_tools.py" provenance "$output/provenance.json" --model "${KASANE_TIMELINE_MODEL:-$repo_root/models/local/mao/runtime/mao_pro.model3.json}"
cargo test --locked --release -p kasane-editor --features test-support --test timeline_acceptance > "$output/run.log" 2>&1
python3 "$repo_root/tools/benchmarks/animation-trace-report.py" "$output/trace.jsonl" > "$output/trace-summary.json"
cat "$output/number-probe.json"
