# Adaptive mesh generation

Native Rust adaptation of PSD2Live's automatic mesh generator, from
`tsunehimatoi/psd2live` revision `692986728e2b7b9254a41a22bdce1656bc0c12f9`.
Derived sources retain **GPL-3.0-only**; see the repository's
[third-party notices](../../../../THIRD_PARTY_NOTICES.md) and
[license](../../../../LICENSE-GPL-3.0).

The upstream files are `core/AdaptiveMeshGenerator.kt`, `core/AlphaEdgePreprocessor.kt`,
`core/MeshWrap.kt`, `core/MeshResolution.kt`, `core/mesh/EdgeBandBuilder.kt`,
`core/mesh/MeshGeometry.kt`, and `core/mesh/StructuredMeshFill.kt` under
`src/main/kotlin/io/github/psd2live/`.

## Pipeline and boundaries

`generate_mesh(width, height, alpha, settings, stage)` is a deterministic CPU-only
operation. Alpha is one byte per pixel, top-down. Positions use image pixels with
bottom-left origin; UVs are normalized to the source image. It never modifies
pixels or the document. Settings have independent edge layout and interior fill
strategies; each fill strategy retains its own controls.

1. Maximum-alpha working-resolution reduction; layer-relative alpha hardening;
   optional Euclidean-distance-transform closing (`wrap / 2`).
2. Pixel-edge contours, precise and coarse simplification, bounded filtering,
   periodic cubic guides and curvature-weighted station/half-station sampling.
3. Single contour row, double/triple staggered edge bands, or longitudinal
   two-sided ribbons for monotone slender islands. Band width responds to local
   clearance; normals are smoothed and folded regions narrowed. Invalid or
   colliding layouts try narrower bands, single rows, then raw contours.
4. Graded Poisson, simple triangular lattice, balanced 4–8 quadtree,
   red/green/blue triangle subdivision, or inward contour paving. Structured
   cores keep fixed triangles; only their free gaps are triangulated. Large
   contour fans receive up to three repair passes.
5. Constrained triangulation, bounded free-edge refinement and optional removal
   of boundary-to-boundary chords; topology and output-precision checks.

`Edges` omits interior sampling/refinement. `Full` includes fill. Generated
boundary, middle and inner loops are exposed alongside the geometry. Single-loop
meshes that meet the SDK's UV validation receive editable meshing metadata with
an empty `constraints` list. Band and patterned-fill connections are fixed only
during generation; the resulting edges are ordinary edges and may change during
later filling or reconnecting. Only explicit user edits create persistent
constraints. `fill_existing_geometry` consumes authored boundary loops, vertices
and user constraints directly, without reading alpha. Existing vertex IDs,
positions and UVs remain unchanged; proposed pattern cells that cross a user
constraint are freed before constrained triangulation.

The Editor replaces the secondary toolbar with a centered, top-attached
generation toolbar. Its fixed-width controls pack horizontally and wrap as needed.
Edges and interior fill are separate stages, sharing an accepted contour snapshot.
The interior stage can also use the original mesh boundary to fill an existing
or manually edited mesh without replacing its contour.
Parameter changes replace a disposable Runtime candidate used by the normal
viewport scene and topology overlays. Apply publishes one history entry; cancel,
Save/FinishEdit, and editing-context changes discard the draft. Generation never
implicitly commits through the ordinary gesture lifecycle.

## Intentional integration differences

- Rust uses `geo` pixel-square union for contour extraction and `spade` for CDT,
  instead of the Kotlin edge tracer and ear clipping/Lawson implementation.
  Vertex order, cocircular diagonal choices and some gap/refinement sites differ;
  this is a behavioral port, not bit-for-bit triangle reproduction.
- Structured cores constrain their triangle edges directly in one CDT rather
  than separately triangulating each gap and stitching the results. Both protect
  the patterned triangles and shared boundaries.
- The source texture is preserved. Alpha threshold is inclusive (clamped to at
  least 1), matching PSD2Live. Its relative hardening and small-island policy
  replace the previous Editor's minimum-margin and optional faint-island rules.
- `clip_to_image` is an integration option for manual editing. When needed,
  triangles are clipped to the image rectangle and boundaries reconstructed;
  partially clipped row metadata is cleared. Ordinary generation permits
  overhang and the existing document adapter adds transparent texture padding.
- `unit_scale = 1` and `detail = 1` use source pixels (the Editor default).
  The API can request document-scale working units and finer tracing. Output is
  always mapped back to source pixels. The UI currently exposes source-pixel
  settings, not document-unit or tracing-resolution selectors.
  Its starting distances and per-field scrub increments scale with source texture
  size; see the [Editor control defaults](../../../zipzi-editor/README.md#mesh-generation-controls).
- Work is bounded before potentially large allocations: 16384 per image axis,
  64 Mi source pixels, 262144 merged rectangles, 1024 components, 65534 traced
  points, and a conservative 1048576 candidate-lattice work bound. Output has a
  configurable maximum of 65534 vertices. Expanded layouts also have a candidate
  budget and can trigger narrower-layout recovery. Exceeding the input/output
  limits, or exhausting valid recovery layouts, is an explicit error.

The original MIT SDK alpha-mesh API remains available to SDK clients. The Editor
uses this GPL module and the new `MeshSettings`; it no longer calls that generator.

## Validation

From the repository root:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --lib adaptive_mesh::
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration mesh_generation::
env -u ZIPZI_DEV_SESSION_DIR cargo check --locked -p zipzi-editor --lib
```

The reference fixture contains actual PSD2Live results for ellipse, hole, ribbon
and concave masks across all 15 edge/fill combinations. Tests compare component
and hole counts, foreground-pixel-center coverage and vertex budgets, in addition
to orientation, manifold incidence, determinism and preservation of edge rows
when changing fill. This does not prove equal behavior for every drawing or equal
deformation quality. Tests also cover wrapping, cropping, errors, and nearest
segment queries against brute force.

Set `ZIPZI_MESH_REPORT=/absolute/path/report.json` on the reference test to export
per-case metrics and generated geometry. See
[fixture generation](../../../../tools/mesh-reference/README.md) to refresh the
upstream reference from a local PSD2Live checkout.
