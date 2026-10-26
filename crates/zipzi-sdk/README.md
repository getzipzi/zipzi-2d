# zipzi-sdk

Engine-independent Rust authoring SDK for session editing, history, project
operations, and CPU geometry queries.

`resolve_mesh_ids`, `object_bounds`, and `hit_test_geometry` operate on one
evaluated `DrawableFrame` plus its frozen `Mesh` and `Part` records. They do not
need a renderer or textures. The observer's Python binding calls them on a
captured scene; the analysis-packet reader reuses them after reopening saved
geometry, including in wheels built without GPU observation.

Palette sorting uses `EditSession::replace_hierarchy_order(HierarchyOrder)`.
`Document::hierarchy_order()` exposes independent organization and deformation
ID ranks. These metadata edits participate in checkpoints, history budgets and
project IO, preserve evaluation revisions, and are removed when their objects
are erased. Empty ordering is omitted and source ordering is the default.

Parameter and parameter group display order use the ordered CDI collections in
`DisplayInfo`. Replacing those collections preserves runtime parameter storage
order and uses the normal validated metadata transaction.
The editor materializes missing resolved CDI entries when sorting its complete
visible parameter list, while retaining unresolved entries and extensions.

Editor visibility and protection use `EditorState` and
`EditSession::replace_editor_state`. The sparse map stores explicit
`hidden_in_editor` and `locked` flags for supported native objects and owned rows.
Editors resolve Part inheritance and composite visibility; deformation parentage
does not propagate editor flags. SDK writes enforce persistent locks, including
direct replacements and dependent objects. Metadata participates in modified
state, history budgets, deletion cleanup and undo/redo. Locks preserve runtime
inputs; Editor visibility also refreshes the host preview. The old
`ObjectLocks` and `DeformerDisplay` APIs and wire fields are removed, with no
migration adapters.

## Session and history ownership

`AuthoringSession` owns document undo/redo through checkpoint history.
`zipzi-core` supplies persistent-content checkpoints, candidate publication and
restoration; `zipzi-project::DocumentSession` owns project/resource I/O and keeps
no history stack. Interactive edits preview an isolated candidate and commit one
history step. Cancellation, unchanged edits and failures preserve the redo branch.
The SDK coordinates history resource paths and the saved baseline when saving.

`AuthoringSession::candidate_snapshot` captures an owned edit with its base
Version and referenced memory image content, without changing formal state or
history. It rejects another SDK session/replacement generation; stale revisions
remain readable for diagnostics. Hosts must attach edit identity/sequence and
capture desired diagnostics before consuming an edit in `commit_to`. Disk bytes
are not pinned. `EditSession::is_aborted` reports ordinary transaction failure;
`estimated_bytes` and `EditReceipt::estimated_bytes` support bounded host metadata.
[`zipzi-runtime`](../zipzi-runtime/README.md) hosts these contracts without adding
a second history or changing standalone SDK ownership.

## Tests

Selected parameter creation uses `ParameterCreateSpec`; UUID allocation and
unspecified `Parameter` defaults stay in Rust. `patch_parameter` preserves
unspecified fields in the current candidate before calling the existing checked
replacement. The focused contract gate is
`cargo test --locked -p zipzi-sdk --test integration parameter_editing::`.

From the workspace root, `./tools/test-sdk.sh fast` runs SDK unit and ordinary CPU
integration tests; add a module filter such as `physics_editing::` for local
iteration. Ordinary suites share `tests/integration.rs`; register new CPU test
modules there. `./tools/test-sdk.sh mesh` runs daily alpha mesh cases and boundary
limits. `regression` includes these daily cases alongside the other workspace
tests; `stress` runs the complete alpha mesh suite and both original random
matrices (24 fragmented masks and 64 small masks). Daily and stress samples
partition each matrix, so full runs check each original mask once.
`./tools/test-sdk.sh full` and plain Cargo workspace tests keep the complete coverage.

```sh
cargo test --locked -p zipzi-sdk --test integration physics_editing::
./tools/test-sdk.sh mesh
./tools/test-sdk.sh stress
```

## Alpha mesh generation

