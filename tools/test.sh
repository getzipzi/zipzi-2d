#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

usage() {
    cat <<'EOF'
Usage: ./tools/test.sh [fast|regression|python|wheel|full] [arguments...]

  fast        SDK unit and ordinary CPU integration tests; optional Cargo filter
  regression  workspace libraries and integration tests, including alpha mesh
  python      build/install current Debug extension, then Python SDK tests
  wheel       build/install a Release wheel and run SDK acceptance
  full        formatting, complete Rust suite and CPU wheel acceptance (default)

Examples:
  ./tools/test.sh fast physics_editing::
  ./tools/test.sh python
  ./tools/test.sh python --observe
  ./tools/test.sh python -- -k history
  ./tools/test.sh wheel --observe
EOF
}

mode=${1:-full}
if [ "$#" -gt 0 ]; then shift; fi
case "$mode" in
    fast)
        exec cargo test --locked -p kasane-sdk --lib --test integration "$@"
        ;;
    regression)
        exec cargo test --locked --workspace --lib --tests "$@"
        ;;
    python)
        exec uv run --locked python tools/test_python.py "$@"
        ;;
    wheel)
        exec uv run --locked python tools/test_python.py --wheel "$@"
        ;;
    full)
        cargo fmt --all -- --check
        cargo test --locked --workspace
        exec uv run --locked python tools/test_python.py --wheel "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
