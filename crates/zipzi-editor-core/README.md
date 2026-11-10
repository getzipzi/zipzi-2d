# zipzi-editor-core

UI-independent Editor coordination built on the resident `zipzi-runtime`
host. This crate owns document views, previews, selection, tools and workspace
state. Runtime owns the formal SDK sessions and their history. EditorCore has
no GPUI or Metal dependency.

The public `Editor` coordinates documents and load/save work. `Document` exposes
Editor authoring operations backed by SDK sessions. `SceneSnapshot`, `ViewState`
and viewport transforms provide the rendering boundary consumed by
[`zipzi-editor-render`](../zipzi-editor-render/README.md) and the application.

## Source map

- `src/workspace/`, `src/session.rs`, `src/document/`: document lifecycle and authoring.
- `src/property/`, `src/inspection.rs`: typed property edits and inspection.
- `src/animation/`, `src/timeline/`, `src/physics.rs`: animation and physics editing.
- `src/transfer.rs`: import/export coordination.
- `src/tools/`, `src/picking/`, `src/viewport/`: interaction, picking and view transforms.
- `src/adaptive_mesh/`: PSD2Live-derived contour/band and interior fill strategies; see its README for provenance, API and reference tests.
- `src/scene.rs`: immutable scene data for previews.
- `src/lib.rs`: public types and exports.

Keep native widgets, GPU resources and window lifecycle in the consuming crates.

Reusable model semantics belong in the SDK; this crate supplies Editor policy
and coordination around them.

Control previews keep the panel object index current as poses change. Successful
non-topological control commits retain that exact candidate index, advancing its
projection revision instead of rebuilding the entire relation graph on mouse-up.
An absent/stale cache, topology changes, cancellation or failed publication still
uses the normal invalidation path. The `mesh_edit::` tests compare retained Mesh
and Warp indexes with fresh builds through commit, cancel, Undo and Redo.

See the [viewport interaction contract](../../docs/archive/references/viewport-interactions.md)
for gesture ownership, capture lifecycle and cancellation rules.

## Test

Run from the repository root:

```sh
./tools/test-editor.sh fast
./tools/test-editor.sh fast timeline::
cargo test --locked -p zipzi-editor-core --test integration
# Local licensed-model and imported-animation acceptance:
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test acceptance -- --nocapture
```

`fast` runs library and synthetic integration tests without native UI acceptance.
The `acceptance` target includes optional local-model cases; missing optional
assets can skip. See [test gates](../../tools/README.md#test-coverage) and
[Editor development](../../crates/zipzi-editor/README.md) for required-asset flags.
`bench-instrumentation` forwards instrumentation to the underlying SDK/core.

Licensed **GPL-3.0-only**; not published to crates.io. See
[component licensing](../../LICENSING.md).

## Runtime ownership

`RuntimeHost` owns the sole formal SDK/history. `Document` holds a SessionKey
adapter, view projections, selection and preview state; formal writes and event
consumption pass through Runtime. `Editor::with_runtime` attaches a workspace to
an application host. `detach_document` retains resident content/history, while
explicit close releases it after save/discard coordination.

View reads capture immutable snapshots. Gestures use Runtime `DraftSource`
values, and another client can publish and invalidate a captured candidate.
Save admission can hold a gesture for retry. Textures come from the captured
resource version. A confirmation owns the Draft snapshot it displays, and
field captures contain stable IDs, expected values and the full Version.
Conversion, deletion and mesh workers prepare candidates, and publication
invalidates projections through the WriteSet.

The `authoring_kernel` target covers native confirmation and resource workflows.
The `integration` target covers synthetic Editor workflows. The `acceptance`
target adds import, export, and licensed-model cases.

Background load/save tasks carry Runtime Work, never Editor view state. Publish
a Completion into Runtime before attempting a weak view callback.
`sync_runtime_jobs` restores pending view state and reports failures even after
window detachment; `attach_resident_sessions` rebuilds views on reopen. Decoder
workers prepare textures/object indexes before returning to the UI.

Focused lifecycle regressions:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test authoring_kernel
```
