#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

usage() {
    cat <<'EOF'
Usage: ./tools/test-sdk.sh [fast|runtime|mesh|regression|stress|python2|python2-connected|full] [arguments...]

  fast        SDK unit and ordinary CPU integration tests; optional Cargo filter
  runtime     CPU Runtime ownership, transactions, jobs and event contracts
  mesh        alpha mesh daily cases, including vertex-budget boundaries
  regression  SDK libraries/integration, excluding the random stress remainder
  stress      complete alpha mesh suite, including the entire fixed-seed matrix
  python2     independent Runtime-backed Python package; Local/Connected and static types
  python2-connected  installed pure new client; subprocess discovery/start/session acceptance
  full        formatting and complete SDK Rust tests (default)

Examples:
  ./tools/test-sdk.sh fast physics_editing::
  ./tools/test-sdk.sh mesh
  ./tools/test-sdk.sh stress
  ./tools/test-sdk.sh python2

Details: tools/README.md#test-coverage
EOF
}

daily_rust_tests() {
    # Append the selector to an existing libtest argument list, or introduce
    # one. Cargo filters and flags such as --no-run keep their usual meaning.
    for argument in "$@"; do
        if [ "$argument" = "--" ]; then
            exec "$@" --skip alpha_mesh_stress::
        fi
    done
    exec "$@" -- --skip alpha_mesh_stress::
}

mode=${1:-full}
if [ "$#" -gt 0 ]; then shift; fi
case "$mode" in
    fast)
        exec cargo test --locked -p zipzi-sdk --lib --test integration "$@"
        ;;
    runtime)
        exec cargo test --locked -p zipzi-runtime "$@"
        ;;
    regression)
        daily_rust_tests python3 tools/workspace/cargo_domain.py sdk test --locked --lib --tests "$@"
        ;;
    mesh)
        daily_rust_tests cargo test --locked -p zipzi-sdk --test alpha_mesh "$@"
        ;;
    stress)
        exec cargo test --locked -p zipzi-sdk --test alpha_mesh "$@"
        ;;
    python2)
        exec uv run --locked python crates/zipzi-python2/tools/test.py "$@"
        ;;
    python2-connected)
        exec uv run --locked python crates/zipzi-python2/tools/connected.py "$@"
        ;;
    full)
        python3 tools/workspace/cargo_domain.py sdk fmt -- --check
        exec python3 tools/workspace/cargo_domain.py sdk test --locked "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
