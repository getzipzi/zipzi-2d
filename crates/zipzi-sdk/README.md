# zipzi-sdk

Engine-independent Rust authoring SDK for document editing, history, project
operations, animation and CPU geometry queries.

## Sessions and editing

`AuthoringSession` owns the document and undo/redo history. `EditSession` edits
an isolated candidate; commit publishes one history step. Cancellation, failures
and unchanged edits preserve redo. `zipzi-project::DocumentSession` handles
project and resource I/O without a separate history stack.

Candidate snapshots retain their base version and memory image content. Hosts
should track edit identity and sequence and capture diagnostics before consuming
a candidate. Disk bytes are not pinned. `zipzi-runtime` hosts these same
contracts for shared clients.

`EditorState` stores persistent visibility and lock flags. Hierarchy and
parameter display ordering are document metadata and participate in history.
`resolve_mesh_ids`, `object_bounds` and `hit_test_geometry` query evaluated
geometry without a renderer or textures.

## Mesh editing and generation

`MeshMeshing` optionally records a UV boundary and internal constraints.
`extract_manual_meshing` validates existing triangulation; `edit_manual_mesh`
returns edited geometry and stable ID mappings. Use a persistent
`ManualVertexIdAllocator` for insertions. For document edits, use
`EditSession::plan_manual_mesh` and `apply_manual_mesh_plan`, or
`EditSession::edit_manual_mesh`. The complete plan transfers base positions,
ordinary keyforms and every Mesh BlendShape with the same frozen UV weights,
retaining local authoring space, parent references, non-position channels and Glue.

Repeated moves of the same vertex set in one candidate sample the first accepted move's source, while
swept checks start at the latest accepted UVs. A rejected plan leaves that preview
usable. Plans are bound to an exact document snapshot and SDK version; another
branch with an equal revision is still rejected. Hosts must also guard their edit
identity and sequence, as Runtime does for `ModelingCommand::ManualMesh`.

Deleting a Glue endpoint (including an incomplete pair or an implicit orphan)
returns `GLUE_VERTEX_REFERENCED`; a sample outside the old UV coverage returns
`OUTSIDE_TRANSFER_DOMAIN`. Only a dependency-free mesh with a global affine map
can extend outside that coverage. Non-affine and collapsed base positions remain
editable inside a valid UV domain. Full generation uses fresh persistent IDs and
is disabled when old Glue endpoints would be retired; interior fill retains them.

`ManualMeshSource` supports generated UV topology through the same plan builder.
`ManualMeshTransferReport` describes channel count, affected vertices, Glue and
nonlinear parents, appearance limitations and finite conversion/history costs.
The report promises stored sampling semantics, not identical pixels for arbitrary
reconnections, nonlinear parents or Glue. Unchanged dependencies carried through
a complete replacement do not require write access; equality is checked in Core.

`alpha_mesh_geometry` generates CPU mesh geometry from a top-down alpha mask.
Options control spacing, margins, threshold, holes and image clipping; distances
are source-image pixels. Output positions use a bottom-left origin and UVs are
normalized. With `clip_to_image: false`, callers must pad the texture and remap
out-of-range UVs. The operation does not transfer keyforms, BlendShapes or glue.
Empty masks, invalid options, geometry failures and exceeded limits return errors.

```sh
cargo test --locked -p zipzi-sdk --test integration manual_mesh::
./tools/test-sdk.sh mesh
```

## Animation preview

`MotionPreview` provides independent playback with motion, expression, physics
and pose evaluation. `MotionEditingPreview` samples a raw clip from the default
baseline without fades, expressions, physics or events. `motion_positions` and
`canonical_motion_seek_time` expose SDK loop coordinates for timeline seeking.

`MotionClipCandidate` supports identity-based key editing and isolated preview;
commit it against the original version. `advance_to_with_progress` and
`seek_with_progress` support cancellation, with different rollback behavior:
incremental advance retains completed steps, while seek retains the prior state.
Expression schedules can be replaced without modifying the authoring document.

## Images and modeling

`prepare_rgba_image` returns a lease that can be passed to image creation or
replacement. Candidates, snapshots, history and leases retain their referenced
content; unused resources can be collected after those owners are dropped.
`ImageMemoryBudget` bounds shared image storage and preparation.

Modeling plans expose source-bound cost and loss reports. Mesh binding specs
accept numeric axes and keyforms; SDK/core allocate identities and validate grids.

## Tests

Run focused tests from the repository root:

```sh
./tools/test-sdk.sh fast physics_editing::
cargo test --locked -p zipzi-sdk --test integration motion_editing::
cargo test --locked -p zipzi-sdk --test integration mesh_binding_editing::
./tools/test-sdk.sh mesh
```

See [test coverage](../../tools/README.md#test-coverage) for regression and
stress modes.
