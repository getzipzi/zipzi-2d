# Canvas interaction

The canvas has three kinds of hit target: an object, a component within that
object, and an operation handle. Object selection is shared with the hierarchy
and parameter panels. The primary object drives the inspector; the full ordered
selection supports Shift toggling and rectangle selection.

## Tools and selection

- **V — Select:** pick a visible mesh surface or a visible deformer marker.
  Selecting a Mesh immediately exposes its vertices and triangle edges; selecting
  a Warp or Rotation exposes its controls and transform handles. There are no
  separate Mesh Edit / Deformer Edit tools or E / D shortcuts.
  A click that hits no target clears object and component selection, including
  Shift-click. With no editable objects selected, blank drag
  selects fully enclosed visible meshes. Shift adds objects. Selection is editor
  state and does not change history.
- **Selected meshes:** show their vertices and triangle edges immediately.
  Click/Shift-click select vertices; drag away from controls (including inside an
  already selected mesh) selects enclosed vertices across the selected meshes.
  Clicking a selected surface clears component selection and retains the object.
  Components carry both object identity and stable VertexId.
- **Selected deformers:** click or Shift-click selects Warp points; dragging a
  rectangle selects points across selected Warps, including when the press falls
  on a descendant mesh. Clicking a selected framework or its contents clears
  component selection while retaining the selected deformer. Clicking empty
  canvas in V deselects objects; clicking another object's marker selects it.
  Unselected ancestor helpers remain opt-in. Framework interiors are not object
  hit regions. Hand/Space navigation never writes control points.
- **G — Glue:** shows selectable connection segments and endpoints. Selection
  opens the shared inspector; this tool does not modify Glue weights or pairings.
- **H / held Space / middle mouse:** navigation. The selected tool is per document;
  held keys and native pointer capture belong to the window adapter.

Selecting a mesh shows its own outline, vertices and triangle edges. A selected deformer shows its own framework. The canvas context menu and
View menu offer independent, checked switches for ancestor deformers and all
deformers. Both default to off, and each document retains its own settings.
Hidden controls do not keep an invisible hit region. Active tool handles precede
object surfaces. Deformers whose helpers are hidden remain selectable from the
hierarchy or the ancestor candidates in the canvas context menu.

### Deformer manipulators

Warp editing defaults to **Bezier**: sparse anchors and round tangent handles
shape a piecewise bicubic controller surface. Dragging an anchor moves its attached
handles; dragging a tangent changes that handle independently. Shift supports
component selection. The toolbar's **Bezier / Division points** switch exposes
the actual conversion lattice for fine edits. Mode changes cancel active gestures.

Authoring controllers are optional, serialized with the Warp base shape or its
individual keyform, and included in SDK undo/redo. Legacy projects initialize
controllers from their existing lattice without writing or changing the model on
selection. Edits apply the controller surface's displacement to the original
lattice, preserving detail that the fitted controller does not represent. Fine
point edits invalidate that form's controllers so they can be fitted again.

Tangent edits refine grids with fewer than four divisions using a uniform integer
factor. Base and ordinary keyform lattices migrate atomically; only the active
form receives the Bezier displacement. Cancel, undo and dragging back restore the
original grid size and controller metadata. Parent-local mapping and reversed
canvas coordinates apply to anchors and handles alike.

