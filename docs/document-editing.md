# Document editing

The editor is a Rust client of `kasane-sdk`. SDK owns model semantics, transactions,
validation, publication and undo/redo. No Python runtime, IPC, second history stack
or event bus is introduced.

```text
editor (EditorView: GPUI, controls, native dialogs, executor and preview composition)
  ├── editor-core
  │     ├── Editor (window workspace, document identities, tasks and close/exit)
  │     │     └── DocumentSession (viewport, scene cache/revisions and ToolController)
  │     │           └── Document → kasane-sdk::AuthoringSession
  │     └── SceneSnapshot (evaluated frame and shared decoded textures)
  └── editor-render → kasane-render + kasane-render-metal
        └── immutable Metal texture → GPUI composition
```

`editor-core/src/workspace.rs` implements the public `Editor`. Each model has a
runtime `DocumentId`, independent of model UUID and project path. `session.rs`
owns each tab's model/view/tool state as a private `DocumentSession`. No new crate is needed: these modules have the same dependency
boundary and neither depends on GPUI or GPU types. `editor.rs` declares editing
intents, file requests and preview inputs.

Opening/importing adds a tab only after success. The existing document and view
survive failure. Opening a project directory, its manifest or an existing symlink
alias activates the same tab without replacing unsaved edits. Save As rejects
another open document's location, including a destination reserved by a pending
save/load. A duplicate model UUID in two files remains two independent sessions.
PSD publication still uses SDK rules: an existing destination is rejected.

`LoadTask` and `SaveTask` are non-cloneable ownership envelopes containing document
and operation identities. Hosts run them on a background executor and return the
completion to core. Completion never resolves its target from the active tab.
`SaveTask` owns the original document session; the tab holds only a read-only
summary, property presentation and viewport while it saves. SDK errors return the
same session and history. A save delivered to the wrong workspace is rejected
without consuming the completion, allowing delivery to the owner. No UI-blocking
write lock, disk reopen or alternate project writer is used.

Save freezes only its document. Other tabs can edit, open and save. A close during
save waits for the result, closes only after success and leaves the tab open on
failure. Dirty close offers Save/Discard/Cancel. Cancelling a picker aborts the
close/exit. Closing the last document leaves an empty workspace; New/Open/Import
still work. Window close and Quit process documents in order; cancellation stops
that process (documents already closed stay closed). Exiting while a load is
pending reports that loading must finish before exit.

`Document` owns editor selection and a persistent SDK candidate for name/opacity
editing. Enter/blur and slider release commit; Esc cancels name editing. Switching
tabs commits property editing and restores that tab's selection and viewport.
An unchanged candidate does not consume history or discard redo. The SDK remains
the sole owner of model history. Save/close/tool coordination cancels canvas candidates before moving or closing a session.
Vertex drags use a fixed target and original positions, update SDK candidates, and
commit the final release position once; cancellation, no-op and invalid updates
preserve model history and redo. Only base meshes without parameter, deformer,
BlendShape or Glue dependencies are editable.

The UI creates property controls with a captured document identity. Switching tabs
replaces their entities/subscriptions, preventing late old-control events and text
undo history from crossing documents. File dialogs are modal in the UI adapter;
background IO is not. Canvas tabs are central and the layers/properties panels
follow the active document. Cmd-W closes the active model, window close is a
separate menu action, and Ctrl-Tab/Ctrl-Shift-Tab navigate models.

`commands.rs` owns stable command IDs, localized label keys, availability and repeat
policy; `settings.rs` validates whole shortcut configurations and atomically writes
versioned preferences. The host normalizes keys using GPUI's platform mapper,
installs only effective bindings, and rebuilds native menus from the same catalog.
Text, canvas and modal contexts remain GPUI contexts; there is no second dispatcher.
Layout uses the locked Kit DockArea/DockSkin. Canvas is permanent in the center;
shared Layers/Properties panels are independent of document lifetimes. Invalid
persisted layouts reset; invalid Canvas regrouping reverts the previous valid layout.

Only the active document submits previews. Every frame carries document, scene, viewport and request identities. Scene revisions
include uncommitted candidates. Hover and selection only rebuild overlays; viewport
changes reuse the evaluated scene. CanvasView measures actual logical bounds and
DPI, and core provides the shared source/logical/physical transform. A new frame
replaces the image and its geometry together; hit tests use the displayed geometry,
and active gestures keep their original targets. Hidden/zero canvases do not submit.
The worker has one current request and one replaceable latest request, with a bounded
completed-frame channel. During save the adapter retains that document's last image.

GPU resources are keyed by immutable content hash and dimensions; per-scene asset IDs
are aliases, so equal IDs in different documents cannot cross-contaminate textures.
The recent texture cache has a 128 MiB budget based on the measured two-model working
set. Least-recently-used inactive textures are evicted when over budget; an active
working set may exceed it. Closing a document does not immediately flush the recent
cache. Production rendering uses the shared GPUI Metal queue without waits/readback.

Tests cover document isolation, directory/manifest/symlink deduplication, save
conflicts and reservations, failed opens/saves, close-after-save, cancellation,
wrong-workspace completions, stale preview rejection and real thread ownership
transfer with history preservation. SDK document and native Metal regression
tests remain in place. Native focus, IME, dialogs, tabs and window close require
manual verification; no computer-use tests are run.

The completed steps, configuration format, limitations and manual checklist are recorded in
[foundation implementation](editor-foundation-implementation.md).
