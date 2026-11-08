# Editor cursor inventory and artwork

The editor owns 21 SVG masters, producing 36 registered resources: five mesh
operations, four box selections, four lassos, three brushes, four bone lengths,
and sixteen orientations of one rotation master. All masters are 24 × 24 logical
pixels. Assets are original vectors with dark strokes and a white outer contour.

## Visual grammar

Precision tools share the same symmetric cross and hotspot at (7, 7). The cross
means the exact interaction location, never “add”. A separate silhouette in the
lower-right quadrant identifies the tool. Every selection variant retains that
silhouette, including Replace. Modifier marks occupy the same lower-right
position: `+` Add, `−` Remove, `±` Toggle (invert each affected element). No mark
means Replace. These marks describe Core's effective selection operation, not
just the current keyboard modifiers. Brush has no Toggle operation.

Black/white contours preserve visibility without relying on operation colors.
The silhouettes and modifier marks must remain recognizable at 1×; inspect them
on both light and dark backgrounds when changing the artwork. The canvas draws
the actual brush radius and selection/geometry previews independently.

## Custom artwork

| Assets | Meaning and visual distinction | Standard fallback |
| --- | --- | --- |
| `box-select`, `box-{add,remove,toggle}` | Corner-bracket rectangle; selection operation in the corner | Crosshair |
| `lasso`, `lasso-{add,remove,toggle}` | Closed rope loop with a loose tail | Crosshair |
| `brush-select`, `brush-{add,remove}` | Diagonal brush tip; actual radius belongs to the canvas overlay | Crosshair |
| `mesh-add` | Vertex square with plus: create a vertex | Crosshair |
| `mesh-delete` | Trash can: delete geometry, distinct from subtracting selection | Arrow |
| `mesh-connect` | Two square endpoints joined by an edge | Pointing hand |
| `mesh-connect-new` | Square endpoint joined to a plus: connect and create a new endpoint | Pointing hand |
| `mesh-fix` | Edge with closed lock: make an automatic edge fixed | Pointing hand |
| `bone-length-{0,45,90,135}` | Axial double arrow with ruler: change bone display length | Corresponding resize arrow |
| `rotate` | Short curved double arrow: rotate in either direction | Open hand |

Names in this table have the `.svg` extension. Bone length arrows are centered
at (7, 7); their ruler stays upright as the arrow direction changes. Rotation's
hotspot is (12, 12), exactly on the midpoint of its arc. Its outward normal is
−Y. Sixteen directions, 22.5° apart, rotate around that fixed hotspot. Frame
rotation uses the displayed frame angle; Rotation deformers add a clockwise
quarter turn to their bone-axis angle. Non-finite angles use the default.

## System cursors and actual routes

| Cursor | Editor use |
| --- | --- |
| Arrow | Default picking, moving points/objects, ordinary canvas box selection, cancelled gestures, disabled numeric inputs |
| Pointing hand | Clickable tabs, rows and buttons; mesh connection fallback |
| Open hand | Hand tool ready to pan, dock panel drag preview; rotation fallback |
| Closed hand | Captured view panning, timeline graph panning, hierarchy sorting drag |
| Four double-headed resize arrows | Scale handles oriented with the displayed frame, custom window borders; bone length fallback |
| Horizontal resize | Numeric value scrubbing and timeline adjustment |
| Crosshair | Color picker, brush-radius adjustment and custom precision-tool fallback |
| Operation not allowed | Uneditable targets, forbidden mesh deletion/connection; disabled component actions |
| I-beam | Text entry supplied by GPUI Kit |
| Column resize | Table column width handles supplied by GPUI Kit |

The custom box selection family belongs to manual mesh selection. Ordinary
canvas box selection keeps the standard arrow. Move also deliberately keeps the
standard arrow, both idle and captured. There is no additional custom move,
scale, text or navigation artwork. Standard system shapes preserve platform
conventions. Windows open/closed hand masters live in
`vendor/gpui-fast-windows/resources/cursors/`; macOS uses system hands.

## Registration and validation

`src/tool_cursors.rs` resolves semantic actions before selecting artwork. Modal
UI yields control; captured panning and gestures take precedence over idle
hover. Captured operations come from Core's `active_gesture`; selection variants
come from `selection_operation`. Captured canvas cursors request a window-wide
override until release/cancellation, including outside the canvas hitbox.

Masters are rasterized at 1×, 2×, 3× and 4× and registered once per application.
Requests carry only resource IDs; native size follows display scaling, not
model zoom. Direction/modifier changes retain the same hotspot and select
cached resources. New artwork requires registration again (restart the Editor).

Focused checks from the repository root:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --test custom_cursors
./tools/test-editor.sh ui tool_cursor
```

The artwork tests cover DPI scaling, fixed hotspots, the unchanged precision
cross, transparent backgrounds and resource fallbacks. The UI tests also check
all sixteen rotated images for clipping and the resolver's operation routing.
