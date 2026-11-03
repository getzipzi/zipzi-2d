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

Mesh transfer budgets count copied snapshot metadata, affected records, geometry
workspace and rebuilt identity indexes. Publication reuses the candidate's vertex
indexes, including those shared with unrelated meshes. Unchanged shared records
do not consume the operation's temporary workspace; retained history keeps its
logical content charge. The default conversion ceiling remains 64 MiB. To verify
allocation peaks against estimates on a local native project without saving it, run
`cargo run --locked -p zipzi-sdk --example manual_mesh_project_profile -- PROJECT`.

`EditSession::edit_manual_mesh_batch` preflights one operation per mesh before
publishing any candidate change. Moves retain separate frozen sources for each
mesh across increments, and rejection preserves the entire last accepted batch.
Committing the shared Edit creates one undo step for all meshes.

Deleting a Glue endpoint (including an incomplete pair or an implicit orphan)
returns `GLUE_VERTEX_REFERENCED`. Manual moves and insertions can extend the
outline anywhere inside the complete source image, including transparent image
padding. Outside the old UV coverage, the nearest source triangle supplies affine
extrapolation weights shared by base positions, ordinary keyforms and BlendShape
deltas. Model-space gestures reject ambiguous inverse branches; source-image
editing remains available. Full generation outside the old coverage still returns
`OUTSIDE_TRANSFER_DOMAIN` unless the source is dependency-free and globally affine.
Non-affine and collapsed base positions remain editable inside a valid UV domain.
Full generation uses fresh persistent IDs and
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

Warp authoring uses typed `SceneContentEdit::WarpAuthoring`, `WarpBezierControls`
and `WarpShape` operations. Authoring settings persist with the Transform and
copy with it. Bezier density is independent of conversion density; changing it
refits every existing controller while retaining missing controllers and every
baked lattice. The finite-difference controller refit is approximate.
`update_warp_points` preserves stored controls. Later Bezier edits add their
controller delta to the lattice unless auto reform is enabled. Reset rebuilds
controls without moving the lattice; explicit Reform samples the current
controller and discards lattice detail. Unchanged controls are no-ops even with
auto reform enabled. BlendShape lattice edits remain available, but independent
BlendShape controllers and controller shape actions are unsupported.

The local, cosine-neighborhood, Gaussian-global and opposing-tangent-ratio
policies are Zipzi authoring algorithms. Native projects retain them; CMO3 export
reports authoring metadata loss, including these settings and Bezier handles.

`SceneContentEdit::WarpKeepChildren` replaces a static affine Warp at its existing
density. `RotationKeepChildren` replaces a static Rotation. Both use one conversion
and verification pipeline to compensate all direct child shape channels. Mesh and Warp
positions/controllers use `G⁻¹F`, BlendShape displacements use its linear part,
and Rotation children reuse inherited-frame compensation. Deeper subtree data
retain their inputs. Static rectangular Warp/Rotation ancestor spaces are
supported. Driven or nonlinear edited/ancestor maps, singular maps, locks and
the fixed 0.01 canvas-unit numerical budget can reject the entire plan. Reports
expose measured preservation deviation and sample count. Samples include native
interior/exterior maps and default/per-driver endpoint, key and midpoint poses;
they supplement affine representability, without asserting a global error bound.

`SceneContentEdit::DeformerOriginal` records or restores an independent canvas
shape for an explicit base/ordinary target, or changes its automatic first-key
policy. New deformers record their final creation shape. Automatic capture occurs
at most once when ordinary keyforms first appear; manual Update disables it.
Imported or older objects without a record remain unrecorded until an explicit
update. Native persistence, copies and history retain the snapshot, source and
first-key marker; CMO3 reports their loss.

Supply frozen parameter values in the edit's `context` to evaluate parent spaces;
an empty context uses parameter defaults. Session creation and grouping plans
capture the session's preview values, as do owned edit candidates. Direct Core
creation uses defaults unless its capture context is set explicitly. Restore
writes only the requested shape through its current parent inverse and retains
the current Rotation reference angle. `Restore { allow_resize: true }` explicitly
allows shared conversion-grid/interpolation resampling and stored-controller
density refits; callers should review these effects before publication. Missing
controllers remain absent and derive from the current authoring configuration.

Nonlinear/singular current parent spaces, incompatible types, active deformer
BlendShapes, locks and tolerance failures reject atomically. A stored controller
captured under a nonlinear parent cannot be restored as equivalent canvas cubics;
a baked lattice without such a controller can be restored below an affine parent.
Stored-controller restoration checks 289 independent canvas samples against
0.01 units, without treating their maximum as a global error bound.

