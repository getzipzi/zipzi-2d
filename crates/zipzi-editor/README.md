# zipzi-editor

The desktop application for Zipzi 2D, built with GPUI Kit. On macOS, model
previews use native Metal or the optional wgpu backend.
This crate owns application startup, window/workspace presentation, panels,
input routing and native UI integration. It consumes
[`zipzi-editor-core`](../zipzi-editor-core/README.md) for editing state and
[`zipzi-editor-render`](../zipzi-editor-render/README.md) for scene rendering.

Windows now has a P5 development prototype using the shared
[wgpu backend](../zipzi-render-wgpu/README.md), D3D12 shared textures and GPUI
D3D11 GPU composition. It requires Rust 1.95+, MSVC C++ Build Tools, Windows SDK and
a usable GPU. Settings use `%APPDATA%\Zipzi Editor`; Named Pipe clients share the
resident Editor Runtime, including `--no-window` and window reopening.

```powershell
cargo run --locked -p zipzi-editor
uv run --locked python tools/dev.py editor run
uv run --locked python tools/dev.py editor reload
uv run --locked python tools/dev.py editor stop
```

The developer executable and license/notices files are published under
`target/dev-editor`. GPUI assets and the app icon are embedded; GPUI Kit's pinned
`gpui-pre` dependency supplies the PerMonitorV2/asInvoker manifest. Do not embed
another manifest resource 1. This is a development portable build, not a signed
installer. Desktop acceptance and performance are tracked in the
[Windows plan](../../docs/plans/windows-development.md).

Windows development reload/stop asks the owned Editor to exit safely before
replacing the executable. Unsaved documents, active dialogs/gestures, pending
jobs or connected Python clients block restart and preserve the running process.
Save work and disconnect clients before retrying. Clean project tabs are restored
after reload. This handshake is enabled only by the development launcher.

Main and mesh-candidate previews publish immutable shared GPU textures. A worker
renders on the same adapter as GPUI, then copies on the GPU into a bounded shared
texture pool. D3D11 waits on the producer fence before sampling and signals a
consumer fence after drawing. There is no pixel readback, alpha conversion or
image-atlas upload in this path. Candidate rasters retain the 2048px longest-edge
limit. See [bridge ownership and limits](../zipzi-editor-render/README.md#wgpu-model-adapter).

Frame traces record `shared_texture_submitted` and `paint_shared_surface`;
these are CPU submission/paint boundaries, not GPU completion or screen presentation.
The Windows trace report accepts both these paints and historical `compose_image`
paints. Its existing metric names retain `image` for compatibility with old reports.

Focused Windows checks (run in a desktop session for the installed Editor gate):

```powershell
cargo check --locked -p zipzi-editor -p zipzi-editor-render --all-targets --features zipzi-editor/test-support,zipzi-editor/wgpu-backend
cargo test --locked -p zipzi-editor-render --features wgpu-backend --test windows_shared_texture -- --nocapture
uv run --locked python tools/dev.py test editor ui headless_tests::ui_performance
uv run --locked python tools/dev.py test sdk python2-connected --editor-host C:\absolute\path\to\zipzi-editor.exe
```

The installed Editor gate covers concurrent cold start, shared history/CPU
poses, window reuse, disconnected-client recovery and save/restart/open. It does
not assert screen pixels, IME behavior or end-to-end presentation latency.

## Develop and test

On macOS, requires a Metal-capable GPU, Rust 1.95+ and Xcode Command Line Tools
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

## wgpu model preview on macOS

`cargo run --locked -p zipzi-editor --features wgpu-backend` selects wgpu for
model preview and Runtime GPU observation. This uses the real wgpu pipeline on
macOS, with a GPU texture handoff into the existing GPUI Metal surface. Completion
is synchronized on the rendering worker before the texture is published.
Window rendering and interaction overlays remain Metal-specific.

For focused application composition coverage:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor \
  --features test-support,wgpu-backend --test native_render metal::
```

See the [wgpu backend](../zipzi-render-wgpu/README.md) for pixel parity and
real-model comparison commands.

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
| `parameter-pad` | Linked two-axis pad alongside a one-axis track, across themes and UI scales |
| `compact-popups` | Menus, submenus and ordinary/searchable Selects at every UI scale |
| `properties` | Mesh properties, Part identity and Parameter validation in both locales |
| `physics` | Physics group list and compact Properties inspector in both locales |
| `manual-mesh` | Mesh Edit secondary toolbar, isolation, outline deletion/extension, connected point creation, undo/redo and native save/reopen |

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
Set `ZIPZI_RUNTIME_INTERACTION_READS=1` to add continuous four-pose CPU sampling,
independent Preview sampling and 96×96 GPU observation on that Runtime. The
report retains per-input timings and verifies background execution overlaps the
input loop. Use the identical driver in both Release builds for a comparison.

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

R6 direct handoff has a separate synthetic native case using the same installed
client and production IPC/Editor/Metal chain:

```sh
ZIPZI_CLIENT_PYTHON=$(python3 crates/zipzi-python2/tools/connected.py --install-only) \
  env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor \
  --features test-support --test runtime_handoff_acceptance
```

It covers one Python batch of mesh/texture/Motion changes, human property text
input and Timeline dragging, Undo/Redo, a new Python process reading and extending
the refinement, the installed command-line example, and temporary image cleanup.
The same flow now finishes durable save/reopen, motion/runtime-package export,
PCM attachment verification and redisplay of the paused animation after IO.
See the [R5/R7 closeout record](../../docs/plans/sdk-runtime/python2-r7-closeout.md)
for normal app/DMG installation and real-model acceptance.
Committed content and resources survive each client exit. No licensed model is
required. [Scope and evidence](../../docs/plans/sdk-runtime/python2-r6-handoff.md).
