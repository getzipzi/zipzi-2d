# Editor cursor artwork

These SVGs are original vector masters for native cursors. They use a dark
stroke with a white outer contour so the hotspot remains readable over light,
dark and textured canvases. The canvas continues to draw operation previews.

| Asset | Logical size | Hotspot from top-left | Standard fallback |
| --- | --- | --- | --- |
| `mesh-add.svg` | 24 × 24 | (7, 7), center of the precision cross | Crosshair |

`src/tool_cursors.rs` rasterizes the master at 1x, 2x, 3x and 4x and registers it
once per application. Window requests and active drags carry only a resource ID.
Native size follows display scaling, independent of model zoom. Changing artwork
requires a new registration; cloned artwork retains its resource identity.

Windows' original open/closed hand masters live in
`vendor/gpui-fast-windows/resources/cursors/`, because standard GPUI hand styles
must also work outside the Editor canvas. macOS keeps its system hand cursors.