`AuthoringSession::plan_deformer_group` combines a Warp or Rotation `SceneEdit::Create`
with selection parenting in one snapshot-bound, undoable plan. It normalizes
selected ancestors and descendants, preserves organization ownership and drawing
order, and converts base shapes, ordinary keyforms and BlendShape deltas together.
Roots must share a deformation parent. A new Warp can pass through an existing
animated Warp lattice; a new Rotation below a Warp requires a static axis-aligned
affine space. Creating a Rotation group under a Rotation with reflection in its
base pose or any ordinary keyform is rejected; a Warp group remains supported.
Unsupported coordinate conversions reject the entire plan. Before publication,
grouping also compares native evaluated positions (including hidden meshes) at
default parameter values within 0.01 model units plus a relative 1e-6 tolerance.
This numeric check supplements the structural checks; it does not prove the
entire animated parameter domain.

`ReparentEdit` domain preservation supports static affine Warp lattices as parent
spaces, including nested Rotation/Warp chains. Lattice admission allows float32
construction rounding. Its discarded lattice residual is charged to the geometry
error bound over ordinary and active BlendShape coordinate envelopes, including
descendant Warp extrapolation. Rounded Warp spaces that leave Rotation direction
unproved reject full-domain preservation. Mesh and Warp base shapes, controllers,
ordinary keyforms and BlendShape deltas convert together. Moving a Warp also
compensates scalar scale inheritance at the first Rotation on each descendant
branch. Subtree structure and independent canvas original-shape records remain intact.

`PreserveEditState` explicitly bakes a nonlinear source parent's captured effect
into Mesh, Warp or Rotation channels. Ordinary binding identities and scalar
channels remain editable; nonlinear sample conversion is recentered to preserve
the captured native pose. Other parameter poses and future Bezier edits may
change. It requires an affine destination with a unique inverse; distorted
destinations still reject. `KeepLocalData` remains explicit and performs no
coordinate conversion. All preservation plans verify native geometry, including
hidden meshes, at captured parameter inputs and enforce the requested absolute
plus relative tolerance. Editor parent properties, tree moves and deletion
share this conversion pipeline; Editor moves default to full parameter scope
and the deletion tolerance of 0.001 model units plus relative 1e-6.

Rotation subtree roots also support preservation: reparenting compensates the
root's base reference angle, origin and scale and its ordinary/BlendShape origin
and scale channels. Descendant data stay unchanged, including nonlinear Warp
descendants. Keeping angle channels intact preserves native BlendShape angle
clamping. Plans check scale-clamp compatibility and verify the resulting native
frame at the frozen edit inputs before publication. Hierarchy and all converted
channels publish atomically; undo restores their original values and identities.

The dependency-cleaning deletion API supports scene objects, parameters,
parameter folders, textures, attachments and typed business records (including
Motion, Expression, Pose, Physics, bindings and metadata children). Part roots use
the organization subtree; Deformer roots use the deformation subtree. Explicit
root scopes are unioned before deduplication.
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
Runtime, Python Local/Connected and Editor entries share this review path.

Parameter axes reduce at the frozen document default, or an explicit
`parameter_slices` coordinate, using native repeat/snapping and grid conversion.
Other dimensions and targets remain; the last dimension becomes static data.
Dependent Blend tables/bindings, constraints, animation/Expression records,
Physics channels and resolved metadata members are cleaned together. Native
invalid containers require an explicit larger deletion or cancellation.
`ParameterFolder` dissolves by default; `ParameterFolderContents` opts into its
parameters. Retained texture users require `texture_replacements` (a retained
UUID or explicit `None` for unconfigured). Optional attachment references clear.
`physics_node_replacements` and `blend_base_replacements` retain native node/base
selection semantics. Removing a Motion group migrates the last registration of
each retained Clip to the root; deleting that registration explicitly requires
choosing Clip deletion or cancelling. Part/drawing and Glue/binding roles retain
distinct typed identities even when they share an ID.

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

Dependency-cleaning scene deletion uses `DeletionRequest` and the source-bound
`DeletionPlan`; `DeletionTarget::Part` always selects organization contents and
`Deformer` always selects deformation contents. Known Mask, Glue, Hit Area,
Part animation/Pose and CDI users are cleaned in one atomic edit. Retained
cross-tree members require native preservation or an explicit migration choice.
`ModelingIntent::Deletion` returns a `DeletionReviewPlan`, including unresolved
or blocked reports. `summary()` and `report_page()` keep business removals,
owned internal identities, rewrites, proofs, choices, locks and export regressions
separate. Runtime/Python/Editor use this modeling route for source, budget and
cancellation management. Raw strict scene deletion remains a separate API.
