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
```

Choose **Color Theme** from the top-right More menu or the **View** menu to switch
between Light, Dark, VS Code Light Modern, and VS Code Dark Modern. The theme and
UI scale are saved in your appearance preferences.

## Test

The full suite requires macOS and Metal. The test script checks formatting,
Python tooling, Rust behavior, and native acceptance. Native acceptance uses the
production application library with the `test-support` feature; headless UI unit
tests live in the library target.

```sh
./scripts/test.sh
```

Licensed model acceptance discovers optional assets under `models/local/` and
reports a skip when none are available. Explicit model paths must exist. Set
`KASANE_ANIMATION_REQUIRE_ASSET=1` or `KASANE_TIMELINE_REQUIRE_ASSET=1` to require
local assets, including in benchmark runs.

The opt-in [Mao format round-trip audit](docs/plans/mao-transfer-roundtrip-tests.md)
repeats Kasane, Cubism and runtime transfers and reports document/behavior loss:

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration transfer_roundtrip::mao_repeated -- --ignored --nocapture
```

Run UI tests or capture screenshots:

```sh
cargo test --locked -p kasane-editor --lib headless_tests
# Focused work-count regressions for canvas redraws and property-list virtualization:
cargo test --locked -p kasane-editor --lib headless_tests::ui_performance
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Two simultaneous Modeling/Animation viewports, independent pixels and render scheduling:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=viewports cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Focused Timeline views and candidate/cancel pixel verification:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=timeline cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Title bar project switching and empty states:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=titlebar cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Import/export settings in English and Chinese:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=transfer cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Compact layout, UI scale, and opaque Metal corner pixel checks:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=compact cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Compact menus, submenus and plain/searchable Selects at every UI scale:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=compact-popups cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Part identity and Parameter validation in English and Chinese:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=properties cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Imported motion edge cases, editing and canvas navigation acceptance (local Mao asset):
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --features test-support --test timeline_acceptance
# Release input/draw, candidate latency, playback, rapid-seek, canvas-pan and dock-resize measurements:
./scripts/timeline-benchmark.sh target/timeline-benchmark-run 600
# Timeline channel-number scrubbing, CPU/GPU stages and authoring comparison:
./scripts/timeline-number-benchmark.sh target/timeline-number-benchmark-run 96
# Continuous Animation workspace drags in a native window (no per-input frame wait):
KASANE_BENCH_ANIMATION=1 KASANE_BENCH_ANIMATION_DRAG=canvas ./scripts/window-benchmark.sh MODEL target/animation-canvas-drag 2400 240
KASANE_BENCH_ANIMATION=1 KASANE_BENCH_ANIMATION_DRAG=number ./scripts/window-benchmark.sh MODEL target/animation-number-drag 2400 240
```

## References

- [UI performance](docs/references/ui-performance.md): optional guidance on redraws,
  virtualization, rendering costs, and choosing regression checks. Agents and
  contributors can decide whether to consult it based on the task.

## Build

```sh
./scripts/build-app.sh
```

App bundles are saved to `target/`.

## License

Kasane Editor is licensed under the [GNU General Public License, version 3 only](LICENSE).
Third-party code in `vendor/` retains its respective licenses.
