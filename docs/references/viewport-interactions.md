# Viewport tools and gestures

Tool state belongs to the document's Modeling context. A viewport provides the
displayed frame, projection and input owner; multiple viewports share object and
component selection but keep navigation and presentation independent.

Each document/viewport fits its content once, when first presented with visible
metrics. Later parameter previews, geometry/visibility changes, animation frames
and resizes preserve its zoom and pan. The explicit Fit command fits the current
content again; an unmeasured or hidden viewport keeps its initial fit pending.

## Where behavior lives

[`zipzi-editor-core/src/tools/`](../../crates/zipzi-editor-core/src/tools/) groups interaction
behavior by operation:

- `select.rs`: shared hit/selection preparation, deferred object clicks,
  rectangle completion and its overlay and capture hints.
- `lasso.rs`: freehand sampling, release rules, overlay and idle/capture hints.
- `controls.rs`: control-point/gizmo candidate preparation, transform math,
  commit/cancel, feedback and intermediate-frame acceptance.
- `transform_constraints.rs`: shared live modifier constraints for object frames
  and native deformer controls, including translation axes and transform steps.
- `hand.rs` and `glue.rs`: tool guidance; Glue shares selection gestures, while
  Hand uses viewport navigation.
- `gesture.rs`: the explicit Select/Lasso/Controls variants, common press
  projection and dispatch. Tool selection-family and idle Escape policy are
  declared here alongside tool dispatch.
- `region.rs`: shared click/region selection rules and rectangle/polygon geometry.
- `input.rs`: pointer event dispatch and whether the model changed.
- `overlay.rs`: projection of shared scene controls; delegates gesture feedback
  to the active variant.

The press snapshot holds the original scene, transform, selection and initial
modifiers. Selection and keep-children policies use that snapshot; transform
constraints track live Shift/Alt state.
Only a control gesture can own a candidate edit. An idle control press creates no
transaction; crossing the existing drag threshold prepares it. Selection gestures
never own model transactions.

Rotation rings start with a 64-model-unit radius, independently of descendant
geometry. Own and inherited Rotation scale and viewport zoom apply to that radius,
with a minimum of 24 logical pixels. Axis tips share that radius; scale handles
sit 24 logical pixels outside the ring. Scaling uses the frozen press ring size;
handle padding and the grab offset do not enter the authored ratio. Controls use
each viewport's presented origin, direction and pose scale, keeping completed
intermediate frames aligned even when a newer preview has already been requested.
Controls keep a plain appearance on hover and during
rotation; the status bar retains the continuous angle delta. Scale-handle spokes do not
rotate the object. Rotation and object frames apply the same translation-axis,
angle-step and fine translation/scale policy once, after pointer interpretation.
Pressing or releasing Shift/Alt updates the current preview even without pointer
motion. Pointer moves and releases also carry authoritative modifier state.
Constraints always recompute from the press geometry; toggling them does not
accumulate transforms or split the gesture's history entry. Shift chooses the
dominant translation axis when activated and holds it until release; Alt changes
the scale pivot immediately.

Select and Lasso share click selection, Shift addition/toggling and Option
subtraction (Option takes precedence over Shift). Region captures use the same
operation from the press snapshot; only their region geometry differs. Disabled
vertices use the same edit-access checks as their overlay styling and are excluded
from click and region selection. They remain hoverable to explain the restriction,
and Option can remove a previously selected point that has since become disabled.

For editable Warp points, Shift-click toggling is deferred until release. A
Shift drag keeps the pressed point selected and locks the dominant canvas axis
when the drag starts or Shift is pressed; later motion cannot change that axis
while Shift remains held. Releasing Shift unlocks it, and pressing it again
chooses a new axis.
This applies in both Bezier and lattice-point modes. Option on a Warp scale
handle instead scales around the frozen frame center, including uniform corner
scaling with Shift. Option point/region subtraction remains unchanged.

Warp frames have a separate rotation handle. Rotation accumulates successive
angle deltas across the ±180° boundary; returning through full turns leaves no
history entry. A point selection shares one frame across independent Warps.
Selected anchors carry their own Bezier handles; a handle-only selection changes
only those handles, and lattice selection changes only the selected grid points.
The Whole Warp toggle retains component selection while using the owning Warp's
whole frame. Picking and drawing use the same frame geometry. Pointer presses
freeze the component target and frame for the entire gesture.