This is an authoring implementation inspired by the official Cubism separation of
[Bezier and conversion divisions](https://docs.live2d.com/en/cubism-editor-manual/making-and-placement-of-warp-deformer/).
It does not claim numerical parity with Cubism's proprietary Smooth 1–3 / Smooth
All or 5.2/5.3 Bezier algorithms. Controller divisions currently initialize at
2×2 when the conversion lattice permits it, otherwise 1×1; configurable counts,
linked-tangent modes and the one-shot automatic-shaping command remain future work.
The runtime Warp evaluator remains unchanged and consumes the baked grid.

Selected Warps have an outer frame with a fixed 4-logical-pixel gap from the
control grid.
Drag a frame edge to translate all
points; its eight square handles scale around the opposite corner or edge. Hold
Shift before dragging a corner for uniform scaling. Scaling stops
short of zero; crossing a handle does not introduce a reflection.

Selected Rotations show a compact ring, a direction axis and a separate square
scale handle. Drag the ring or direction endpoint to change angle, the square to
scale, and the center to translate the deformer **and its contents**. Center-only
relocation with descendant compensation is not implemented in this iteration.
Rotation rings, direction axes and scale-handle distances follow the viewport's
zoom, where 100% maps each source pixel to one physical display pixel. Handle
markers and pointer tolerance retain logical-pixel sizes for picking.

The contextual toolbar follows deformer selection and contains ancestor display,
Bezier/fine editing, subdivision preview and gesture help. There is no Transform dialog. Rotation
scale is restricted to the evaluator's 0.0001–100 range and authored angles to
−3600–3600 degrees.

Optional subdivision preview samples the Warp interpolation using evaluated
control positions, capped at 48 subdivisions per axis. These faint lines are
visual guides only and never become selectable control points. The toggle is
per document and defaults to off. Geometry for outer-frame drawing and picking
comes from the same screen-space gizmo; hover changes cursor/handle feedback and
dragging displays the current delta, scale or angle.

This iteration edits existing deformers. Creating deformers, changing their
parents and relocating a rotation center while preserving its contents still
require a separate authoring workflow.

Right-click lists the objects at the pointer and their organizational/deformation
ancestors, including Part and Offscreen. These containers show member outlines and
bounds when selected, but their bounding rectangles never capture normal clicks.
Masks and model HitAreas remain references to meshes; their existing relation
navigation opens the corresponding mesh. They do not acquire duplicate geometry.

## Picking and presentation

`picking.rs` builds an immutable `CanvasScene` paired with the rendered texture's
`CanvasFrame`. Picking and control overlays use that displayed frame's evaluated
geometry and viewport, including its evaluated deformer controls. An edit can start
only when the displayed scene and viewport versions are current.

Mesh picking rejects degenerate triangles and tests barycentrically interpolated
UVs against bilinearly sampled source alpha. Drawable UVs use bottom-up V;
picking converts V to top-down decoded texture rows, matching the GPU vertex
conversion for both surfaces and mask sources. Masks combine source coverage,
including inverted masks. Offscreen opacity/masks and the nested render plan
control candidate order and eligibility. A candidate must exceed alpha 1/255.
Overlapped eligible meshes remain available in the context menu.

Component overlays and rectangle selection respect visibility and opacity through
nested Offscreen groups. Texture holes and masks do not remove control points
from an otherwise visible selected mesh's topology. `canvas_topology.rs` caches
vertex IDs, triangle/edge connectivity, deformer grids and object ancestry across
pose updates. Canvas selection uses a small metadata index; it does not rebuild
the inspector's property and binding projection during continuous edits.

Parameter and control edits publish new immutable positions using shared topology.
Opacity previews share the previous positions but publish the latest frame source,
visibility and render plan, so transparent meshes and masks remain correct for
picking. Rename, general property changes and history navigation invalidate the
structure cache; old displayed frames retain their own metadata and geometry.

This is a CPU authoring hit policy, not a readback of the final GPU framebuffer.
Mipmapped/filtering edges and values near the alpha threshold can differ from the
final displayed pixel. Picking performs no GPU wait or readback. A coarse bounds
check avoids triangle work outside each mesh.

The companion zipzi-core evaluation exposes transient, non-serialized deformer
points on `DrawableFrame`, sampled in the same evaluation pass as meshes. The
editor does not approximate a posed warp grid from its undeformed authoring data.
Both mesh and deformer snapshots include the final Y reversal requested by
`Canvas.flag`; omitting it mirrors controls vertically on affected imported models.
Root-position writes and rotation gestures invert this same final conversion.

## Editing transactions

`tools.rs` owns press/drag/marquee state, component selection and hover.
`control_edit.rs` resolves a fixed authoring target when the pointer passes the
three-logical-pixel drag threshold. A click does not create an SDK edit candidate.

Unbound meshes/deformers edit the base shape. Bound objects require all their
binding axes to match an existing stored keyform; interpolation positions are
read-only. The canvas displays the current write destination and parameter values.
The two-axis SDK limit remains in force. Direct BlendShape targets and meshes
participating in Glue remain read-only for control editing.

Dragging a point maps its original authored position into the displayed parent
space, applies the screen-space delta and solves back into parent-local space.
Noninvertible or nonconvergent deformations cancel the candidate. Rotation controls
preserve the original f64 pivot when only the angle changes. A single transaction
can edit selected vertices across meshes; simultaneously editing a deformer and
its descendants is rejected to avoid applying displacement twice.

Mouse release commits once. Escape, lost button state, focus/window deactivation,
resize, save, tool switch, or document switch cancel the candidate. Returning to
the original position and selection-only operations preserve redo. SDK owns the
only model history.

## Validation

Core tests cover alpha holes, masks, offscreen ordering, object/rectangle selection,
per-document tools and helper visibility, exact-keyform writes, parent-local
coordinates, rotation, multi-mesh transactions, no-op/cancel/invalid input, and
stale presentation.
Cache regression tests check shared topology with independent posed positions,
fresh alpha/visibility during opacity previews, immutable old frames, and metadata
refresh after rename, cancellation, property changes and undo/redo.
Warp-to-mesh alignment and pointer-following drags cover reversed canvases with
nonzero origins, nonunit pixels-per-unit, zoom and pan.
GPUI headless tests cover surface selection, tool buttons/shortcuts, the deferred
context menu's selection and helper switches, hand/Space navigation, and input focus.
They also dispatch complete vertex drags, shortcut undo/redo, Escape, tool changes,
and focus loss in an active test window.

The CPU smoke test below exercises all selection/editing tools on the locally
available full models, checks finite overlays and valid context-menu targets, and
tries eligible vertex drags followed by cancellation. It never saves the models.
Reported CPU timings exclude GPU rendering and native display latency.

```sh
cargo run --locked -p zipzi-editor-core --example canvas_selftest -- \
  models/local/mao_pro.zipzi/project.zipzi.json \
  models/local/Shirousagi.zipzi/project.zipzi.json
```

Actual native pointer capture outside the window and visual feedback on different
DPI displays still require manual inspection. The source-alpha threshold policy
is intentional; this implementation does not add GPU pixel-ID picking.

### Performance comparison (2026-10-02)

Native-window captures compare committed editor `397f4f4` / SDK `97de656`, the
interaction rewrite before this fix, and the cached implementation. All use Mao,
the former Vertices tool, a 960 × 1364 physical render target, 30 warmup and 360 measured
inputs per workload. The machine is an Apple M4 with 16 GB RAM and a 75 Hz display.
FPS below counts newly presented model previews, excluding repeated window images.

| Workload | Committed baseline | Before fix | After fix |
| --- | ---: | ---: | ---: |
| Debug parameter drag | 74.83 FPS | 37.50 FPS | 74.50 FPS |
| Debug opacity drag | 27.84 FPS | 18.59 FPS | 27.73 FPS |
| Release parameter drag | 74.33 FPS | 74.33 FPS | 74.33 FPS |
| Release opacity drag | 75.00 FPS | 68.18 FPS | 75.00 FPS |

Parameter scene-building P95 falls from 14.03 to 0.72 ms in Debug and from 2.02 to
0.20 ms in Release. Pan and zoom remain around the 75 Hz display limit. Debug
opacity retains the committed version's existing UI projection cost; this change
removes the additional canvas regression without claiming to resolve that cost.

The local `target/perf-20261002-canvas-fix/` directory contains frozen binaries,
source snapshots, scripts, raw traces and `comparison.json` / `comparison.csv`.
There are 14 accepted captures with zero lost trace events. Two incomplete runs
lost window visibility/focus and were excluded and rerun; their data is retained.
All binaries were built before native sampling; the CPU-only probe ran afterwards.
