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
it with `prepare` and `AuthoringSession::publish`. Commands cover parameters,
ordinary/BlendShape bindings and keyforms, range/grid conversions, scene and
organization edits, reparenting/grouping, dependency deletion, mesh topology and
manual/alpha mesh generation, Motion/Expression/Pose/Physics, references,
resources, attachments, document fields and EditorState. Motion curve commands
retain the existing interpolation, Bezier split/handle and time-scaling rules;
command failures abort the entire Draft.

Commands install affected records through Core Builder tracking. Typed patches
check expected values; domain operations enforce owner/child locks, preserve or
remap stable identities and remove orphan EditorState. Forward references may be
assembled in a batch, whose complete graph must validate before success.
Canonical axis/sample identities remain in the working Builder for later commands.
Absolute vertex positions can be calculated from Draft::baseline without
accumulating gesture deltas. Conversion admission bounds actual work independently
of history policy. Manual mesh transfer samples ordinary and BlendShape channels.

Frozen queries expose domain projections, reference/deletion analysis, binding
samples and evaluation. Query source is carried by SourcedSnapshot. Animation and
format libraries now consume frozen state; host orchestration remains stage four.

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

Stage three domain migration is complete. Core's old mutable Document and
checkpoint/transaction/planner machinery, SDK full-document diff inference and
Project live-document ownership have been removed. Format adapters consume
DocumentState and owned ResourceView values directly.

The fast gate covers SDK library and authoring integration tests. Alpha mesh
algorithms also have a standalone `cargo test --locked -p zipzi-sdk --test alpha_mesh`
gate. Remaining old integration targets and examples are migration inputs for
host migration and final cleanup; these gates do not imply their full coverage.

Observe, Runtime, Editor and the Python native adapter still consume removed APIs
and do not yet compile against this intermediate SDK. They switch in stage four
of the [architecture plan](../../docs/plans/authoring-architecture.md).
Whole-workspace, legacy regression/full and client gates are not claimed passing.