Properties' Transform Warp dialog uses this same target and canvas-space math.
Translation, angle and percentage scale are relative increments around the frame
center and reset on reopening. Apply publishes one atomic undo step; changing the
document, editing context or component target expires the dialog. Ancestor and
descendant Warps cannot be transformed together.

The Warp frame menu is shared by Properties and each Modeling viewport toolbar.
Show frame affects painting and picking together; hidden frames leave control
points available. Adjust frame only uses the frame's ordinary move, rotate and
scale gestures without acquiring a model candidate. Points are unavailable in
this mode. Escape and capture loss restore the starting frame. The frame's center
cross and rotation arm show its pivot and orientation, and scale cursors follow
its local axes. Side-handle spans use the local frame dimensions directly.

Frame state belongs to the document, viewport, object and component target.
Navigation preserves canvas-space frame coordinates; target changes and geometry
or parameter publications retire prior overrides. Align to Warp direction sums
the lattice's horizontal canvas vectors and fits the target along that direction;
an ambiguous direction rejects without changing the frame. Reset fits a fresh
axis-aligned frame. These commands add no model history. Numeric Warp transforms
use the owning viewport's adjusted center.

Switching Bezier/grid display never changes geometry or history. Fine-point
edits retain any stored controller; absent controllers stay derived until an
actual controller edit. Handle drags preserve the explicit conversion and
controller densities. Derived controllers default to 1×1 independently of the
conversion lattice.

Bezier edits freeze the object's influence policy with the edit snapshot.
Selected controls stay explicit; local cosine or global Gaussian smoothing can
move neighboring controls, and tangent-ratio mode couples opposing handles.
The status label identifies these extra influences. With auto reform off, the
controller delta is added to the grid, retaining its residual detail. With it
on, actual controller edits resample the grid. Whole affine transforms retain
the transformed residual. Changing the auto-reform toggle alone and submitting
an unchanged control never reform geometry. Reset controller preserves grid
points; the separate Reform action deliberately replaces their detail.

Keep children fixed is transient Modeling tool state. Warp also shares it with
Properties' numeric transforms. The primary modifier (Command on macOS, Ctrl on
Windows) enables it for Warp and Rotation pointer gestures: center/edge translation,
rotation, and scale handles use the same coordinate compensation. The press freezes
this keep-children policy; Shift/Alt transform constraints remain live. Frame-only
adjustment continues to affect only the frame. Unsupported maps reject the gesture
and cancel its whole candidate, without falling back to an ordinary drag.
Rotation keep-children rings follow the deformer scale while the artwork stays fixed.

Each keep-children preview restores captured child channels before applying the
shared `WarpKeepChildren`/`RotationKeepChildren` conversion. This avoids accumulated
inverses and ensures a return to the starting pointer position restores absent
metadata as well as geometry. It requires static, reversible affine old/new Warp
or Rotation maps and supported static ancestor spaces. All base/ordinary/BlendShape child channels are
converted together; Rotation inheritance uses the shared reparent math. Nonlinear
edited maps and animated ancestor spaces remain explicit refusals. Publication
checks a 0.01 canvas-unit numerical budget, independent of zoom or device pixels.

Original-shape Update and Restore settle the current edit before freezing their
target. A target includes the Runtime session, full version, preview revision,
property scope and object selection; pending candidates cannot supply one.
Changes to any of those retire the target, including undo/redo. Parent mapping
uses the frozen preview inputs. Restore changes the current base/ordinary shape
through the current parent inverse and retains other parameter shapes and the
parent pose. Shared grid/interpolation changes or controller-density refits need
an explicit review dialog. Cancel makes no edit; Apply is one atomic history
step. A successful Restore clears component indices and frame overrides so a
changed shared topology cannot leave stale control selections.

The viewport's Select All command (`Cmd+A` on macOS) shares these region selection
rules: it selects editable vertices of selected meshes/warps, or visible unlocked
meshes when there are no component targets. It respects viewport isolation and
requires a current displayed frame. Text inputs retain their own Select All action.

## Lifecycle

[`viewport/interaction.rs`](../../crates/zipzi-editor-core/src/viewport/interaction.rs)
owns interaction acquisition and
release for both UI and synchronous core clients:

1. `begin_canvas_gesture(binding, point, shift, alt, primary)` checks the displayed frame before taking
   ownership. A rejected stale press leaves a running drag intact.
2. The result supplies the current capture token. Captured UI events use
   `update_canvas_gesture(token, input)` to reject old serials and repeated presses.
