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
