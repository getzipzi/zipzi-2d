#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -n "${FUZZ_CC:-}" ]; then
  COMPILER=$FUZZ_CC
elif [ -x /opt/homebrew/opt/llvm@22/bin/clang ]; then
  COMPILER=/opt/homebrew/opt/llvm@22/bin/clang
elif command -v clang-18 >/dev/null 2>&1; then
  COMPILER=clang-18
else
  COMPILER=clang
fi
MODE=${1:-arena}
RUNS=${2:-1000}
case "$MODE" in
  arena) MALLOC=OFF ;;
  malloc) MALLOC=ON ;;
  *) echo "Usage: $0 [arena|malloc] [run-count]" >&2; exit 2 ;;
esac
case "$RUNS" in
  *[!0-9]*|'') echo "run-count must be a nonnegative integer" >&2; exit 2 ;;
esac

COMPILER_TAG=$("$COMPILER" --version | sed -n '1s/.* version \([0-9][0-9]*\).*/llvm\1/p')
BUILD="$ROOT/target/fuzz/build-$MODE-${COMPILER_TAG:-clang}"
CORPUS="$ROOT/target/fuzz/corpus-$MODE"
mkdir -p "$CORPUS"
cmake -S "$ROOT/modules/purism-core" -B "$BUILD" \
  -DCMAKE_C_COMPILER="$COMPILER" -DCMAKE_BUILD_TYPE=Debug -DBUILD_SHARED_LIBS=OFF \
  -DPURISM_CORE_BUILD_FUZZER=ON -DPURISM_CORE_FUZZ_DEBUG_MALLOC="$MALLOC"
cmake --build "$BUILD" --target purism_fuzzer --parallel
"$BUILD/purism_fuzzer" "$CORPUS" "$ROOT/modules/purism-core/fuzz/corpus" \
  "-runs=$RUNS" "-artifact_prefix=$BUILD/"