3. Pointer Up completes the gesture and releases the token in the same call.
   Cancel, lost button and operation errors roll back and release it.
4. Releasing an unfinished Modeling capture with `cancel_viewport_interaction`
   also rolls it back. It cannot leave a candidate without an owner.
5. Visibility, resize, document/context changes and focus loss use the same core
   cancellation path. Delayed cleanup of an old token cannot affect its successor.
6. Core commands release the Modeling token when they end its gesture, including
   commands that fail after cancellation. Preflight rejection preserves a running
   gesture and its token; navigation captures remain independent.

`ViewportInputUpdate` reports only `model_changed` and the surviving capture.
Selection-only clicks do not evaluate the model or open an edit. `hover_canvas`
handles uncaptured move/leave and cannot disturb a gesture in another viewport.
`begin_viewport_navigation` acquires a navigation token; Modeling acquisition
always begins the gesture in the same call. Device-specific pan positions, wheel interpolation and OS
capture stay in `viewports/`; they do not own model transaction completion.

## Status bar hints

Tools and gestures declare static semantic `InteractionHint` slices next to their
operation. Timeline declarations and published state live in
`panels/timeline/hints.rs`. A Timeline-owned `TimelineHints` handle lets the dock
and status bar read a snapshot while the Timeline entity is borrowed. Capture
snapshots check the original core operation before producing hints; cancellation
and entity release clear them. `InputRouter` stores no Timeline hint state. These
declarations carry actions, modifiers and command identities, with no GPUI or
translation dependency in core.

StatusBar chooses active viewport capture, active Timeline capture, then idle tool
guidance. It reads viewport capture directly from core, resolves configured command
bindings, translates labels and formats keys. It has no tool-specific capture
variants or operation rules. The existing message-only visual presentation remains;
this refactor does not enable visible shortcut hints.

## Window input routing

`input_routing.rs` coordinates three steps without panel-specific branches:

1. `input_routing/facts.rs` reads rendered hitboxes, focus and blocking UI facts,
   event flags and surface-provided targets, keeping only the first valid capture
   snapshot in surface order.
2. `input_routing/policy.rs` resolves an immutable snapshot into Ignore,
   CancelCapture, Consume or an Invoke with a command ID and local target. It
   classifies keyboard ownership and applies capture/hover/focus priority,
   Escape policy, command scopes, binding order and repeat policy. Only key
   matching uses a host adapter.
3. The window executes that decision: cancel with the captured serial, consume
   a suppressed repeat, or construct and invoke the command once. A disabled
   local command remains matched and consumed.

Each `InputRegion` declares its target resolver. Viewport input declares its
binding and idle Escape action; Timeline declares its own panel binding;
Properties declares whether it currently targets a Timeline inspector or an
ordinary panel. Dock and panel lifecycle modules enumerate visible surfaces;
lifecycle cancellation also enumerates hidden surfaces. Target resolution does
not affect keyboard focus.

Pointer observation remains separate from keyboard policy. It records the native
menu target from the current rendered regions and deliberately retains that
target when the pointer enters menu chrome. Shortcut routing uses current capture,
hover or focus facts and never falls back to the saved menu target.

Capture cleanup still has two phases for GPUI borrowing: the local entity compares
the captured operation with its current operation and releases local state, then
returns core cleanup for the window to apply. Delayed cleanup cannot release a
replacement capture.

## Verification

Run focused checks from the repository root:

```sh
./tools/test-editor.sh fast viewport_targets::
./tools/test-editor.sh fast viewport_isolation::
# Native/headless UI libraries require macOS and Metal:
./tools/test-editor.sh ui headless_tests::input_routing
./tools/test-editor.sh ui input_routing::policy::
```

