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

Animation previews one existing motion at a time. Select a registered group
entry or an ungrouped clip in Motions, then use Space, the playback button or
the View menu to play / pause. Back to Start pauses and returns to zero.
Motion loops follow the resource; the transport shows the rendered time and
cycle. Preview Info shows the displayed frame's time and opacity, motion duration,
FPS, effective fades, track/event counts, expression blends, and Physics / Pose
availability. Resource diagnostics are available before playback: missing files,
unresolved targets, unsupported Model channels, PartOpacity without Pose, and Sound
registrations whose audio is not played. Runtime diagnostics use the same list.
Select an expression in Expressions to preview it alone or with the motion.
Switches use the resource's fades and Add / Multiply / Overwrite rules; Preview
Info distinguishes the selected expression from those active in the rendered frame.
Drag the ruler to pause and locate a motion. Non-looping seeks and Back to Start
replay this run's expression switches; selecting another expression in the past
replaces future switches. Loop scrubbing starts a new first-cycle preview with
the selected expression activated at zero. Pausing / resuming a loop keeps its
continuous physics clock.

Clear Expression immediately rebuilds the current time without any expression
switches and preserves play / pause. Reset Preview clears both resource selections
and returns to a paused baseline at zero. Expressions without a motion use a
continuous preview clock with the ruler disabled.
Open Preview Parameters from the Window menu or Preview Info to inspect parameter
values from the displayed frame. This optional panel is read only and is restored
with the Animation layout. Reset Layout returns to Motions, Expressions and Preview
Info. Transport and resource tooltips explain seeking, fades, clear and reset;
feedback is available in English and Simplified Chinese.
Motion / Expression / Physics / Pose run in the detached runtime. Playback does not change modeling
parameters, editor visibility, history or the project's modified state.
Leaving the workspace, changing documents, losing window activation, opening a
modal or saving pauses playback. Animation fits the model on first entry and
keeps its own zoom / pan afterward. Seek runs on the background evaluator with
latest-target coalescing and cancellation; old results cannot overwrite a newer target.

Space no longer temporarily activates Hand.
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
