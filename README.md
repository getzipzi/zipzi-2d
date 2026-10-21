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

`dev.sh` uses `target/dev/` for Cargo builds (both watch and run modes)

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
# Mesh properties, Part identity and Parameter validation in English and Chinese:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=properties cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Physics group list and compact Properties inspector in English and Chinese:
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=physics cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
# Imported motion edge cases, editing and canvas navigation acceptance (local Mao asset):
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --features test-support --test timeline_acceptance
# Release input/draw, candidate latency, playback, rapid-seek, canvas-pan and dock-resize measurements:
./scripts/timeline-benchmark.sh target/timeline-benchmark-run 600
# Timeline channel-number scrubbing, CPU/GPU stages and authoring comparison:
./scripts/timeline-number-benchmark.sh target/timeline-number-benchmark-run 96
# Mesh properties and complete RGB/HSV color picker in a native window:
./scripts/properties-benchmark.sh target/properties-benchmark-run 300 120
# Paired Release editing-chain totals, nested stage attribution and work counts:
python3 scripts/property-chain-benchmark.py models/local/mao/runtime/mao_pro.model3.json target/property-chain-run --reader-policy unique
# Continuous Animation workspace drags in a native window (no per-input frame wait):
KASANE_BENCH_ANIMATION=1 KASANE_BENCH_ANIMATION_DRAG=canvas ./scripts/window-benchmark.sh MODEL target/animation-canvas-drag 2400 240
KASANE_BENCH_ANIMATION=1 KASANE_BENCH_ANIMATION_DRAG=number ./scripts/window-benchmark.sh MODEL target/animation-number-drag 2400 240
```

## References

- [Property editing chain benchmark](docs/references/property-chain-benchmark.md):
  measurement boundaries, paired probes, snapshot ownership controls and reproduction.
  [Mao baseline and bottlenecks](docs/benchmarks/2026-10-21-property-chain.md)
  record the initial measurements before refactoring.
  [Refactor comparison](docs/benchmarks/2026-10-21-property-chain-refactor.md)
  records the same workloads run against saved before/after binaries, including
  the follow-up changes to property policies, index snapshots and Motion storage.
- [Physics field patches](docs/benchmarks/2026-10-20-physics-patches.md): shared SDK storage,
  generic property editing, and release write-cost measurements.
- [UI performance](docs/references/ui-performance.md): optional guidance on redraws,
  virtualization, rendering costs, and choosing regression checks. Agents and
  contributors can decide whether to consult it based on the task.
- [Mesh properties benchmark](docs/references/mesh-properties-benchmark.md): native
  control checklist, CPU/GPU measurements, and result interpretation.
  [Mao results](docs/benchmarks/2026-10-19-mesh-properties.md) include the measured
  CPU budget overruns and trace coverage audit.
  [CPU fixes and small-sample verification](docs/benchmarks/2026-10-19-mesh-properties-cpu-fix.md)
  record the subsequent tooltip and synchronous commit improvements.

## Build

```sh
./scripts/build-app.sh
```

App bundles are saved to `target/`.

## License

Kasane Editor is licensed under the [GNU General Public License, version 3 only](LICENSE).
Third-party code in `vendor/` retains its respective licenses.
