# Kasane Editor

A native macOS editor for Kasane 2D models, built with Rust, GPUI Kit, the Kasane SDK, and Metal.

## Setup

- macOS with a Metal-capable GPU.
- Rust stable and Xcode Command Line Tools (`xcode-select --install`).
- A compatible [kasane-2d](https://github.com/pakholeung37/kasane-2d) checkout at `../kasane-2d`, including `modules/kasane-render-metal` and the required SDK/render changes. See [dependency details](DEPENDENCIES.md).

## Run

From the repository root:

```sh
cargo run --locked -p kasane-editor
```

To explore the UI components:

```sh
cargo run --locked -p component-playground
```

## Workspaces

Use the Modeling / Animation switch beside the document tabs, or the View menu.
Both workspaces share open documents and history, and keep separate layouts and
per-document zoom/pan. Modeling keeps the existing editing layout and parameter
preview. The last workspace and each layout are restored on startup; Reset Layout
only resets the current workspace.

Animation currently provides the workspace shell, navigation and a disabled
playback bar. Motion/expression playback is the next implementation phase.
Space is reserved for Play / Pause and no longer temporarily activates Hand.
Use middle-button drag, the Hand tool, or trackpad gestures to navigate.

## Develop

Install `watchexec`, then rebuild and restart automatically when source files change:

```sh
brew install watchexec
./scripts/dev.sh
./scripts/dev.sh playground  # Watch the component playground
```

## Tests

Run all tests and check formatting (the full suite requires macOS and Metal):

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
```

Test locations:

- `crates/editor-core/tests/`: document, command, and editor logic tests.
- `crates/editor/headless_tests/`: UI interaction tests using GPUI's headless window.
- `crates/editor/tests/`: native Metal, mesh overlay, and screenshot tests.

Run a specific test module or target:

```sh
cargo test --locked -p editor-core --test commands
cargo test --locked -p kasane-editor --bin kasane-editor headless_tests::canvas
cargo test --locked -p kasane-editor --test native_metal
```

Add UI tests with `#[gpui_kit::test]` in `crates/editor/headless_tests/`; register new modules in its `mod.rs` and reuse the shared `setup` harness.

## Headless screenshots

Capture the real editor UI without opening a native window. Requires macOS and a Metal-capable GPU.

```sh
unset KASANE_DEV_SESSION_DIR
cargo test --locked -p kasane-editor --test headless_screenshot
```

## Build apps

```sh
./scripts/build-app.sh
./scripts/build-app.sh playground
```

Release bundles are saved to `target/Kasane Editor.app` and `target/Kasane Component Playground.app`.