Native `Mesh`, `MeshGeometry` and `GeometrySnapshot` optionally carry `MeshMeshing`:
a canonical counterclockwise UV boundary and sorted internal `User` / `EdgeSupport`
constraints. Core validates the complete UV triangulation when this metadata exists;
ordinary meshes retain their previous render-only rules. Position/pose edits preserve
this structure, and invalid raw UV/topology edits fail atomically. Ordinary replacement
cannot discard it; a complete `TopologyReplacement` explicitly permits `meshing: None`.
Constraint-only edits publish topology notifications, dirty state and history without
refreshing evaluation inputs. Undo/redo and native JSON/CBOR preserve the exact triangles.
External model exports use the actual triangles and do not preserve modeling constraints.

`extract_manual_meshing` validates an existing single-boundary triangulation without
changing its faces. `edit_manual_mesh` is the pure manual edit layer: insert, move,
delete, constrain, release and reconnect return complete `MeshGeometry`, stable ID
mapping and affected edges. Local edits retain unaffected triangle edges; reconnect
globally rebuilds free edges. Pass a fresh ID from a persistent
`ManualVertexIdAllocator` for insertion, keeping the allocator across canceled
candidates so old references cannot name a later point. `AffineUvMap::from_geometry`
recovers the fixed image transform, including reflections. The focused gate is
`cargo test --locked -p zipzi-sdk --test integration manual_mesh::`.

Mesh Edit can insert outside the current outline within the source image, extending
the boundary while keeping old faces. `InsertConnectedVertex` inserts and fixes its
connection atomically. `DeleteEdge` on an outer edge removes its incident face and
orphaned vertices, then recovers the remaining outline; an internal fixed edge becomes
automatic. Operations that leave no faces, holes or disconnected regions fail without
modifying the input.

`alpha_mesh_geometry(AlphaMask { width, height, alpha }, &AlphaMeshOptions)`
is a deterministic, CPU-only geometry operation. `alpha` contains one byte per
pixel, in top-down rows; retained pixels above `alpha_threshold` are included.
It returns `MeshGeometry`, without changing any model.

The seven main controls are outside/inside spacing, outside/inside margin,
minimum margin, minimum boundary points and the alpha threshold. `standard()`,
`deformation_small()` and `deformation_large()` provide modeling presets. Their
outside/inside spacing is respectively 85/85, 160/160 and 40/40 pixels, targeting
roughly half the earlier vertex counts while retaining margins, corner coverage
and thin-feature support. The exact reduction depends on shape and size; these
are not equivalent Cubism settings. Custom spacing keeps its literal meaning.
**All distances are source-image pixels.** There is no implicit 1024-pixel,
canvas, UV-atlas or DPI scaling. `preserve_holes` defaults to false: the outer
domain spans source holes, while interior support follows the artwork instead
of filling large transparent voids with lattice vertices. Holes narrower than
the inside spacing receive no separate support ring. Foreground islands inside
large holes still receive support. Set `preserve_holes: true` to retain holes in
the mesh domain. Positive outside margins can merge nearby components and alter
holes. `remove_faint_islands` defaults to false. When enabled it discards isolated
4-connected components whose peak alpha is at most 64 and total alpha is at most
1020 (four fully opaque pixels), only if the image also has alpha >= 128. This
explicit policy retains opaque dots, connected faint edges, larger translucent
regions and fully translucent source images; it is not a global alpha threshold.

The generator unions foreground pixel squares (merging horizontal/vertical
runs), offsets their polygons, and simplifies only when the required padded
foreground is still covered. Failed shortcuts protect nearby raster corners;
other smooth spans keep their simplification tolerance. Offsets are simplified
before clipping to the image bounds, so a tightly cropped diagonal does not
force dense sampling along the entire boundary. Exterior and retained hole
boundaries become CDT constraints. Smooth convex contours are redistributed by
arc length when preserving every simplified segment would substantially oversample
the curve. Required padding, valid topology and minimum ring sizes are checked;
sharp/concave contours and rings with holes retain their existing corners.
Other segments use subdivision allowing at most 10% spacing slack.
Simplified inner offset rings are also constrained, preventing
outer edges from skipping the ring and connecting directly to deep interior
vertices. Face inclusion uses the actual sampled f32 outline, independently of
the inner constraints, so internal rings do not become holes.

Collapsed or very narrow inset components use chordal-axis support chains
derived from the boundary triangulation. Chains are clipped to foreground away
from surviving cores; short spurs are removed. This is a heuristic for thin
features, not an exact medial axis or a reconstruction of Cubism internals.
A staggered triangular lattice fills the cores with clearance from ring edges.
When holes are spanned, every foreground component gets at least one interior
support vertex, including tiny islands that have no surviving inset.
There is no global angle refinement that could disturb the lattice. Boundary
and interior spacing remain targets; corners and support chains can add points.
There is no guaranteed minimum triangle angle or guarantee of deformation
quality. Pixel alpha is never modified; alpha thresholding is explicit.

