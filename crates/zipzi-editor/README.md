# zipzi-editor

The Zipzi 2D desktop editor provides modeling and animation workspaces and hosts
Runtime sessions for the [Python SDK](../zipzi-python2/README.md). It is built
with GPUI Kit and uses [`zipzi-editor-core`](../zipzi-editor-core/README.md) for
editing state and [`zipzi-editor-render`](../zipzi-editor-render/README.md) for
rendering.

## Develop and test

Run commands from the repository root. The workspace requires Rust 1.95 or
newer, uv 0.10.7 or newer, and CPython 3.14. See the
[root README](../../README.md#development) for workspace setup.

- **macOS:** Xcode Command Line Tools and a Metal-capable GPU are required.
  Model previews use Metal by default; wgpu is available as an optional backend.
- **Windows:** The development editor requires MSVC C++ Build Tools, the Windows
  SDK and a usable GPU. It uses wgpu and shared GPU textures for previews.
  Desktop validation is still in progress.

Start the editor with Cargo on either platform:

```sh
cargo run --locked -p zipzi-editor
```

For a development build and process management, use the cross-platform launcher:

```sh
uv run --locked python tools/dev.py editor run
uv run --locked python tools/dev.py editor reload
uv run --locked python tools/dev.py editor stop
```

`editor watch` rebuilds on source changes; `./tools/dev-editor.sh` is also
available as a shell wrapper. The launcher keeps build and process files under
the checkout's `target/` directory by default. On Windows, reload and stop
request a safe exit and preserve the running editor if it has unsaved documents
or connected clients.

Choose a focused test for the change:

```sh
# Editor core logic
./tools/test-editor.sh fast adaptive_mesh::

# Editor UI logic
./tools/test-editor.sh ui mesh_generation::

# Python client connected to an installed editor host
./tools/test-sdk.sh python2-connected --editor-host /absolute/path/to/zipzi-editor
```

On Windows, use `uv run --locked python tools/dev.py test editor` or
`uv run --locked python tools/dev.py test sdk` with the same modes and filters. Editor shell gates clear
`ZIPZI_DEV_SESSION_DIR`; unset it explicitly for direct Editor Cargo tests so
they do not attach to a live development session. Native Metal acceptance and
screenshot tests require macOS and Metal; Windows UI logic tests are available.
See [test coverage and prerequisites](../../tools/README.md#test-coverage) and
`./tools/test-editor.sh --help` for the available gates. Full and acceptance
runs are more expensive than focused tests.

## Resident application host

Start the editor without a window using:

```sh
cargo run --locked -p zipzi-editor -- --no-window
```

The Python SDK can connect to the resident host and share its Runtime sessions,
documents and history. When connected clients, pending jobs or an explicit
`--no-window` launch keep the host resident, closing a window detaches its view;
reopening a window attaches the resident sessions. Closing a document tab also
detaches its view. Otherwise, closing the last window exits after any unsaved
documents are resolved. Use Quit to explicitly exit. See the
[Python SDK guide](../zipzi-python2/client/python/zipzi/docs/guide.md) for
connection and session usage.

## Local model acceptance

Some acceptance tests use separately obtained model assets under
[`models/local/`](../../models/README.md). Missing optional assets are reported
as skips. Set `ZIPZI_ANIMATION_REQUIRE_ASSET=1` or
`ZIPZI_TIMELINE_REQUIRE_ASSET=1` to require them.

The opt-in format round-trip test can be run on macOS with the local Mao asset:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test acceptance transfer_roundtrip::mao_repeated -- --ignored --nocapture
```

## Native screenshots

On macOS, run a screenshot scene against the production application library:

```sh
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=timeline cargo test --locked -p zipzi-editor --features test-support --test headless_screenshot
```

Omit `ZIPZI_SCREENSHOT_SCENE` for the default set. Available scenes are defined
in [`acceptance/screenshots.rs`](acceptance/screenshots.rs). Screenshots go to
`target/headless-screenshots/` by default; `ZIPZI_SCREENSHOT_DIR` overrides the
output directory.

## Mesh generation controls

The editor's adaptive mesh controls use source pixels. Initial distances and
scrub increments scale with the longer texture dimension. For a 2048 × 1024
texture, edge spacing starts at 500 px, interior spacing at 130 px, outer margin
at 2 px and edge width at 16 px. The defaults and scaling rules are defined in
[`mesh_generation_controls.rs`](src/ui/mesh_generation_controls.rs); see the
[adaptive mesh reference](../zipzi-editor-core/src/adaptive_mesh/README.md) for
the generator and its limits.

## Distribution

`./tools/build-editor-app.sh` builds a macOS app bundle. Windows portable
archives are built with `tools/packaging/package_windows.py`. See
[distribution commands and verification](../../tools/README.md#local-distribution-artifacts)
and the [release guide](../../docs/RELEASING.md) for artifacts and prerequisites.

## Source map

- `src/main.rs`, `src/lib.rs`: application entry point and library.
- `src/workspace/`, `src/panels/`, `src/properties/`: workspaces and editing UI.
- `src/viewports/`, `src/preview.rs`, `src/presentation.rs`: canvas and previews.
- `src/input_routing/`, `src/commands.rs`, `src/keybindings.rs`: input and commands.
- `src/host.rs`, `src/ipc.rs`, `src/worker.rs`: resident host, IPC and background work.
- `assets/`: icons and localization resources.
- `headless_tests/`, `tests/`, `acceptance/`: UI logic and native acceptance tests.

The `debug-tools` feature enables GPUI FPS tools; `bench-instrumentation` enables
editing-chain instrumentation. See the [UI performance reference](../../docs/references/ui-performance.md)
and [benchmarks](../../tools/README.md#benchmarks-and-research-tools) when
working on performance.

The editor is licensed **GPL-3.0-only** and is not published to crates.io.
Distribution requires its assets, workspace patches and
[third-party notices](../../LICENSING.md).
