# zipzi-observe

Frozen snapshot capture and explicit-view GPU rendering for Runtime observation
jobs. The public surface follows the paths used by `zipzi-runtime`:

- `ObservationInput::capture_from_snapshot` evaluates one parameter pose.
- `ObservationInput::capture_motion_from_snapshot` captures the matching frozen
  animation state and applies Model opacity before mask / Offscreen composition.
- `ResolvedObservation::capture_many_with_control` validates 1..64 inputs from one
  snapshot, resolves each texture once, and shares decoded content across frames.
  Cancellation is checked between samples, decodes and scene digests; an entered
  decode can finish.
- `Observer::new` and `render` reuse a device, renderer and texture cache across
  explicit ROI renders. `RenderRequest::mapping` preserves aspect ratio and
  supplies the model-to-image scale, offset and actual visible ROI.
- Results contain raw RGBA8, PNG encoding, effective parameters, drawable geometric
  bounds, texture hashes, document identity, evaluation revision and GPU backend.
  Capture, scene, render and input digests identify the acquired content and view.

The native backend is Metal on macOS and WGPU elsewhere. The `wgpu-backend`
feature selects WGPU on macOS too. Texture filtering uses linear mipmaps and
repeat sampling. Rendering requires a usable device; capture and digest checks
run on CPU. Geometry bounds do not prove unoccluded painted pixels.

Runtime owns job admission, cancellation, budgets and source lifetime. A batch
keeps its resolved textures until rendering finishes, without revisiting live
asset paths. Authoring records remain internal inputs to content digests and
batch validation. There is no scene save/open API or automatic fitted-view path.
Python inspection exports images and metadata, not a reloadable scene.

`RenderRequest.topology` optionally composites diagnostic wireframe, vertices,
persistent vertex IDs, stored boundary/constraint edges and default-pose edge
strain in Rust after GPU readback. `topology_only` clears appearance first.
These layers use the exact `ViewMapping` and ignore masks/occlusion. Both render
and input digests include the overlay options. Optional Mesh/Part `targets` select
the diagnostic layers and quality reports while retaining the full context image.
Parts include descendants; an empty explicit selection draws/reports no meshes.
Labels use a bounded global screen-space layout, nearby leaders, adjustable scale,
spacing and count; crowded labels are omitted. `anomalies_only` restricts all layers
to threshold violations, flips, degeneracy and undefined reference ratios.
Heat uses a separate most-severe-per-pixel layer blended once at `heat_opacity`,
preventing opacity accumulation from shared edges or overlapping mesh layers.
`RenderRequest.quality` includes
per-mesh model-space metrics and source-alpha UV coverage at at most 256x256
regular pixel-center samples (alpha >= 8). Coverage ignores filtering, atlas
ownership and out-of-range repeat; it reports sampling step and denominators.
Capture with `capture_*_from_snapshot_with_quality(..., true)` before requesting
quality or strain. The reference uses default parameters of that frozen source,
including hidden meshes. CPU-only sampling exposes the same geometric metrics
through `GeometryChecks.quality`, without texture coverage. Reports certify only
the requested poses. Coverage and overlay CPU work have explicit bounds.

Focused CPU checks:

```sh
cargo test --locked -p zipzi-observe --lib
cargo test --locked -p zipzi-observe --test capture cancelled_capture
cargo test --locked -p zipzi-observe --test capture candidate_animation
cargo test --locked -p zipzi-observe --test capture batch_samples
```

GPU checks, including the Runtime adapter:

```sh
cargo test --locked -p zipzi-observe --test capture
cargo test --locked -p zipzi-observe --features wgpu-backend --test capture
cargo test --locked -p zipzi-runtime --features gpu-observation --test observation -- --include-ignored
```

GPU observation through Python is checked explicitly with
`./tools/test-sdk.sh python2 --observation`.
`ObservedFrame.adapter_backend` distinguishes native `Metal` from `wgpu (Metal)`.
