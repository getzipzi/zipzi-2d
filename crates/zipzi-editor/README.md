# zipzi-editor

The Zipzi 2D desktop editor provides modeling and animation workspaces and hosts
Runtime sessions for the [Python SDK](../zipzi-python2/README.md). It is built
with GPUI Kit and uses [`zipzi-editor-core`](../zipzi-editor-core/README.md) for
editing state and [`zipzi-editor-render`](../zipzi-editor-render/README.md) for
rendering.

## Develop and test

Run commands from the repository root. The workspace requires Rust 1.95 or
newer, uv 0.10.7 or newer, and CPython 3.14. See the
[root README](../../README.md#development) for workspace setup.

- **macOS:** Xcode Command Line Tools and a Metal-capable GPU are required.
  Model previews use Metal by default; wgpu is available as an optional backend.
- **Windows:** The development editor requires MSVC C++ Build Tools, the Windows
  SDK and a usable GPU. It uses wgpu and shared GPU textures for previews.
  Desktop validation is still in progress.

Start the editor with Cargo on either platform:

```sh
cargo run --locked -p zipzi-editor
```

For a development build and process management, use the cross-platform launcher:

```sh
uv run --locked python tools/dev.py editor run
uv run --locked python tools/dev.py editor watch
uv run --locked python tools/dev.py editor reload
uv run --locked python tools/dev.py editor stop
```

`editor watch` uses `watchexec` native file notifications on macOS and Windows
to rebuild and relaunch on source changes. Install it with
`cargo install --locked watchexec-cli` (or `brew install watchexec` on macOS).
Edits during a build queue another reload; a failed build preserves the running
Editor. Closing the Editor also stops the watcher. `./tools/dev-editor.sh` is
available as a shell wrapper. The launcher keeps build and process files under
the checkout's `target/` directory by default. On Windows, reload and stop
request a safe exit and preserve the running editor if it has unsaved documents
or connected clients.

Choose a focused test for the change:

```sh
# Editor core logic
./tools/test-editor.sh fast adaptive_mesh::

# Editor UI logic
./tools/test-editor.sh ui mesh_generation::

# Python client connected to an installed editor host
./tools/test-sdk.sh python2-connected --editor-host /absolute/path/to/zipzi-editor
```

On Windows, use `uv run --locked python tools/dev.py test editor` or
`uv run --locked python tools/dev.py test sdk` with the same modes and filters. Editor shell gates clear
`ZIPZI_DEV_SESSION_DIR`; unset it explicitly for direct Editor Cargo tests so
they do not attach to a live development session. Native Metal acceptance and
screenshot tests require macOS and Metal; Windows UI logic tests are available.
See [test coverage and prerequisites](../../tools/README.md#test-coverage) and
`./tools/test-editor.sh --help` for the available gates. Full and acceptance
runs are more expensive than focused tests.

## Resident application host

Start the editor without a window using:

```sh
cargo run --locked -p zipzi-editor -- --no-window
```

The Python SDK can connect to the resident host and share its Runtime sessions,
documents and history. When connected clients, pending jobs or an explicit
`--no-window` launch keep the host resident, closing a window detaches its view;
reopening a window attaches the resident sessions. Closing a document tab also
detaches its view. Otherwise, closing the last window exits after any unsaved
documents are resolved. Use Quit to explicitly exit. See the
[Python SDK guide](../zipzi-python2/client/python/zipzi/docs/guide.md) for
connection and session usage.

## Local model acceptance

Some acceptance tests use separately obtained model assets under
[`models/local/`](../../models/README.md). Missing optional assets are reported
as skips. Set `ZIPZI_ANIMATION_REQUIRE_ASSET=1` or
`ZIPZI_TIMELINE_REQUIRE_ASSET=1` to require them.

The opt-in format round-trip test can be run on macOS with the local Mao asset:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test acceptance transfer_roundtrip::mao_repeated -- --ignored --nocapture
```

## Native screenshots

On macOS, run a screenshot scene against the production application library:

```sh
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=timeline cargo test --locked -p zipzi-editor --features test-support --test headless_screenshot
```

Omit `ZIPZI_SCREENSHOT_SCENE` for the default set. Available scenes are defined
in [`acceptance/screenshots.rs`](acceptance/screenshots.rs). Screenshots go to
`target/headless-screenshots/` by default; `ZIPZI_SCREENSHOT_DIR` overrides the
output directory.

The `warp` scene captures inline subdivision inputs, Warp point editing and
Mesh transform frames in English, Simplified Chinese and Japanese. Set
`ZIPZI_SCREENSHOT_SAVE=1` to save every image from that scene.
The `warp-authoring` scene compares all four Bezier policies on a textured grid
and captures direct controller-density edits in those three languages.
The `warp-keep-children` scene exercises native Command dragging in all three
languages, checks that every textured child vertex stays fixed, and verifies that
the toolbar button and Properties checkbox are absent.
The `deformer-original` scene exercises recording, density changes, restore review,
Cancel and Apply, checks the restored Warp lattice and captures Warp/Rotation
record states in all three languages.

## Deformer creation

New Warp and New Rotation wrap the current selection in one undo step. Toolbar,
menu and shortcut creation share this behavior; row menus prioritize a nonempty
selection over the clicked row. Selected Parts include their organization
descendants. Selected parents retain their children, and organization ownership
and drawing order stay unchanged. Base shapes, parameter keyforms and BlendShape
deltas are converted together to preserve the evaluated result. Without a
deformable selection, creation produces an empty deformer.

Selected deformation roots must share a parent. Inserting a Rotation below a Warp
requires a static axis-aligned affine Warp space; other Warp spaces reject the
whole operation. New Warp supports animated, nonlinear parent Warp lattices.

Mesh moves between parents in the Deformer tree preserve base positions,
ordinary parameter keyforms and BlendShape displacements. Static rectangular
Warp lattices and Rotation chains support this conversion, including nested
Warp/Rotation parents. Unsupported nonlinear, driven or singular parent spaces
reject the whole move instead of applying model coordinates as Warp coordinates.
Sibling reordering does not convert geometry. Hover only checks support; release
builds the numeric conversion and commits it with the hierarchy in one undo step.
Whole Rotation subtrees can change parent while keeping child geometry in place.
The Rotation's origin, reference angle and inherited scale are compensated,
including ordinary keyforms and BlendShape translations/scales; child Mesh and
nested deformer data remain intact. Angle channels stay unchanged, including
BlendShape angle clamping. Mixed selections normalize selected descendants and
commit in one undo step. Warp subtree roots, singular parents and conversions
that would change BlendShape scale clamping remain unsupported. Ordinary Rotation subtree dragging
captures the current modeling pose, including driven Rotation ancestors, then
inherits the new parent's subsequent motion. A plan expires if preview inputs
change before publication. The explicit preservation review retains its
full-domain option, which requires fixed ancestor frames over that domain.
Reordering deformers under the same parent remains available.

## Deformer properties

Select one Warp or Rotation to edit its local appearance and organization in
Properties. Parent choices use the same shape-preserving move plan as the
Deformer tree; unsupported conversions explain their refusal. Rotation origin
inputs use parent coordinates and retain f64 precision. Angles are in degrees,
scale is a percentage, and the reference angle edits the definition separately
from the current pose. Stored ordinary keyforms receive pose/appearance changes;
interpolated poses and contributing Deformer BlendShapes require returning to an
editable stored/base shape. Multiple selections currently request a single target.

Mesh and Deformer use the same basic field configuration and flat property-row
layout: name, runtime ID, Part, parent deformer, opacity, multiply/screen colors
and enabled state. Mesh-specific fields are inserted in their existing positions.
The shared color controls include white/black reset buttons. Deformer keeps its
own parent-move validation, shape destination and extended opacity range.

Warp subdivisions or Rotation pose follow those basic fields. Grid and Bezier
density for a new Warp defaults to 5 × 5 grid cells (36 points) and 2 × 2 Bezier
cells (9 anchors). Nested grouping preserves a parent lattice when needed to keep
the existing deformation. Density can be edited directly with horizontal × vertical inputs: Enter
or blur commits, Escape cancels, and dragging a number previews one undoable
edit. Grid density resamples all related shapes; Bezier density refits stored
controllers while preserving grid positions. Interpolation has no editor choice;
the stored runtime mode remains available to importers/SDK code. Bezier controls
and original-shape recovery remain collapsible.

Mesh, Warp, Part and Offscreen selections have canvas transform frames.
Rotation uses its native origin, axis, ring and scale controls; directly selected
Rotations are excluded from generic frames, including mixed selections.
Containers transform their drawable contents; selected deformation descendants
are normalized so a parent and child are not moved twice. A point selection
restricts the frame to those points. Drag frame edges to translate, square handles
to scale and the round handle to rotate. A single selected grid point, Bezier
anchor/tangent or mesh vertex is dragged directly and shows no transform frame;
multi-point selections keep their shared frame. Shift constrains translation, preserves
corner scaling proportions, and snaps rotation to 15-degree increments.
Alt/Option scales around the frame center. Shift+Alt/Option enables fine steps:
translation in one canvas-unit increments, rotation in one-degree increments,
and scale ratios in one-percentage-point increments relative to the press shape.
Fine translation retains axis locking; fine corner scaling is uniform and
centered. Rotation center dragging supports the same fine translation chord;
its axis and ring share continuous rotation across full turns. Rotation scaling
is always uniform. Modifiers are frozen on mouse-down; changing keys during a
drag does not change the active constraints. Snapping leaves the frozen pivot
and parent-space conversion intact.
Container transforms involving Rotation children scale uniformly; incompatible
parent spaces reject unrepresentable transforms.
Escape cancels a gesture; commit creates one undo step. Locked objects and
noneditable parameter poses retain their normal editing restrictions.

Arrow keys in a focused Modeling canvas move the current selection by one canvas
unit, Shift+arrows by ten and Alt/Option+arrows by 0.1. Text inputs, hierarchy
navigation, timelines, modal dialogs and topology-edit mode retain their keys.
Each key event commits one undo step and refreshes all affected viewports.

Warp shows division points and Bezier controls together, without a display-mode
switch. Thin gray lattice lines and small gray points sit below muted green
Bezier curves, larger green-bordered anchors and solid green tangent handles.
Coincident points pick the Bezier control first. Region selection can include
both types; mixed transforms move explicitly selected grid points once while
applying controller influence to the remaining grid. The toolbar provides ancestor
visibility, the dense reference grid and gesture help. Numeric transform dialogs and frame-configuration
menus have been removed. Frames follow their current targets automatically.

Fine-point edits retain the controller. By default, later Bezier edits add the
controller's change to the grid, retaining hand-edited detail. **Reset controller**
rebuilds controls without moving grid points; **Reform from handles** replaces
the current editable shape's grid with controller samples. **Auto reform on
edits** does that on subsequent Bezier edits; changing the toggle alone leaves
geometry unchanged. These settings and actions use ordinary undo/redo and native
project persistence.

Bezier influence offers selected controls, local cosine smoothing (two controller
cells), global Gaussian smoothing, and preservation of opposing tangent length
ratios. These are Zipzi policies, without a claim of Cubism equivalence. Smoothing
can affect controls outside the selection; the viewport status names the active
policy. Whole-Warp transforms keep their affine behavior and grid detail.

Holding **Command on macOS / Ctrl on Windows** at the start of a Warp drag keeps
children fixed for that gesture. There is no persistent mode, toolbar button or
Properties checkbox; subsequent ordinary drags deform the children normally.
The shared plan changes the Warp and compensates child base shapes, ordinary
keyforms and BlendShape deltas
in one undo step, including nested nonlinear Warp subtrees and Rotation children.
Cancel or dragging back to the start restores the captured data exactly.

The edited Warp must have reversible affine old and new maps and static ancestor
spaces. Current support admits rectangular Warp ancestors and Rotation chains;
driven or nonlinear edited/ancestor maps, density changes, locked dependencies
and conversions with measured preservation error above 0.01 canvas units reject
atomically. Validation checks actual float32 mapping and child-position errors;
absolute coordinate magnitudes do not consume the tolerance. Non-finite outputs
are rejected.
Local control edits that make the map nonlinear are rejected. Independent
interior/exterior samples and parameter endpoints/key midpoints supplement the
constant affine channel conversion; their measured maximum is not a global
error bound. There is no arbitrary nonlinear keep-children conversion yet.

**Original shape** shows the recorded source and provides **Update original**,
**Restore original** and automatic first-key updating for Warp and Rotation.
New deformers record their canvas shape at creation. The first ordinary keyform
can replace it once; deleting and recreating a binding does not repeat this.
Manual update disables automatic updating. Imported objects and older projects
without a record show that state and require an explicit update.

Update and Restore use the current editable base or ordinary keyform and the
frozen preview parent pose. Restore maps the saved canvas shape through the
current parent, retaining that parent's pose and the current Rotation reference
angle. A review dialog precedes shared conversion-grid/interpolation changes or
stored-controller density refits. Cancel changes nothing; Apply publishes one
undo step. Missing controllers remain derived using the current authoring settings.

Restore requires an invertible affine current parent space. Missing records,
type mismatches, active deformer BlendShapes, nonlinear/singular current parents,
locked dependencies and tolerance failures reject the operation atomically.
A controller captured under a nonlinear parent cannot be restored as equivalent
cubic curves; a baked Warp lattice without a stored controller can be restored.
Original records survive native save/reopen, copying and undo; CMO3 reports their
loss. These actions restore a shape rather than rolling back the document.

## Object deletion

In Modeling, **Delete Object** is shared by the Edit menu, the Part and Deformer
row menus, the viewport context menu, and the configurable `edit.delete-object`
shortcut (Delete and Backspace by default). Shortcuts use keyboard focus before
pointer hover and leave text editing and timeline deletion to their own controls.
In Mesh Edit, the same keys delete selected geometry instead of the mesh object.
Right-clicking a selected row uses the current selection; another row targets
only that object. Part tree deletion includes organization descendants, while
Deformer tree and viewport deletion include deformation descendants. Owned
bindings are removed with their targets. Every object deletion requires a review of
the frozen deletion list and explicit confirmation, with Cancel focused initially.
Completing or recalculating the review never deletes objects automatically.
A single-object confirmation names the object directly; larger deletions list the
objects and why they are included. Required choices show the reason before the
replacement controls. Reference updates, IDs, attached records and conversion
reports are behind **View details**. Choose whether to keep a
group's contents with explicit scope controls. Choosing replacements or changing
the scope updates the preview automatically; **Recheck impact** is available in
technical details. Replacement actions stay disabled until a target is selected.
Each deletion is one undo step.
Locked objects (including inherited locks and locked descendants) and references
from outside the deletion set block the operation. Menu labels stay short; clicking
an unavailable action explains the reason in the status bar without changing the
document. This feedback is shared by creation and other object context menus.
Changing the document or workspace expires an open deletion menu or review.

## Mesh Edit

Select one or more meshes before entering Mesh Edit. Box and lasso selection can
span them; selected points move and delete together in one undo step, while each
mesh keeps its own topology. A rejected operation retains the last valid result
for the whole group. Separate Box and Lasso buttons in the secondary toolbar choose
the selection shape; point picking, dragging, creation and deletion work in both
states. Clicking a mesh makes it current for point creation,
generation and source-image padding; the secondary toolbar names that mesh.
Source-image view is available when editing a single mesh.

Mesh Edit retains the complete model and its current camera. The selected mesh's
handles follow the evaluated pose, including ordinary parameters, BlendShapes,
parent deformers and Glue. Picking maps the displayed triangles back to UVs;
drags freeze the source pose, invert Glue and the parent deformation, and resample
all stored shapes through the resulting UVs. No authored keyform needs to be selected.

The image button switches to **Edit on source image** for an optional pixel-space
UV view. Both views allow topology editing, retain selection and use separate cameras.
Switching settles the active drag. Folded or singular poses explain an ambiguous
inverse and retain the last valid candidate; the source view remains available.
Other Modeling viewports retain their model view and cannot acquire this edit gesture.

New and moved points resample every stored shape from the operation's frozen UV
source. Glue endpoint markers remain visible; deleting a referenced endpoint
requires releasing its pair first. Outside-domain operations explain their refusal
without closing the mode. Interior generation retains identities and Glue; edge
and full rebuilds require that no old vertex is referenced. A generation preview
is applied explicitly; saving only settles an active point drag.

The secondary toolbar's **Expand source image** adds 32 transparent pixels on
each side per click. It remaps UVs without moving the artwork or resampling
stored shapes; image and UV changes share one undo step. The extra space allows
an affine mesh's outline to extend beyond the original image. Existing
deformation sampling domains still apply to meshes with dependencies.

## Mesh generation controls

The editor's adaptive mesh controls use source pixels. Initial spacing and
scrub increments scale with the longer texture dimension. For a 2048 × 1024
texture, edge spacing starts at 500 px, interior spacing at 130 px, outer margin
at 6 px and edge width at 16 px. The single-row outer margin starts at 6 px for
every texture size. The defaults and scaling rules are defined in
[`mesh_generation_controls.rs`](src/ui/mesh_generation_controls.rs); see the
[adaptive mesh reference](../zipzi-editor-core/src/adaptive_mesh/README.md) for
the generator and its limits.

## Distribution

`./tools/build-editor-app.sh` builds a macOS app bundle. Windows portable
archives are built with `tools/packaging/package_windows.py`. See
[distribution commands and verification](../../tools/README.md#local-distribution-artifacts)
and the [release guide](../../docs/RELEASING.md) for artifacts and prerequisites.

## Source map

- `src/main.rs`, `src/lib.rs`: application entry point and library.
- `src/workspace/`, `src/panels/`, `src/properties/`: workspaces and editing UI.
- `src/viewports/`, `src/preview.rs`, `src/presentation.rs`: canvas and previews.
- `src/input_routing/`, `src/commands.rs`, `src/keybindings.rs`: input and commands.
- `src/host.rs`, `src/ipc.rs`, `src/worker.rs`: resident host, IPC and background work.
- `assets/`: icons and localization resources.
- `headless_tests/`, `tests/`, `acceptance/`: UI logic and native acceptance tests.

The `debug-tools` feature enables GPUI FPS tools; `bench-instrumentation` enables
editing-chain instrumentation. See the [UI performance reference](../../docs/references/ui-performance.md)
and [benchmarks](../../tools/README.md#benchmarks-and-research-tools) when
working on performance.

The editor is licensed **GPL-3.0-only** and is not published to crates.io.
Distribution requires its assets, workspace patches and
[third-party notices](../../LICENSING.md).
