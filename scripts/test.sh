#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

usage() {
    cat <<'EOF'
Usage: ./scripts/test.sh [fast|ui|regression|acceptance|tools|full] [test arguments...]

  fast        editor-core unit and synthetic integration tests
  ui          workspace libraries/headless UI tests, with the full feature graph
  regression  workspace libraries and synthetic integration tests
  acceptance  licensed model, native rendering and screenshot tests
  tools       Python tooling tests
  full        original complete checks, including doctests/examples (default)

Examples:
  ./scripts/test.sh fast timeline::
  ./scripts/test.sh ui headless_tests::ui_performance
  ./scripts/test.sh regression --no-run
EOF
}

mode=${1:-full}
if [ "$#" -gt 0 ]; then shift; fi
case "$mode" in
    fast)
        exec env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --lib --test integration "$@"
        ;;
    ui)
        exec env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace --features kasane-editor/test-support --lib "$@"
        ;;
    regression)
        exec env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace --features kasane-editor/test-support --lib --test integration "$@"
        ;;
    acceptance)
        exec env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace --features kasane-editor/test-support \
            --test acceptance --test native_render --test headless_screenshot \
            --test animation_acceptance --test timeline_acceptance "$@"
        ;;
    tools)
        exec python3 -m unittest discover -s scripts -p 'test_*.py' "$@"
        ;;
    full)
        cargo fmt --all -- --check
        python3 -m unittest discover -s scripts -p 'test_*.py'
        exec env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace --features kasane-editor/test-support "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
