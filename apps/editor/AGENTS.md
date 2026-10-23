Read ./README.md

This project has not been officially released. Breaking refactors are allowed, and migrations do not need to be considered.

## Test wisely

Use full suites cautiously: `./scripts/test.sh`, `cargo test --locked --workspace --features kasane-editor/test-support`, and the multi-package suite in `../kasane-2d` are expensive. Prefer the affected module/test filter during iteration; reserve full runs for changes that need broad regression coverage.