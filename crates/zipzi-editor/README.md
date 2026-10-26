# zipzi-editor

The native macOS application for Zipzi 2D, built with GPUI Kit and Metal.
This crate owns application startup, window/workspace presentation, panels,
input routing and native UI integration. It consumes
[`zipzi-editor-core`](../zipzi-editor-core/README.md) for editing state and
[`zipzi-editor-render`](../zipzi-editor-render/README.md) for scene rendering.

## Develop and test

Requires macOS, a Metal-capable GPU, Rust 1.95+ and Xcode Command Line Tools
(`xcode-select --install`). SDK sources are in the same workspace; no sibling
checkout is needed. Dependency pins and patch details are in
[`vendor/README.md`](../../vendor/README.md#dependencies).
Run from the repository root:

```sh
cargo run --locked -p zipzi-editor
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
native measurement commands. Consult the
[UI performance reference](../../docs/references/ui-performance.md) when changing
redraws, caching, large lists or frame scheduling.

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
`ZIPZI_ANIMATION_REQUIRE_ASSET=1` or `ZIPZI_TIMELINE_REQUIRE_ASSET=1` to require
local assets, including in benchmark runs.

The opt-in [Mao format round-trip audit](../../docs/archive/plans/mao-transfer-roundtrip-tests.md)
repeats Zipzi, Cubism and runtime transfers and reports document/behavior loss:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test acceptance transfer_roundtrip::mao_repeated -- --ignored --nocapture
```

## Native screenshots

Run the production application library with `test-support`; set
`ZIPZI_SCREENSHOT_SCENE` to select a scene:

```sh
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=timeline cargo test --locked -p zipzi-editor --features test-support --test headless_screenshot
```

| Scene | Coverage |
| --- | --- |
| unset | Default screenshot acceptance |
| `viewports` | Simultaneous Modeling/Animation viewports and independent scheduling |
| `timeline` | Timeline views and candidate/cancel pixels |
| `titlebar` | Project switching and empty states |
| `transfer` | Import/export settings in English and Chinese |
| `compact` | Compact layout, UI scaling and opaque Metal corners |
| `parameter-toolbar` | Keyform SVG buttons, standard sizing and alignment across themes and UI scales |
| `compact-popups` | Menus, submenus and ordinary/searchable Selects at every UI scale |
| `properties` | Mesh properties, Part identity and Parameter validation in both locales |
| `physics` | Physics group list and compact Properties inspector in both locales |

Imported motion edge cases, editing and canvas navigation are covered by
`timeline_acceptance` using the local Mao asset:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --features test-support --test timeline_acceptance
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

Keep UI-independent editing rules in `zipzi-editor-core` and backend rendering in
`zipzi-editor-render` or the SDK render crates. Optional `debug-tools` enables GPUI FPS
tools; `bench-instrumentation` enables editing-chain instrumentation.

Licensed **GPL-3.0-only**. This crate is not published to crates.io; distribute it
with its assets, workspace patches and [required notices](../../LICENSING.md).

## Resident application host

Run `cargo run --locked -p zipzi-editor -- --no-window` to start the application
without a window and keep it resident. Closing the last window in a normal desktop
launch exits when there are no connected SDK clients or pending background jobs.
Unsaved documents, including detached tabs, first prompt to save, discard or cancel;
cancelling or a failed save keeps the window open. Disconnected clients retained
only for reconnect grace do not prevent this exit.

With a connected SDK client, pending job or explicit `--no-window` launch, closing
a window detaches its view and retains Runtime sessions, unsaved content and history.
Closing a document tab also only detaches its view. Reopening the application window
attaches resident sessions. Use Quit to save/discard and explicitly exit.
Load/import/save completion returns to the application host even without windows;
errors remain available when a window is reopened.

The focused native host gate is `./tools/test-editor.sh ui -- headless_tests::runtime_host`.
The Release dual-viewport/background-client probe uses
`runtime_interaction_acceptance` with `ZIPZI_TIMELINE_MODEL`,
`ZIPZI_RUNTIME_INTERACTION_OUTPUT` and `ZIPZI_RUNTIME_INTERACTION_SAMPLES` (at
least 30). It checks input reaches both current Metal surfaces while another
session publishes Runtime jobs, and verifies visible history isolation.

### Local Python connection (R3)

The production Editor owns a private Unix socket and its endpoint lock for the
whole resident lifetime, including `--no-window`. Python cold startup transfers
an already locked owner descriptor; another desktop invocation forwards a window
reopen to the existing process after a strict handshake. Protocol work enters
Runtime on the application thread, and worker completion remains independent of
windows. Development launches set `ZIPZI_EDITOR_HOST` to their paired executable;
a reused resident host is reported without claiming ownership of its process.
See [new Python client connection usage](../zipzi-python2/README.md#desktop-host-and-resident-sessions)
and the [R0–R4 reconnection record](../../docs/plans/sdk-runtime/python2-r0-r4.md).

```sh
./tools/test-sdk.sh python2-connected
ZIPZI_CLIENT_PYTHON=$(python3 crates/zipzi-python2/tools/connected.py --install-only) \
  env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor \
  --features test-support --test runtime_ipc_acceptance
```

The native acceptance requires the isolated client installed by the first
command and a working Metal device; it fails when prerequisites are absent.
