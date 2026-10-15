#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
python3 -m unittest discover -s scripts -p 'test_*.py'
env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace --features kasane-editor/test-support
