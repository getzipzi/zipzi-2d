# Editor cursor artwork

These SVGs are original vector masters for native cursors. They use a dark
stroke with a white outer contour so the hotspot remains readable over light,
dark and textured canvases. The canvas continues to draw operation previews.

| Asset | Logical size | Hotspot from top-left | Standard fallback |
| --- | --- | --- | --- |
| `mesh-add.svg` | 24 × 24 | (7, 7), center of the precision cross | Crosshair |
| `mesh-delete.svg` | 24 × 24 | (7, 7), pointer tip | Arrow |
| `mesh-connect.svg`, `mesh-connect-new.svg` | 24 × 24 | (7, 7), first endpoint | Pointing hand |
| `mesh-fix.svg` | 24 × 24 | (7, 7), center of the fixed segment | Pointing hand |
| `rotate.svg` | 24 × 24 | (7, 7), precision cross | Open hand |
| `bone-length-{0,45,90,135}.svg` | 24 × 24 | (7, 7), center of the axial arrow | Corresponding standard resize arrow |

`src/tool_cursors.rs` rasterizes the master at 1x, 2x, 3x and 4x and registers it
once per application. Window requests and active drags carry only a resource ID.
Native size follows display scaling, independent of model zoom. Changing artwork
requires a new registration; cloned artwork retains its resource identity.

`tool_cursors::resolve` selects semantic actions before mapping them to these
registered resources. Captured operations come from Core's `active_gesture`;
idle actions use the existing hover feedback and current modifiers. Length
variants reuse the four standard resize directions and never allocate artwork
for a floating-point angle. Variant and modifier changes retain the same hotspot.

Windows' original open/closed hand masters live in
`vendor/gpui-fast-windows/resources/cursors/`, because standard GPUI hand styles
must also work outside the Editor canvas. macOS keeps its system hand cursors.
