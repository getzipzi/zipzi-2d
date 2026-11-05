# zipzi-editor

The Zipzi 2D desktop editor provides modeling and animation workspaces and hosts
Runtime sessions for the [Python SDK](../zipzi-python/README.md). It is built
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
./tools/test-sdk.sh python-connected --editor-host /absolute/path/to/zipzi-editor
```

On Windows, use `uv run --locked python tools/dev.py test editor` or
`uv run --locked python tools/dev.py test sdk` with the same modes and filters. Editor shell gates clear
`ZIPZI_DEV_SESSION_DIR`; unset it explicitly for direct Editor Cargo tests so
they do not attach to a live development session. Native Metal acceptance and
screenshot tests require macOS and Metal; Windows UI logic tests are available.
See [test coverage and prerequisites](../../tools/README.md#test-coverage) and
`./tools/test-editor.sh --help` for the available gates. Full and acceptance
runs are more expensive than focused tests.

## Native project files

Open and save `.zipzi` files from the File menu. A project is one ZIP archive
containing the document and every valid image resource; copying the file carries
those resources with it. The Editor manages private resources under the platform
cache directory (`ZIPZI_CACHE_DIR` overrides it). Directory projects and bare JSON
are no longer supported as user project inputs.

Launch with one or more files; use `--` before filenames beginning with `-`:

```sh
cargo run --locked -p zipzi-editor -- -- "角色 one.zipzi" "角色 two.zipzi"
```

Startup, system file events and the Open dialog share a bounded queue (128 files,
1 MiB of paths). An existing Editor receives requests through its private Runtime
endpoint, including when it has no window. Opening the same file activates its
existing document; another path is a separate project even if its document UUID
matches. One failed file does not prevent the next file from opening.

Release macOS bundles register `.zipzi` as a regular ZIP document. Development
bundles offer support without claiming the default association. The Windows
portable archive includes `Register-FileAssociation.ps1` and instructions for
registering or unregistering the current application location for the current
user. Choose the default application through Windows settings; register again
after moving the portable application.

## Welcome panel

Starting without a project shows a dismissible panel inside the main window, with
New/Open actions, recent projects and the GitHub repository link. Opening a file
from the system skips the panel. Closing the last project leaves the empty
workspace visible. Escape or clicking outside dismisses the panel. Reopen it
from **Zipzi Editor > Welcome**.

## Resident application host

The application menu and About dialog provide **Check for Updates**. A quiet
title-bar icon appears only when an authenticated update is ready. Automatic
updates and explicit restarts respect unsaved projects, Runtime jobs and SDK
connections. See the [desktop update reference](../../docs/references/desktop-updates.md)
for release configuration, signing, packaging and installed upgrade acceptance.

Start the editor without a window using:

```sh
cargo run --locked -p zipzi-editor -- --no-window
```

The Python SDK can connect to the resident host and share its Runtime sessions,
documents and history. When connected clients, pending jobs or an explicit
`--no-window` launch keep the host resident, closing a window detaches its view;
reopening a window attaches the resident sessions. **Close Project** resolves
unsaved changes and releases the project's Runtime session and history.
Otherwise, closing the last window exits after any unsaved
documents are resolved. Use Quit to explicitly exit. See the
[Python SDK guide](../zipzi-python/client/python/zipzi/docs/guide.md) for
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
The `rotation` scene checks content-centered creation, direct picking of unselected bones, bone hover and drag
overlays, live modifier changes, angle readouts, uniform scaling and Alt/Option
display-length adjustment in all three languages. Set `ZIPZI_SCREENSHOT_SAVE=1`
to save all fifteen images.

Unselected Warp deformers show their evaluated outer lattice outline by default.
Click the outline to select the Warp; its interior remains available for Mesh picking.
Separate title-bar buttons hide unselected Warp outlines and Rotation bones in that
viewport independently, including direct picking. Selected editing controls remain available.
All/ancestor Warp grid commands add reference detail to these outlines.
Object and Warp transform frames share a 6 logical-pixel margin.
Primary editing points and handles render at 6 logical pixels; secondary Warp
lattice points render at 5 pixels. Both use a circular 6-pixel hit radius. Editable lines use a 3-pixel hit tolerance. Rotation surfaces
use the exact bone polygon; unselected bones render at 20% opacity.
Picking ranks points, lines, then surfaces. Within a point/line class the nearest
hit wins, with selected objects resolving equal distances; Mesh surfaces retain
compositing order. Hover changes color without growing markers or their hitboxes.
Reference lines use 0.5-pixel strokes and active lines use 1-pixel strokes.

## Deformer creation

New Warp and New Rotation wrap the current selection in one undo step. Toolbar,
menu and shortcut creation share this behavior; row menus prioritize a nonempty
selection over the clicked row. Selected Parts include their organization
descendants. Selected parents retain their children, and organization ownership
and drawing order stay unchanged. Base shapes, parameter keyforms and BlendShape
deltas are converted together to preserve the evaluated result. Without a
deformable selection, creation produces an empty deformer.

New Rotation places its origin at the current evaluated content center, including
nested Mesh descendants, and converts all child shape channels together so the
artwork stays in place. Empty UI creation uses the last active Modeling viewport
center, including after leaving that viewport to activate a toolbar or menu.

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

## Part compositing

Select a Part and enable **Offscreen drawing** in Properties to composite its
contents as a group. The Part's layer icon indicates compositing; Offscreen is
an internal component rather than a separate row or creation-menu item.
Properties exposes opacity, multiply/screen colors, blend modes and clipping
masks. Use the Part's existing visibility controls to hide its contents.
The Part's Enabled checkbox reflects both its own flag and its Offscreen flag;
enabling it restores a disabled composite without replacing its settings or
keyforms, in one undo step.
Removing compositing preserves the Part and its
contents; Undo restores its settings and animation data.

Opacity and colors edit the current stored Part keyform, or the static appearance
when the Part has no parameter binding. Interpolated poses and contributing
Offscreen BlendShapes require returning to an editable stored/base shape. Shared
source keyforms are detached for the edited Part slot, preserving other samples.
Numeric/color previews support cancellation and commit as one undo step.

`ZIPZI_SCREENSHOT_SCENE=part-offscreen` captures the Chinese Part inspector with
offscreen drawing enabled and opacity set to 60%, using the native Metal harness.

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
Rotation uses a Spine-style bone: drag its circular root to translate, its body
to rotate, and its square tip to scale uniformly along the bone axis. Hold
Alt/Option when pressing the tip to adjust the display length without changing
the pose or its children. The chosen tip operation stays fixed until release.
Initial display length follows a ray from the evaluated root along the displayed
bone direction to the combined descendant Mesh bounds, including hidden and nested
Meshes. The tip stays 15% inside the forward intersection segment. The length is
captured once and saved at a reference scale; missing/degenerate bounds or a ray
that misses the forward bounds fall back to 128 model units.
The Rotation Properties panel exposes Bone length as an Editor-only numeric field,
independent of pose and keyforms, with preview and undo/redo. Rotation values
truncate their display to two decimal places for origins, one for angles and bone
length, and three for scale percentages. Tiny nonzero values retain necessary
digits; shortened display text never quantizes stored values on focus or submit. Imported
coordinate conversion scales are normalized against that reference; subsequent
own/inherited scale changes and viewport zoom apply to the bone body,
including its width, without a minimum screen length. Root and tip handles keep
a fixed logical-pixel size and hit radius. Cubism imports
retain a 90-degree display-axis offset to match their vertical handle convention;
native bones keep their horizontal reference. This Editor metadata persists across
save/reopen and affects painting and picking without changing runtime poses. Bones use
a blue fill from the Warp transform-frame palette when selected, and a translucent
gray fill when unselected. Unselected bones remain
visible and directly pickable independently of the optional Warp helpers; every
overlapping bone appears in the canvas candidates, respecting visibility, locks
and viewport isolation. Painting and picking share the same bone geometry.
Selected bones show numeric feedback while dragging.
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
centered. Object frames and Rotation controls share the same modifier policy for
translation axis locking, rotation steps and fine translation/scale increments.
The Rotation body supports continuous rotation across full turns. Rotation
scaling is always uniform; Alt/Option at tip press reserves the gesture for display length. Shift and Alt/Option take effect as they are pressed
or released during a drag, including with a stationary pointer. Shift locks the
dominant translation axis until released; pressing it again chooses a new axis.
Alt/Option changes the scale pivot immediately. Every preview recalculates from
the same press geometry and parent space, so toggling constraints does not
accumulate transforms or create additional undo steps.
Container transforms involving Rotation children scale uniformly; incompatible
parent spaces reject unrepresentable transforms.
Escape cancels a gesture; commit creates one undo step. Locked objects and
noneditable parameter poses retain their normal editing restrictions.

Hold Command on macOS or Ctrl on Windows before dragging Warp or Rotation
controls to keep child artwork fixed while translating, rotating or scaling the
deformer. Both use the same coordinate compensation, including ordinary keyforms
and BlendShape child channels. This policy stays fixed for the gesture while
Shift/Alt constraints remain live. The edited deformer and its ancestor spaces
must be static and reversible; unsupported conversions cancel the complete edit.

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
group's contents with scope tabs. Impact report tabs switch between deletion,
dependency and blocking details; technical report tabs appear under **View details**.
Choosing replacements or changing
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

## Deformer selection benchmark

The opt-in selection workload alternates between two deformers in a native
window and verifies that the inspector follows without adding history entries:

```sh
cargo build --locked --release -p zipzi-editor --features test-support
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_CONFIG_DIR="$PWD/target/selection-config" \
  ZIPZI_WINDOW_BENCH=1 ZIPZI_FRAME_TRACE="$PWD/target/selection-trace.jsonl" \
  ZIPZI_BENCH_MODEL=/absolute/path/to/model.zipzi \
  ZIPZI_BENCH_DEFORMER_SELECTION=1 ZIPZI_BENCH_SAMPLES=12 ZIPZI_BENCH_WARMUP=2 \
  target/release/zipzi-editor
```

The default targets are the first two deformers. Set
`ZIPZI_BENCH_SELECTION_OBJECTS=id1,id2` to select two specific objects, including
deep leaves with many potential parents. The trace records `selection_tick`,
`selection_dispatch_end` and `deformer_parent_choices`, alongside native draw
and presentation events. Command injection excludes OS input delivery and tree
hit testing. Each run requires a new trace path; existing traces are preserved.

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
