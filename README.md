# Kasane Editor

A native macOS editor for Kasane 2D models, built with Rust, GPUI Kit, and Metal.

## Setup

- macOS with a Metal-capable GPU.
- Rust stable and Xcode Command Line Tools (`xcode-select --install`).
- A compatible `kasane-2d` checkout at `../kasane-2d`. See [dependencies](DEPENDENCIES.md).

## Develop

Run the editor:

```sh
cargo run --locked -p kasane-editor
```

Watch source changes and rebuild automatically:

```sh
brew install watchexec
./scripts/dev.sh
./scripts/dev.sh playground  # Component playground
```

## Test

The full suite requires macOS and Metal.

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
```

Run UI tests or capture screenshots:

```sh
cargo test --locked -p kasane-editor --bin kasane-editor headless_tests
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --test headless_screenshot
# Focused Timeline views and candidate/cancel pixel verification:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=timeline cargo test --locked -p kasane-editor --test headless_screenshot
```

## Build

```sh
./scripts/build-app.sh
./scripts/build-app.sh playground
```

App bundles are saved to `target/`.
