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

## Canvas navigation

- Trackpad: two-finger scrolling pans in both directions, including momentum; pinch zooms around the pointer.
- Mouse: the wheel zooms around the pointer; middle-button dragging pans.
- Ctrl/Cmd + trackpad scrolling also zooms. H or held Space enables left-button panning.

## Palette sorting

- Drag tree names above or below another sibling to reorder them. Part and deformer trees keep independent display orders; sibling sorting preserves parent relationships and draw order. In the Part tree, drop in the middle of a Part to append to it, at another Part’s child row edge to move beside that child, or on the bottom drop area to remove the Part assignment. Moving a Part carries its children. Deformer parent changes are not supported by dragging. Tree sorting is disabled while search or type filters are active.
- Moving between Parts preserves the existing drawing groups and animated draw-order values. Projects with implicit drawing groups save their current grouping in the same undo step before changing ownership. Part enablement and Offscreen membership still follow organization.
- Drag a parameter's name above or below another parameter to place it there and adopt its group. Drop on a group header to append to that group, including an empty group or Ungrouped. Hovering a collapsed group or destination Part opens it after a short delay.
- Drag parameter group headers above or below sibling groups to move the group with its contents. Direct parameters remain before nested subgroups.
- Lists scroll when a drag approaches their edges. Escape, window deactivation, or switching documents cancels the drag. Each completed move is one undo step and survives saving and reopening the project.

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

Outputs `editor.png`, `editor-selected.png`, and `editor-hidden.png` to `target/headless-screenshots/`. Prefix the test command with `KASANE_SCREENSHOT_DIR=/tmp/kasane-screenshots` to change the output directory. This target also runs with the full test suite.

## Build apps

```sh
./scripts/build-app.sh
./scripts/build-app.sh playground
```

Release bundles are saved to `target/Kasane Editor.app` and `target/Kasane Component Playground.app`.