The shell entrypoint clears `ZIPZI_DEV_SESSION_DIR`. See
[test coverage](../../tools/README.md#test-coverage) for broader gates.


Core integration cases cover current-frame rejection, delayed frames, original
write targets, one-step history, cancel/rollback and independent viewport owners.
`viewport_targets.rs` also exercises automatic token release, abandoned captures
and selection changes without model evaluation. Headless canvas and input-routing
tests cover native pointer delivery, Escape, focus loss, tool switches and
configured-key hint resolution. Routing policy tests cover focus/IME ownership,
capture/hover/focus priority, scope filtering, Escape and repeat decisions without
a GPUI window. Input-routing headless regressions also cover Properties target
rebinding and stale/window-local Timeline hint snapshots.

## Mesh Edit direct gestures

Mesh Edit accepts one or more selected meshes, each with its own source image,
vertex selection, allocator and topology. Region selection spans the group;
vertex identities include their owning mesh. A shared drag freezes every source
pose and solves selected Glue endpoints together. Move and delete batches validate
every mesh before changing the candidate, retain the last valid batch on rejection,
and commit one history step. Cancel, lost button, viewport changes and external
publications discard the entire unfinished gesture.

Point creation and connections belong to the mesh under the press; connections
cannot join meshes. The toolbar shows only the current mesh's name for generation
and padding; hovering reveals its full name.
Right-clicking another mesh makes it current without changing point selection.
Source-image view is limited to single-mesh mode.

Generate Mesh lives in the top secondary toolbar while Mesh Edit is active.
Switching to any primary tool exits Mesh Edit and retains
completed edits and their undo history. The secondary toolbar has no exit or help
button. Selecting objects, changing properties, creating parameters and switching
to Select/Hand/Lasso/Glue exit the mode before
performing the requested operation. They preserve completed mesh edits, cancel any
unfinished mesh gesture and retire its capture; no second click is required.

Mode lifetime is independent of ordinary tool cleanup and document source versions.
A shared command boundary preserves only mesh operations, view controls and history;
other commands and authoring publications use the explicit exit lifecycle. Inspector
preview exits before acquiring its own candidate. External changes still revalidate
the stable Mesh ID; an unavailable target suspends input until recovery or exit.

Mesh Edit uses one default interaction: click empty space or an edge to insert a
point, drag an existing point to move the selection, and drag empty space to box
select. Delete/Backspace removes selected points; right-click opens the normal
context menu. Shift adds and Option subtracts selection; Shift-click selects an
edge for edge commands. A click is distinguished from a marquee at release using
the drag threshold, so box selection never inserts a point.

Command-drag from a point connects it to another point or inserts and connects at
the release position. Command-click on a dashed automatic edge makes it a fixed
User constraint without adding vertices. Command-click on empty space or an
already-fixed edge does nothing; plain click remains the insertion gesture.
Command+Shift-click deletion remains supported. Press modifiers fix the operation
until release or cancellation. Modifier-only changes immediately switch to a pointing-hand cursor on connectable
vertices and automatic edges, or a prohibited cursor on empty space and fixed edges;
an active connection keeps its cursor after Command is released. Instructions
appear only in the multiline Mesh Edit tooltip, without a persistent status-bar
hint or viewport hint overlay. Target vertices highlight and the preview snaps to their positions. Each
completed edit creates one undo step; Escape, lost button and focus loss cancel the current gesture. The secondary
toolbar contains generation, Retriangulate Mesh and result preview. Retriangulate
Mesh operates on the current mesh and is also available in the Mesh Edit context
menu. Right-clicking an automatic edge offers Constrain Edge without needing Command.
Right-clicking a constrained edge instead offers
Release Edge Constraint, which preserves both endpoints and the current triangles
and creates one undo step. Automatic edges and outline edges cannot be released.
There are no separate Create/Delete tools. The Mesh Edit entry tooltip explains
the default interaction in both locales.

The secondary toolbar has separate Box and Lasso buttons with mutually exclusive
selection states. Clicking the active button retains its mode. Only a drag
beginning in empty space changes between a rectangle and a freehand polygon.
Both shapes share point picking, vertex dragging, point
creation, connections and deletion. Shift adds selection and Option subtracts it.
Region selection is applied on release; Escape, lost button and switching the shape
cancel an unfinished region without changing selection or history. Degenerate
lasso paths leave selection unchanged. The same interaction works across meshes
and in model and source-image views.

Edit on source image is the final button in the Mesh Edit secondary toolbar.
Expand source image adds 4 px of transparent padding on each side per click.
The image and UV remap publish atomically in one undo step, keeping original
texels, model-space positions, stored shapes and Glue unchanged. Repeated clicks
add more room; the source-image camera refits to the expanded image. Padding
is unavailable during an active gesture. The enlarged texture admits outline
edits beyond the old image for affine meshes; existing deformation sampling
domains remain enforced for dependent meshes.

Moving and deleting vertices retriangulate free edges instead of treating the old
triangles as motion barriers. Generated support constraints are released by those
edits; explicit User constraints remain fixed. The outline locally encloses
surviving points after a corner moves or is deleted. Boundary and fixed-edge
crossings, coincident points, image limits and degenerate final meshes still reject
atomically and preserve the last valid preview.
