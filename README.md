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

The playground opens the Compact sample by default, using the editor's shared
styles and number input. It includes Chinese/English labels, button states,
search, property rows, and a panel-spacing prototype; use the theme button to
compare dark and light appearances and the UI scale button for 100%, 110%, and 125%.
The title bar combines a project dropdown (including New/Open and Close), a centered
workspace switcher, and a workspace menu (the top-right ellipsis) with UI scale.
The project dropdown remains available when no project is open.
Right-click a panel header (including its tab and unused space) for panel actions
or to add panels. Drag the tab or header background to move a panel. Groups with
multiple tabs offer a selection-only dropdown. Canvas tools live in the canvas overlay;
zoom controls are aligned to the right of its panel header.
Context and dropdown menus share Kit's Small density, including submenus and
multiline project entries. Menus and Selects use 12px text. Selects use XSmall
dropdown styling with 22px triggers; compact menu rows are 20px. Both follow
the editor's UI scale.

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

Run UI tests or capture screenshots:

```sh
cargo test --locked -p kasane-editor --lib headless_tests
# Focused work-count regressions for canvas redraws and property-list virtualization:
cargo test --locked -p kasane-editor --lib headless_tests::ui_performance
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Focused Timeline views and candidate/cancel pixel verification:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=timeline cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Title bar project switching and empty states:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=titlebar cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Compact layout, UI scale, and opaque Metal corner pixel checks:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=compact cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Compact menus, submenus and plain/searchable Selects at every UI scale:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=compact-popups cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
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
./scripts/build-app.sh playground
```

App bundles are saved to `target/`.

## License

Kasane Editor is licensed under the [GNU General Public License, version 3 only](LICENSE).
Third-party code in `vendor/` retains its respective licenses.
