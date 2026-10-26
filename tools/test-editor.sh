#!/bin/sh
set -eu
tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")/." && pwd)
. "$tools_dir/workspace/workspace.sh"

usage() {
    cat <<'EOF'
Usage: ./tools/test-editor.sh [fast|ui|regression|acceptance|tools|full] [test arguments...]

  fast        zipzi-editor-core unit and synthetic integration tests
  ui          Editor libraries/headless UI tests, with the full feature graph
  regression  Editor libraries and synthetic integration tests
  acceptance  licensed model, native rendering and screenshot tests
  tools       Python tooling tests
  full        complete checks, including doctests/examples (default)

Examples:
  ./tools/test-editor.sh fast timeline::
  ./tools/test-editor.sh ui headless_tests::ui_performance
  ./tools/test-editor.sh regression --no-run

Details: tools/README.md#test-coverage
Editor UI/native gates require macOS and Metal.
EOF
}

prepare_runtime_client() {
    if [ -z "${ZIPZI_CLIENT_PYTHON:-}" ]; then
        ZIPZI_CLIENT_PYTHON=$(python3 "$repo_root/crates/zipzi-python2/tools/connected.py" --install-only)
        export ZIPZI_CLIENT_PYTHON
    fi
}

mode=${1:-full}
if [ "$#" -gt 0 ]; then shift; fi
case "$mode" in
    fast)
        exec env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --lib --test integration "$@"
        ;;
    ui)
        exec env -u ZIPZI_DEV_SESSION_DIR python3 "$repo_root/tools/workspace/cargo_domain.py" editor test --locked --features zipzi-editor/test-support --lib "$@"
        ;;
    regression)
        exec env -u ZIPZI_DEV_SESSION_DIR python3 "$repo_root/tools/workspace/cargo_domain.py" editor test --locked --features zipzi-editor/test-support --lib --test integration "$@"
        ;;
    acceptance)
        prepare_runtime_client
        exec env -u ZIPZI_DEV_SESSION_DIR python3 "$repo_root/tools/workspace/cargo_domain.py" editor test --locked --features zipzi-editor/test-support \
            --test acceptance --test native_render --test headless_screenshot \
            --test animation_acceptance --test timeline_acceptance --test runtime_interaction_acceptance --test runtime_ipc_acceptance --test runtime_handoff_acceptance "$@"
        ;;
    tools)
        exec python3 "$repo_root/tools/test_tools.py" "$@"
        ;;
    full)
        python3 "$repo_root/tools/workspace/cargo_domain.py" editor fmt -- --check
        python3 "$repo_root/tools/test_tools.py"
        prepare_runtime_client
        exec env -u ZIPZI_DEV_SESSION_DIR python3 "$repo_root/tools/workspace/cargo_domain.py" editor test --locked --features zipzi-editor/test-support "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
