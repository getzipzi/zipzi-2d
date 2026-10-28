# zipzi-editor-core

UI-independent Editor coordination built on `zipzi-sdk`, `zipzi-core` and
`zipzi-project`. This crate owns open documents, editing sessions, previews,
selection, tools and workspace state. It has no GPUI or Metal dependency.

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
R1 adds `HostedDocument`, a typed SessionId reference that borrows Runtime's
original SDK session without owning a second authoring instance. The integration
fixture verifies shared publication/history. Existing `Document` ownership and
native window lifecycle migrate in R2; this bridge does not claim that migration.

Reusable model semantics belong in the SDK; this crate supplies Editor policy
and coordination around them.

See the [viewport interaction contract](../../docs/references/viewport-interactions.md)
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

`RuntimeHost` owns the sole formal SDK/history. `Document` holds a SessionId
adapter, view projections, selection and preview state; formal writes and event
consumption pass through Runtime. `Editor::with_runtime` attaches a workspace to
an application host. `detach_document` retains resident content/history, while
explicit close releases it after save/discard coordination.

Background load/save tasks carry Runtime Work, never Editor view state. Publish
a Completion into Runtime before attempting a weak view callback.
`sync_runtime_jobs` restores pending view state and reports failures even after
window detachment; `attach_resident_sessions` rebuilds views on reopen. Decoder
workers prepare textures/object indexes before returning to the UI.

Focused lifecycle regressions:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration runtime::
```