Output coordinates use the bottom-left image corner as `(0, 0)`, +Y up; UVs are
`(x / width, y / height)`. `clip_to_image` defaults to true, stopping margins at
image edges and keeping UVs in `[0, 1]`. Set it to false to keep complete margins
outside the source rectangle. Those UVs can leave `[0, 1]`; callers must add
transparent texture padding and remap UVs before rendering. Retain the image
pixels and their pixel-to-model transform to regenerate
after editing; do not derive a new source mask from the previous mesh.
Triangles are CCW and use local sequential vertex IDs. The generator does not
transfer keyforms, BlendShapes or glue: applying its output to existing artwork
requires an explicit `TopologyReplacement` and dependency policy.

Empty masks, invalid options, geometry failures, and exceeded budgets return
structured `SdkError`s (`EMPTY_ALPHA_MASK`, `INVALID_ALPHA_MASK`,
`INVALID_ALPHA_MESH_OPTIONS`, `ALPHA_MESH_GEOMETRY`, `ALPHA_MESH_LIMIT`). No partial
geometry is returned. Defaults cap output at 65536 vertices; preprocessing is
also bounded (16384 per image axis, 64 Mi pixels, 262144 merged rectangles,
1048576 sampling and support checks). Highly fragmented masks may reach those limits.

```sh
cargo test -p zipzi-sdk --test alpha_mesh --locked
```

Tests cover whole-pixel coverage, winding, closed boundaries, UV orientation,
holes, disconnected islands, tiny/diagonal features, thresholding, density,
limits and repeatability.


## Detached animation playback

`MotionEditingPreview` samples one raw clip at a local time on the model's
default baseline. It applies no motion fades, expressions, physics or events;
Pose resolves the sampled Part controls immediately, without replaying Pose
fades. Sampling is deterministic across forward/backward inspection and never
changes authoring assets. Numeric parameter maps and geometry scratch storage
are reused. Result preview continues to use `MotionPreview` and its full SDK
pipeline.

`MotionSnapshot::motion_positions` reports the actual SDK local sample time,
period, loop index and absolute cycle origin. `canonical_motion_seek_time` maps
a local time in the selected loop for canonical replay of a clip scheduled at
zero. Exact cycle seams belong to the preceding SDK loop, so seeking the next
loop's zero uses the next representable preview clock value; asset times are
unchanged. Do not derive the displayed phase with `absolute_time % duration`.

Motion initial keys and segment endpoints use persistent `MotionKey` UUIDs;
Bezier handles still use `MotionPoint`. Runtime `CompiledCurve` strips identities.
`MotionClipCandidate` supports UUID-addressed simultaneous key moves, sampled
insertion (including De Casteljau Bezier splits), batch deletion, interpolation,
independent handles and explicit time scaling. Failed operations retain its last
legal state. Unknown targets are read-only; unknown clip extensions retain the
existing opaque edit protection. Ordinary non-monotone time Beziers support
endpoint value changes but reject splits and endpoint time changes.
Ill-conditioned f32 Bezier splits that fail runtime sample probes return
`MOTION_BEZIER_SPLIT_PRECISION` and retain the original candidate.

For each drag update, rebuild a candidate from the original clip and total
displacement. `AuthoringSnapshot::with_motion_candidate` produces an independent
preview source without changing committed assets, history or authoring preview
values. Its version is the committed base version; hosts must include their own
edit ID/revision in asynchronous acceptance checks. Dropping a candidate cancels
it. Commit once with `EditSession::replace_motion_candidate`, using the original
version in `begin_edit`. UUID-based `move_motion_keys`, `insert_motion_key`,
`delete_motion_keys`, `set_motion_interpolation`, `set_motion_bezier_handle` and
`scale_motion_time` are also available inside normal SDK transactions. No-change
commits preserve history and redo. The older index-based move method remains a
low-level operation that leaves handles unchanged.

```sh
cargo test --locked -p zipzi-sdk --test integration motion_editing::
cargo test --locked -p zipzi-project --test motion_tests
```

