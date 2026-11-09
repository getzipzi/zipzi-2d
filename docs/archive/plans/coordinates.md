# Coordinate contract

Zipzi stores geometry in origin-relative model coordinates: X right, Y up.
The model has no canvas rectangle, source-origin offset, axis flag or PPU.
One model unit is one pixel at the default artwork import scale. Texture
resolution does not impose a model size; resampled artwork needs an explicit
placement/import scale.

## Spaces

- Root mesh positions, root Warp control points and root rotation centers are
  model-space positions, with the model origin at (0, 0).
- Children of a rotation use length-valued rotation-local coordinates.
- Children of a Warp use dimensionless grid parameters. Do not apply PPU to them.
- Warp control points and rotation centers are stored in the parent's space.
- Keyforms use the same space as their target's base data. BlendShape deltas
  are vectors in that space: scale/reflect them, never translate them.
- Angles are degrees, positive counterclockwise in model space. Preserve turns.
- UVs have their origin at the bottom-left, independent of geometry axes.
  Render backends convert UVs to their image-row convention at upload.
- Triangle indices use the same winding in document geometry and evaluated
  frames. Backends handle the target camera's reflection consistently.

`Document::coordinate_space(parent)` and SDK `GeometrySpace` identify model,
rotation-local and Warp-parameter data. `DrawableFrame` contains evaluated
model geometry only. Bounds, hit tests and observer ROIs are all model-space
queries; there is no conversion back to a source canvas.

## Format boundaries

MOC3 import expands model/rotation lengths by the source PPU and normalizes
its axis flag, UVs and winding. Warp parameters remain unchanged. The alternate
axis convention also changes rotation angles and the direction inherited from
Warp parents; it is normalized during import, never during frame evaluation.
PSD import places source pixels relative to its reference origin and converts
Y-down image placement into Y-up model positions.

`ModelSetup::source_canvas` retains source-format provenance. Changing it does
not affect evaluated geometry or output policy. `ModelSetup::export` contains
independent `ExportSettings`:

- `pixels_per_unit`: model/canvas pixels per Cubism runtime unit.
- `bounds`: optional stable [left, bottom, right, top] model-space rectangle.
- `padding_fraction`, `padding_pixels`, `minimum_extent`: automatic framing.

Automatic framing uses evaluated default-pose geometry, including hidden
meshes. Specified bounds bypass automatic padding and minimum extent. Pixel
edges round outwards. Export origin is (-left, top) in top-left canvas pixels;
MOC3 stores its origin using that format's bottom-left convention. MOC3 output
uses the canonical axis flag. CMO3 root positions convert to Y-down canvas
pixels at serialization; parent-local parameters retain their own semantics.

Project format 14 stores this contract and output settings. Earlier coordinate
contracts are rejected; reimport original artwork/model assets.

## Editor and renderer

The editor camera maps model positions to viewport-local logical pixels:

```text
screen.x = width/2  + pan.x + model.x * zoom
screen.y = height/2 + pan.y - model.y * zoom
```

`ModelPoint` represents a position and `ModelVector` a displacement.
Zoom is logical pixels per model unit. Device scale only affects physical
render target size and the final model-to-target matrix. Fit centers the model
bounds; empty documents use the origin and default zoom. Pointer events invert
this camera, then editing tools invert the object's evaluated parent mapping.
GPU vertices, mesh bounds and masks stay in model space until the viewport
matrix maps them into the target. All consumers use the same coordinates.
