#!/bin/sh
set -eu
. "$(dirname "$0")/workspace.sh"

usage() {
    cat <<'EOF'
Usage: ./tools/test-editor.sh [fast|ui|regression|acceptance|tools|full] [test arguments...]

  fast        editor-core unit and synthetic integration tests
  ui          Editor libraries/headless UI tests, with the full feature graph
  regression  Editor libraries and synthetic integration tests
  acceptance  licensed model, native rendering and screenshot tests
  tools       Python tooling tests
  full        original complete checks, including doctests/examples (default)

Examples:
  ./tools/test-editor.sh fast timeline::
  ./tools/test-editor.sh ui headless_tests::ui_performance
  ./tools/test-editor.sh regression --no-run
EOF
}

mode=${1:-full}
if [ "$#" -gt 0 ]; then shift; fi
case "$mode" in
    fast)
        exec env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --lib --test integration "$@"
        ;;
    ui)
        exec env -u KASANE_DEV_SESSION_DIR python3 "$repo_root/tools/cargo_domain.py" editor test --locked --features kasane-editor/test-support --lib "$@"
        ;;
    regression)
        exec env -u KASANE_DEV_SESSION_DIR python3 "$repo_root/tools/cargo_domain.py" editor test --locked --features kasane-editor/test-support --lib --test integration "$@"
        ;;
    acceptance)
        exec env -u KASANE_DEV_SESSION_DIR python3 "$repo_root/tools/cargo_domain.py" editor test --locked --features kasane-editor/test-support \
            --test acceptance --test native_render --test headless_screenshot \
            --test animation_acceptance --test timeline_acceptance "$@"
        ;;
    tools)
        exec env PYTHONPATH="$repo_root/tools${PYTHONPATH:+:$PYTHONPATH}" python3 -m unittest test_package_source test_cargo_domain test_dev test_workspace_entrypoints test_property_chain_benchmark test_test_runner test_animation_trace_report test_frame_trace_report test_properties_trace_report "$@"
        ;;
    full)
        python3 "$repo_root/tools/cargo_domain.py" editor fmt -- --check
        env PYTHONPATH="$repo_root/tools${PYTHONPATH:+:$PYTHONPATH}" python3 -m unittest test_package_source test_cargo_domain test_dev test_workspace_entrypoints test_property_chain_benchmark test_test_runner test_animation_trace_report test_frame_trace_report test_properties_trace_report
        exec env -u KASANE_DEV_SESSION_DIR python3 "$repo_root/tools/cargo_domain.py" editor test --locked --features kasane-editor/test-support "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