`AuthoringSession::motion_preview()` captures an independent default-parameter
runtime. Use `schedule_motion_entry(group, index, time)` for model3 registration
fades, or `schedule_motion(clip_id, time)` for an ungrouped clip's own fades.
Playback never writes authoring preview values or history.

`MotionPreview::advance_to_with_progress(target_time, progress)` advances on the
same absolute 60 Hz grid as canonical `seek`. It reuses completed full steps;
a fractional display sample retains the preceding full state so the next
sample resumes without accumulating partial physics steps. Backward targets and
arbitrary `advance` states use transactional seek. This permits display-rate
sampling (for example 120 Hz) with results identical to canonical seek. One
transient full state is retained outside the seek-checkpoint budget.
Cancellation during incremental advance retains completed full steps, allowing
a newer target to resume; discard cancelled results. In contrast,
`seek_with_progress` preserves the entire prior state on cancellation. Both
validate finite nonnegative targets and enforce the one-million-step limit.

Canonical playback also retains bounded checkpoints once per simulated second.
For live expression changes, `replace_expression_schedule` replaces the complete
ordered `(expression_id, time)` schedule and keeps history before its first change.
It rewinds an affected current frame to the nearest unaffected checkpoint; then
call `advance_to_with_progress` for the target time. This avoids restarting the
entire motion and physics simulation on every expression switch. Checkpoints at
or after the change are discarded, and backward seeks use the updated schedule.
Invalid schedules preserve all state; identical schedules are a no-op.

`MotionSnapshot::model_opacity` is separate from the evaluated drawable frame.
Hosts using the observer's Model color-alpha policy call
`apply_model_opacity(&mut frame, opacity)` once before mask / Offscreen rendering.
This multiplies each drawable opacity and leaves independent Part / Offscreen
factors intact. Scaling a final flattened image produces different overlap
pixels. Framework Model opacity does not set renderer color alpha automatically.

For paused inspection, `MotionPreview::replace_expression_schedule_with_transitions`
accepts `(expression_id, time, ExpressionTransition)` entries. `Immediate` replaces
all currently active expressions at full weight without advancing the clock;
`Fade` preserves authored fade-in/out. The transition is part of the schedule and
cache identity, so seeking and cold replay preserve cuts. Expression values still
feed the normal Physics and Pose stages; the authoring document is unchanged.

## Modeling batches and image memory

`prepare_rgba_image(width, height, rgba)` returns a `PreparedImageContent` lease
without registering an asset. Pass it to `create_prepared_image` or
`replace_prepared_image`; keep the lease until the candidate owns the content.
Unrelated commits cannot collect its pixels, and dropping an unused lease frees
its storage. This replaces the descriptor-only `prepare_rgba_asset` API.

`zipzi_runtime::modeling::ModelingCommand` owns cross-domain adapter dispatch;
SDK exposes the underlying domain methods and contains no transport command enum.
`modeling_records` borrows complete selected records without cloning the model.
`AuthoringSnapshot::sample_modeling` evaluates explicit parameter maps and shares
`diagnose_geometry` with formal sessions. `ModelingIntent`/`ModelingPlan` unify
read-only domain plans and their cost/loss reports for native and hosted callers.
No batch or plan creates a separate history; publication remains `commit_to`.

`MeshBindingCreateSpec`/`MeshKeyformSpec` accept numeric axes, positions and
optional appearance. SDK/core allocate canonical axis/sample identities and
validate complete grids through the existing binding methods. The selected
`MeshBindingOperation` composes with the same candidate and preserves identities
when changing one keyform. Focused tests:
`cargo test --locked -p zipzi-sdk --test integration mesh_binding_editing::`.

Project's `ImageMemoryBudget` accounts shared RGBA/PNG buffers, temporary
preparation reservations and blobs. `PreparedImageContent::from_rgba` accepts
shared pixels; `from_rgba_bytes` reserves conversion space before copying.
Candidates retain prepared/new and existing images; snapshots include uncommitted
prepared images. `collect_image_resources` retains formal/history roots and
independent leases while reclaiming unused registry entries. Commit collects
after history/redo eviction; standalone callers can also collect after dropping
an edit or snapshot. `set_image_memory_budget` admits retained images atomically,
including owned candidates crossing into a host. Pixel Arcs held by textures
continue to count until their last owner drops.

[R4 evidence and remaining work](../../docs/plans/sdk-runtime/r4-modeling.md)
describe the current connected slice and limits.
