# kasane-editor

The native macOS application for Kasane 2D, built with GPUI Kit and Metal.
This crate owns application startup, window/workspace presentation, panels,
input routing and native UI integration. It consumes
[`editor-core`](../editor-core/README.md) for editing state and
[`editor-render`](../editor-render/README.md) for scene rendering.

## Develop and test

Requires macOS, a Metal-capable GPU, Rust 1.95+ and Xcode Command Line Tools
(`xcode-select --install`). SDK sources are in the same workspace; no sibling
checkout is needed. Dependency pins and patch details are in
[`vendor/README.md`](../../vendor/README.md#dependencies).
Run from the repository root:

```sh
cargo run --locked -p kasane-editor
brew install watchexec
./tools/dev-editor.sh
./tools/test-editor.sh ui headless_tests::ui_performance
./tools/test-editor.sh acceptance
./tools/build-editor-app.sh
```

The UI gate selects all three Editor crates to preserve dependency feature
unification. `test-support` enables production-library native acceptance targets:
`native_render`, `headless_screenshot`, `animation_acceptance` and
`timeline_acceptance`. Screenshots and native execution require Metal; some
animation/transfer cases additionally need separately obtained local model assets.
See [test coverage and CI](../../tools/README.md#test-coverage) for the full mode
matrix and [benchmarks](../../tools/README.md#benchmarks-and-research-tools) for
native measurement commands.

## Development process

`dev-editor.sh` uses the repository root `target/dev/` for Cargo builds (both watch and run modes).
PID files, locks, logs and app bundles also stay under the root `target/`, separately
for each checkout. The watcher observes Editor sources, SDK crates and the root
Cargo manifest/lockfile. Scripts can be invoked by absolute path from any directory.
Explicit relative model/output arguments and `CARGO_TARGET_DIR` are relative to the
caller; defaults use the repository root. `CARGO_BUILD_TARGET` is supported through
Cargo artifact messages. Dev builds intentionally override `CARGO_TARGET_DIR`.

## Local model acceptance

Licensed model acceptance discovers optional assets under the repository root `models/local/` and
reports a skip when none are available. Explicit model paths must exist. Set
`KASANE_ANIMATION_REQUIRE_ASSET=1` or `KASANE_TIMELINE_REQUIRE_ASSET=1` to require
local assets, including in benchmark runs.

The opt-in [Mao format round-trip audit](../../docs/archive/plans/mao-transfer-roundtrip-tests.md)
repeats Kasane, Cubism and runtime transfers and reports document/behavior loss:

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test acceptance transfer_roundtrip::mao_repeated -- --ignored --nocapture
```

## Native screenshots

Run the production application library with `test-support`; set
`KASANE_SCREENSHOT_SCENE` to select a scene:

```sh
env -u KASANE_DEV_SESSION_DIR KASANE_SCREENSHOT_SCENE=timeline cargo test --locked -p kasane-editor --features test-support --test headless_screenshot
```

| Scene | Coverage |
| --- | --- |
| unset | Default screenshot acceptance |
| `viewports` | Simultaneous Modeling/Animation viewports and independent scheduling |
| `timeline` | Timeline views and candidate/cancel pixels |
| `titlebar` | Project switching and empty states |
| `transfer` | Import/export settings in English and Chinese |
| `compact` | Compact layout, UI scaling and opaque Metal corners |
| `compact-popups` | Menus, submenus and ordinary/searchable Selects at every UI scale |
| `properties` | Mesh properties, Part identity and Parameter validation in both locales |
| `physics` | Physics group list and compact Properties inspector in both locales |

Imported motion edge cases, editing and canvas navigation are covered by
`timeline_acceptance` using the local Mao asset:

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --features test-support --test timeline_acceptance
```

## App bundles

`./tools/build-editor-app.sh` saves the app under the root `target/`. Binary
locations come from Cargo artifacts; bundles include embedded UI assets and
required license notices. See [distribution commands](../../tools/README.md#local-distribution-artifacts)
for debug/release bundles and artifact checks.

## Source map

- `src/main.rs`, `src/lib.rs`: application binary and reusable application library.
- `src/workspace/`, `src/panels/`, `src/properties/`: workspace layout and editing UI.
- `src/viewports/`, `src/preview.rs`, `src/canvas_overlay.rs`: canvas presentation.
- `src/input_routing/`, `src/commands.rs`, `src/keybindings.rs`: input and command routing.
- `src/worker.rs`, `src/worker_queue.rs`: background work coordination.
- `assets/app-icons/`, `assets/icons/`, `assets/locales/`: crate-owned application assets.
- `tests/`: native rendering, screenshot and animation acceptance.

Keep UI-independent editing rules in `editor-core` and backend rendering in
`editor-render` or the SDK render crates. Optional `debug-tools` enables GPUI FPS
tools; `bench-instrumentation` enables editing-chain instrumentation.

Licensed **GPL-3.0-only**. This crate is not published to crates.io; distribute it
with its assets, workspace patches and [required notices](../../LICENSING.md).
