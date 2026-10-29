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

A plan owns the fully checked candidate and its Core change receipt. Applying it
checks the exact publication snapshot and installs that result, without a second
full topology replacement. Source admission and its memory estimate are reused
through a drag; unchanged vertex slots are copied once per stored channel and
only changed samples are patched. Conversion budgets, lock checks and cancellation
remain part of planning.

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

The dependency-cleaning deletion API supports Mesh, Part, Deformer, Glue and
Offscreen roots. Part roots use the organization subtree; Deformer roots use the
deformation subtree. Explicit root scopes are unioned before deduplication.
`DissolvePart` and `RemoveDeformer` preserve contents and report their migrations.
Use `deletion_request` to resolve current handles, `analyze_deletion` to
review typed removals, field rewrites, locks, migration choices and opaque export regressions,
then `plan_deletion` and `execute_deletion_plan` for one atomic history step.
`EditSession::plan_deletion` can compose cleanup after earlier candidate edits;
its plan is valid only in that Edit at the captured candidate snapshot.
`AuthoringSnapshot::plan_deletion_with_control` supports background preparation
with limits and cancellation. SDK analysis prepares an isolated candidate when
the native structural analysis has no unresolved choices or locks; inspect
`export_analysis_complete` before interpreting its export results. It can run
conversions and must not be used for per-frame command-state queries. Core's
`analyze_deletion` remains a structural scan, while `prepare_deletion` returns
either the validated plan or a typed report of required choices and blockers.

Deletion removes owned bindings and internal identities, unlinks surviving Mask
consumers and removes affected Glue/Hit Area, Pose, Motion, Expression source-scene
and resolved CDI records. Surviving cross-part deformation children migrate to
the nearest retained deformation ancestor, using parameter-domain preservation
when proved. Unsupported or numerically unproved conversion requires an explicit
`reparent` resolution; no local-data fallback or scope expansion is automatic.
`conversion_context` freezes explicit values, domain and time. `tolerance`
defaults to 0.001 model units plus 0.000001 times the geometry scale; the native
conversion guarantee reports the actual tolerance and error bound.
Offscreen owner and mapping changes use `offscreen_placements`. Drawing groups
flatten surviving external members in stored expansion order and report lost
group ranges, dynamic order and compositing effects. Shared images, parameters,
Blend tables and constraints remain. New opaque export failures
require explicit acceptance through `DeletionOptions`; provenance and later
export checks remain intact. Strict deletion APIs retain their existing behavior.
Runtime/Python transport and Editor review integration are not available yet.

```sh
cargo test --locked -p zipzi-sdk --test integration deletion::
```

`prepare_rgba_image` returns a lease that can be passed to image creation or
replacement. Candidates, snapshots, history and leases retain their referenced
content; unused resources can be collected after those owners are dropped.
`ImageMemoryBudget` bounds shared image storage and preparation.

Modeling plans expose source-bound cost and loss reports. Mesh binding specs
accept numeric axes and keyforms; SDK/core allocate identities and validate grids.

`GeometryChecks.quality` adds per-pose `MeshQuality` rows to `ModelingSamples`.
These advisory model-space angle, aspect, area, edge-stretch and flip metrics
are separate from authored validity diagnostics. Ratios use the same snapshot's
default-parameter evaluation, including hidden reference meshes. Degenerate
denominators produce `None`, and reports certify only the requested poses.
`mesh_quality` also compares matching evaluated frames directly. GPU observation
can add source-alpha coverage and `TopologyOverlay` drawings in the same view.

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
