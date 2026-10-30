# Mesh quality and inspection

### Mesh quality scope

The Python modeling API currently exposes rectangle remeshing, manual topology
edits and complete topology replacement. A connected Editor may also offer
`zipzi.mesh_generation.generate()` for alpha-based mesh generation; check
`runtime.connection.capabilities` for `editor_mesh_generate`. The Rust SDK's
Alpha mesher is not bound to Python. Topology replacement
requires explicit dependency/binding and vertex-mapping handling; do not assume
arbitrary remeshing automatically migrates every keyform, BlendShape or Glue.

Sampling validity checks cover authored small triangles, inconsistent winding
and root model bounds. Request evaluated quality separately:

```python
from zipzi import sampling

source = session.meshes.get(mesh_id).source
sampled = session.modeling.sample(
    ({parameter_id: -1.}, {}, {parameter_id: 1.}),
    checks=sampling.Checks(quality=True), expected=source,
)
for pose_quality in sampled.value.quality:
    for mesh in pose_quality:
        print(mesh.mesh_id, mesh.min_angle_degrees, mesh.max_aspect_ratio,
              mesh.max_edge_stretch, mesh.flipped_triangles)
```

`quality` has one row per requested pose. Areas and angles use evaluated Y-up
model coordinates, including deformation parents. Area ratios and symmetric
edge stretch compare against default parameters of the same frozen source;
they do not compare consecutive samples or apply Motion physics to that baseline.
`relative_area < 0` means flipped. A zero-area reference has no area ratio;
collapsed reference or current edges have no finite stretch ratio. Collapsed
sampled triangles are counted explicitly; their aspect ratio is `None`.
An empty validity diagnostic list or a good sampled quality report does not
certify unsampled parameter intervals. CPU quality does not load image pixels.

For texture coverage and visual topology inspection, request an explicit view:

```python
from zipzi import observation, inspection

view = observation.View(
    width=512, height=512, roi=(0., 0., 1., 1.), quality=True,
    topology=observation.Overlay(
        wireframe=True, vertices=True, vertex_ids=True,
        constraints=True, strain=True,
    ),
)
images = session.modeling.observe(({},), view, expected=source).wait()
inspection.save(images, output_directory)
coverage = images.value.frames[0].quality[0].texture_coverage
```

The same view works with `preview.sample(..., view=view)`. Reports, topology
positions and stable vertex IDs come from that exact frozen source. Triangle
indices in quality reports match evaluated index order. Overlays are composited
in Rust onto the GPU readback using its exact model-to-image mapping. They ignore
masking and occlusion. By default only enabled, visible, positive-opacity meshes
are drawn; `include_hidden=True` includes the other evaluated meshes.
`topology_only=True` clears the appearance image to transparent first.

To focus inspection without hiding the textured context image, select meshes or
organizational Parts explicitly. Part selection includes nested Parts; mixed
selectors form a deduplicated union. `targets=None` selects all, `targets=()`
selects none, and absent IDs fail with `MISSING_MESH` / `MISSING_PART`.
When `View.quality=True`, its per-mesh reports use this same selection, retaining
all triangles and their metrics even when the drawing shows anomalies only.

```python
overlay = observation.Overlay(
    targets=(observation.PartTarget(value=face_part_id),),
    vertices=True, vertex_ids=True, label_scale=2, label_spacing=4, max_labels=128,
    strain=True, heat_opacity=0.2, anomalies_only=True,
    thresholds=observation.Thresholds(
        min_angle_degrees=10., max_aspect_ratio=20., max_edge_stretch=2.,
    ),
)
# To select one mesh instead: targets=(observation.MeshTarget(value=mesh_id),)
```

Wireframe is cyan, vertices and persistent vertex IDs are yellow, stored boundary
edges are green, and stored internal constraints are magenta. No constraints are
inferred when meshing metadata is absent. Strain fills progress from blue at 1x
edge stretch through orange at 2x to red at 4x or more. Flipped or collapsed
triangles are magenta. Undefined reference ratios are magenta too; inspect the
report's `None` values to distinguish them. In anomaly mode, shape violations
without excessive stretch use amber. Heat is applied once per pixel using the
most severe overlapping triangle, so layers and shared edges cannot accumulate
fill opacity. `heat_opacity` defaults to 0.25 and accepts 0..1; zero suppresses
heat while leaving independently enabled lines, vertices and labels available.

`anomalies_only=True` restricts every enabled diagnostic layer to triangles that
are degenerate, flipped, have undefined reference ratios, or violate one of the
three thresholds. Lines/constraints must belong to those triangles; dots and IDs
must reference their vertices. Normal triangle regions retain the original image.
This is an advisory drawing filter and never changes the quality measurements.

IDs use a two-pixel glyph scale by default (configurable 1..4). A global screen-space
layout tries nearby positions, draws a short leader to the vertex, and avoids
label rectangles with `label_spacing` clear pixels. Crowded/out-of-view IDs are
omitted instead of overlapping, with at most `max_labels` labels (default 256,
maximum 4096) per image. Set `max_labels=0` to suppress labels. Select a smaller
mesh/Part scope or zoom in when you need the omitted IDs. Fills and edges are
drawn before the label pass, so other mesh layers cannot overwrite labels.
Vertex IDs belong to each mesh; different meshes can reuse the same number.
Use a single Mesh target when you need an unambiguous ID-to-object association.

With `quality=True`, each image also reports source-alpha coverage of the UV
triangle union inside [0,1]. Alpha >= 8 is painted. Transparent covered centers
measure wasted coverage; uncovered opaque centers measure contour coverage
error, not a contour distance. Masks, occlusion, texture filtering, padding and
out-of-range UV repeat are excluded. Shared texture atlases can include painted
regions belonging to other meshes, so interpret missing coverage accordingly.
Pixel centers are tested at `pixel_step=max(1, ceil(max(width,height)/256))`;
`pixel_step > 1` means approximate coverage. Counts and denominators are returned,
with `None` fractions for zero denominators. The report separately counts UV
triangles extending outside [0,1]. Quality is opt-in; ordinary images are unchanged.
Coverage is bounded to 8 million triangle scanlines per frame and overlay drawing
to 64 million pixel visits; expensive requests fail explicitly. Reduce output
resolution or overlay options, or use CPU-only quality for complex geometry.
Inspection PNGs and JSON retain overlay options, quality and Source provenance.
