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

## Object deletion

In Modeling, **Delete Object** is shared by the Edit menu, the Part and Deformer
row menus, the viewport context menu, and the configurable `edit.delete-object`
shortcut (Delete and Backspace by default). Shortcuts use keyboard focus before
pointer hover and leave text editing and timeline deletion to their own controls.
In Mesh Edit, the same keys delete selected geometry instead of the mesh object.
Right-clicking a selected row uses the current selection; another row targets
only that object. Part tree deletion includes organization descendants, while
Deformer tree and viewport deletion include deformation descendants. Owned
bindings are removed with their targets. Every object deletion requires a review of
the frozen deletion list and explicit confirmation, with Cancel focused initially.
Completing or recalculating the review never deletes objects automatically.
A single-object confirmation names the object directly; larger deletions list the
objects and why they are included. Required choices show the reason before the
replacement controls. Reference updates, IDs, attached records and conversion
reports are behind **View details**. Choose whether to keep a
group's contents with explicit scope controls. Choosing replacements or changing
the scope updates the preview automatically; **Recheck impact** is available in
technical details. Replacement actions stay disabled until a target is selected.
Each deletion is one undo step.
Locked objects (including inherited locks and locked descendants) and references
from outside the deletion set block the operation. Menu labels stay short; clicking
an unavailable action explains the reason in the status bar without changing the
document. This feedback is shared by creation and other object context menus.
Changing the document or workspace expires an open deletion menu or review.

## Mesh Edit

Mesh Edit retains the complete model and its current camera. The selected mesh's
handles follow the evaluated pose, including ordinary parameters, BlendShapes,
parent deformers and Glue. Picking maps the displayed triangles back to UVs;
drags freeze the source pose, invert Glue and the parent deformation, and resample
all stored shapes through the resulting UVs. No authored keyform needs to be selected.

The image button switches to **Edit on source image** for an optional pixel-space
UV view. Both views allow topology editing, retain selection and use separate cameras.
Switching settles the active drag. Folded or singular poses explain an ambiguous
inverse and retain the last valid candidate; the source view remains available.
Other Modeling viewports retain their model view and cannot acquire this edit gesture.

New and moved points resample every stored shape from the operation's frozen UV
source. Glue endpoint markers remain visible; deleting a referenced endpoint
requires releasing its pair first. Outside-domain operations explain their refusal
without closing the mode. Interior generation retains identities and Glue; edge
and full rebuilds require that no old vertex is referenced. A generation preview
is applied explicitly; saving only settles an active point drag.

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
