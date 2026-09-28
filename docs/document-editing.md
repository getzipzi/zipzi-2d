# Document editing

The editor is a Rust client of `kasane-sdk`. The SDK owns the model, validation,
project publication, undo and redo; editor code owns selection and viewport state.
No `kasane-sdk-observe`, Python, WGPU or backend selection is needed for this path.

```text
editor (EditorView: GPUI controls, project dialogs and host task scheduling)
  ├── editor-core
  │     ├── Editor → Document → kasane-sdk::AuthoringSession
  │     └── SceneSnapshot → evaluated frame + shared decoded textures
  └── editor-render → kasane-render + kasane-render-metal
          └── immutable Metal texture → GPUI composition
```

`editor-core/src/editor.rs` owns the UI-independent `Editor`: its document,
viewport, busy state, command availability, close disposition and preview request
revision. The UI has read-only access to the document and sends `EditorCommand`
values for edits. Busy checks are enforced in core as well as reflected in the
controls. `LoadRequest` performs open/import work on the host's background
executor; `Editor::finish_load` replaces the document and resets the viewport
only on success, preserving the existing state on failure. Core creates preview
inputs and rejects stale preview revisions. It has no GPUI or Metal dependency.

`editor/src/main.rs` owns `EditorView`, the GPUI adapter. It translates input
events to core commands, synchronizes controls, formats localized errors and
notices, and connects preview inputs to the native renderer. Dialog presentation,
focus, window lifecycle and executor integration remain in this UI crate.
This separation retains the current single-document behavior; multi-document
workspace support is a subsequent step.

`editor-core/src/document.rs` creates and opens documents, selects meshes,
and translates UI editing gestures into SDK transactions. A persistent
`EditSession` previews a candidate document while the committed session remains
unchanged. Enter/blur commits a name; slider release commits opacity. Switching
properties or selection also finishes the current gesture. Cancel drops the
candidate. Gestures that return to their original value do not consume an undo
step or discard redo. Visibility changes are immediate SDK transactions.

`editor-core/src/scene.rs` defines CPU-only render inputs and independent
viewport state. The document evaluates committed previews through the SDK's
preview cache and candidates through the core evaluator. Decoded texture bytes
are loaded when the document opens and retained with `Arc`, so a queued snapshot
does not depend on files or the live editing session remaining alive. External
asset changes require reopening the project. Future texture editing must refresh
these resources as part of the document operation.

`editor-render` receives those values and owns GPU resources. It uploads actual
document textures, submits on GPUI's queue and returns a new immutable output
texture. It does not know about selection, history or project paths. The GPUI
worker/Surface integration remains in `editor/src/preview.rs`; project dialogs
live in `project.rs`, and layout in `ui.rs`.

`editor/src/menus.rs` installs the native application menu and keyboard bindings.
The editor root has a focus handle and command context; clicking the canvas moves
focus out of the name input. Native menu commands invoke the editor operations.
Kit's Undo/Redo action types are reused so the focused text input handles its own
history before the editor's document handlers. Document handlers are registered
only when available, allowing AppKit to validate their menu items. Close and Quit
use the same unsaved-document prompt as closing the single editor window.

The editor starts with an empty SDK document, with no layer or texture. Tests
create a separate fixture through the SDK. Opening a project creates and
validates a replacement document in a background task before replacing the
current one. Missing/corrupt resources fail
the open operation in this version; a resource-repair UI is not yet implemented.
The File menu imports model3 as an unsaved document or PSD into a new project
directory through the SDK. Import also runs in the background and keeps the
current document if loading fails. Saving runs synchronously and surfaces SDK
errors and publication warnings.

Tests cover gesture grouping/cancellation, selection, undo/redo, immutable
snapshots, project/texture round trips and failed publication. The native Metal
tests render SDK-backed inputs with GPUI and verify composition, opacity,
visibility, resizing and retained frames. File dialogs, IME, focus transitions
window-close interactions and native menu/focus routing still require manual
verification; no computer-use tests run.

Core editor tests additionally cover busy-operation guards, failed-load state
retention, preview invalidation, and save/reopen through the editor controller.
