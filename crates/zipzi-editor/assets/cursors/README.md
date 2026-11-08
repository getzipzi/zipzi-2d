# Editor cursor artwork

These SVGs are original vector masters for native cursors. They use a dark
stroke with a white outer contour so the hotspot remains readable over light,
dark and textured canvases. The canvas continues to draw operation previews.

| Asset | Logical size | Hotspot from top-left | Standard fallback |
| --- | --- | --- | --- |
| `mesh-add.svg` | 24 × 24 | (7, 7), center of the precision cross | Crosshair |
| `mesh-delete.svg` | 24 × 24 | (7, 7), pointer tip | Arrow |
| `mesh-connect.svg`, `mesh-connect-new.svg` | 24 × 24 | (7, 7), first endpoint | Pointing hand |
| `mesh-fix.svg` | 24 × 24 | (7, 7), center of the edge; lock badge means fixing an automatic edge | Pointing hand |
| `rotate.svg` | 24 × 24 | (12, 12), midpoint of the 90-degree arc | Open hand |
| `bone-length-{0,45,90,135}.svg` | 24 × 24 | (7, 7), center of the axial arrow | Corresponding standard resize arrow |
| `lasso.svg`, `lasso-{add,remove,toggle}.svg` | 24 × 24 | (7, 7), precision cross | Crosshair |
| `brush-select.svg`, `brush-{add,remove}.svg` | 24 × 24 | (7, 7), precision cross | Crosshair |

`src/tool_cursors.rs` rasterizes the master at 1x, 2x, 3x and 4x and registers it
once per application. Window requests and active drags carry only a resource ID.
Native size follows display scaling, independent of model zoom. Changing artwork
requires a new registration; cloned artwork retains its resource identity.

`tool_cursors::resolve` selects semantic actions before mapping them to these
registered resources. Captured operations come from Core's `active_gesture`;
idle actions use the existing hover feedback and current modifiers. Selection
badges use Core's `selection_operation`, including deferred click toggles; brush
Replace clears the previous selection, Add retains it and Remove erases weights.
Point and object motion use the standard arrow while idle and captured. View
panning uses the open/closed hand styles. Captured canvas cursors also request a
window-wide override until release/cancellation, so leaving the canvas hitbox
does not give a toolbar or peer viewport control of the pointer. Length
variants reuse the four standard resize directions and never allocate artwork
for a floating-point angle. Variant and modifier changes retain the same hotspot.

Rotation uses a 90-degree arc with open chevron arrowheads tangent to both ends.
Sixteen directions, 22.5 degrees apart, are rasterized and registered once. Hover
and captured rotation choose the nearest direction from Core's displayed gizmo angle;
missing or non-finite angles use the default direction. Direction changes only
select a cached resource ID and keep the logical hotspot at (12, 12).
Frame rotation uses its frame angle directly. Rotation deformers report their
bone-axis angle and add a clockwise quarter turn to align the cursor's arc normal.

Windows' original open/closed hand masters live in
`vendor/gpui-fast-windows/resources/cursors/`, because standard GPUI hand styles
must also work outside the Editor canvas. macOS keeps its system hand cursors.
