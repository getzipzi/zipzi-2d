# zipzi-sdk

Rust authoring API built on immutable Core state and Project resource ownership.
The crate root now exports the new `AuthoringSession`, `Snapshot`, `Draft`,
`PreparedChange`, source identities, publication receipts and soft history types.
The old EditSession/checkpoint/field-handle API is no longer exported or executed.

## Native authoring workflow

`Snapshot::new` validates and pairs a `DocumentState` with its exact `ResourceView`.
`AuthoringSession::new` starts an unsaved document. `AuthoringSession::open` returns
an opened session and resource diagnostics. Project owns no live authoring state.

Call `begin_draft`, use `Draft::batch` with its expected `DraftSource`, then consume
it with `prepare` and `AuthoringSession::publish`. Commands currently cover typed
asset/parameter/part/transform/mesh names, stable-ID base vertex positions, part,
mesh and parameter creation, empty assets and immutable image replacement. A batch
can assemble forward references; its complete graph must validate before success.
Commands enforce organizational locks. Absolute vertex positions allow gestures
to calculate from `Draft::baseline` without accumulating deltas.

Multiple Drafts may coexist. Ordinary command failure, an ignored command error,
an invalid final graph or a panic ends that Draft. Preparing or publishing consumes
the attempt. The exact Session/generation/publication source must still match;
there is no automatic replay or rebase. Worker values can be sent across threads
without moving the Session. Notifications are queued after formal installation.

`read_snapshot` preserves whether a query observed published or candidate content.
Cloned snapshots and their image versions remain valid after Session closure.

## History and project I/O

Undo/Redo restore snapshots and always advance the publication revision. A stale
query cannot become valid again after Undo restores an older ContentId. No-op and
failed edits preserve redo. History has an independent `max_steps` limit and a
soft `target_bytes` goal. An oversized latest step remains undoable; Undo/Redo do
not reject or trim on bytes. `max_steps: 0` explicitly disables retained history.

`save` uses the current project path and fingerprint. `save_as` publishes to a new
path or requires an explicit existing-target fingerprint. It changes destination
metadata without rewriting content, resources or historical paths. `SavedMarker`
contains only the saved ContentId. Undo to that node becomes clean; manually
recreating identical values in a new node may remain dirty.

Save holds an exclusive mutable Session borrow through capture, projection,
atomic file publication and marker installation. Existing snapshot holders may
continue reading. There is no concurrent edit/replacement/save through that borrow,
and I/O does not take ownership of the Session. File publication failure preserves
the old marker and destination. Cancellation after publication and directory-sync
failure return a published receipt (with durability warnings) and install the marker.
Asynchronous host project-task coordination belongs to the later host migration.

`PreparedProject::load` is a value-only load result with its original Version;
`publish_loaded` rejects stale results before replacing generation, history,
project context and saved marker. Failed loads preserve the current document.

Run a complete native load/edit/history/save-as workflow with:

```sh
cargo run --locked -p zipzi-sdk --example native_authoring -- INPUT.zipzi NEW_OUTPUT.zipzi
./tools/test-sdk.sh fast
cargo test --locked -p zipzi-sdk --doc
```

## Migration boundary

Stage two is complete for the native workflow above. The previous domain algorithm
sources, `tests/integration.rs`, `tests/alpha_mesh.rs` and earlier examples remain
as migration inputs for stage three. They are not compatibility entrypoints and
still require migration before their original Cargo targets compile against the
new crate root. The current fast gate is `--lib --test authoring`; it is not the
former full domain coverage under another name.

Observe, Runtime, Editor and the Python native adapter still consume the removed
API and do not yet compile against this intermediate SDK. They switch in stages
three/four as specified in the [architecture plan](../../docs/plans/authoring-architecture.md).
Whole-workspace, old SDK integration/mesh/regression/full, and client gates are not
claimed passing for this stage. Independent Core/Project behavior remains tested.
