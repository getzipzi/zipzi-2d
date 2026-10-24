Read ./README.md

This project has not been officially released. Breaking refactors are allowed, and migrations do not need to be considered.

## Test wisely

Prefer focused package/module tests. Full SDK and Editor suites are expensive; reserve them for broad regression coverage.

Start with the test selection table in [README.md](README.md#tests).
[tools/README.md](tools/README.md#test-coverage) defines modes, prerequisites and CI
coverage; each crate README describes its focused tests. Inspect `--help` on
`tools/test-sdk.sh`, `tools/test-editor.sh` and `tools/test_tools.py` to discover
arguments. Both domain shell scripts default to expensive full runs.

Tooling tests live in `tools/tests/`; shared test data lives in `fixtures/`.
Editor shell gates clear `ZIPZI_DEV_SESSION_DIR`; clear it explicitly for direct
Editor Cargo test commands.
