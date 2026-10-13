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

Animation uses its own panel set. Properties shows the selected motion and
expression definitions; Preview Info shows playback status, events and diagnostics.
The default layout places Motions above Expressions on the left, Preview Info
and Properties in a shared tab group on the upper right, and Preview Parameters
below them. Preview Info is the initially active tab. Add Physics or Pose from a
panel's `⋯` menu to inspect additional values from the displayed frame. Physics shows final parameter values, including motion
and expression effects. These inspectors are read-only and do not change Modeling
state. Existing saved layouts keep their arrangement; use Window → Reset Layout (also in the workspace `⋯` menu) to load the default
arrangement. Layout recovery remains available when all panels have been closed.
Click a motion row to select it; clicking the selected row preserves playback
progress and state. Double-click to play it from the start. Use the transport or
Space to pause and resume.
Switching expressions while paused immediately previews the latest choice at the
current time. Switching during playback preserves the authored fades.

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
cargo test --locked -p kasane-editor --test animation_acceptance
```

Add UI tests with `#[gpui_kit::test]` in `crates/editor/headless_tests/`; register new modules in its `mod.rs` and reuse the shared `setup` harness.

The animation acceptance target imports the local licensed Mao, Shirousagi and
heijiao model3 packages when available. It checks actual Metal images, seeking,
expression clearing, document switching, Modeling restoration and project reopen.
Missing local packages are explicitly reported as skipped; portable fixtures still
cover runtime semantics and masked / Offscreen opacity in the regular suite.

Profile release playback with an isolated native window, or collect headless
workflow, cancellation, seek-cache and memory data:

```sh
./scripts/animation-window-benchmark.sh models/local/mao/runtime/mao_pro.model3.json target/animation-window-run 600
./scripts/animation-benchmark.sh target/animation-headless-run 600
python3 scripts/test_animation_trace_report.py
```

Use a new output directory for each run. The native benchmark requires the window
to stay visible and active; it uses the application's display callbacks and compositor
presentation timestamps. Headless profiling uses real SDK / Metal workers with a
test clock and does not measure display presentation. Both emit JSONL traces and
correlated timing / queue summaries.

Animation samples follow the window frame cadence, including high-refresh displays;
the SDK retains the canonical 60 Hz simulation state and evaluates fractional
presentation times without accumulating partial physics steps. Actual throughput
still depends on evaluation, rendering and compositor timing. See the
[initial animation validation report](docs/archive/workspaces-animation-validation.md)
for model coverage and the original 60 Hz baseline.

Animation evaluation and Metal submission workers use macOS interactive QoS;
idle workers still sleep on their bounded mailboxes. Traces include thread CPU
versus elapsed time, UI/compositor timing, and long presentation intervals.
For a scheduling A/B run, prefix the native benchmark command with
`KASANE_TRACE_DEFAULT_WORKER_QOS=1`; this opt-out only applies while tracing.

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
