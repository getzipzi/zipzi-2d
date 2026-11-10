# zipzi-editor-render

Metal, Direct3D 11, and wgpu scene adapters for the Editor. They consume
[`zipzi-editor-core`](../zipzi-editor-core/README.md) scene snapshots and viewport transforms,
and delegate model rendering to the corresponding shared backends.
Document editing and project I/O remain outside this crate.

## Rendering boundary

`MetalSceneRenderer` uploads and caches scene textures, synchronizes shared model
frames, applies viewport transforms and submits rendering on the consumer's Metal
queue. `SubmittedFrame` retains the output texture and command buffer so callers
can track GPU completion without blocking the UI thread. `MetalOverlayRenderer`
and the overlay types draw Editor interaction overlays.

The Metal implementation is in `src/metal.rs` and `src/overlay.rs`; the WGPU
adapter is in `src/wgpu.rs`. Backend-independent
render planning belongs in `zipzi-render`; reusable Metal rendering belongs in
`zipzi-render-metal`. This crate adapts those facilities to Editor scene and
interaction data.

Cold Metal texture misses are deduplicated by content and prepared with at most
four CPU workers. Batches bound the estimated temporary base/mipmap/footprint
workspace to 64 MiB; a single image exceeding that bound is processed alone.
Workers generate straight mipmaps before premultiplying owned upload buffers.
Metal allocation/upload stays on the rendering thread in source order. Cached
frames create no preparation threads. Authored image bytes remain unchanged.

The fixed-scene first-frame probe excludes project loading and renderer creation
from its timers, checks real artwork mipmaps against the general area filter,
and reads GPU output back after timing. Each cold sample uses a fresh renderer:

```sh
cargo run --locked --release -p zipzi-editor-render --example first_frame_benchmark -- \
  /absolute/path/to/model.zipzi 3 target/first-frame.rgba
```

It reports CPU texture preparation/upload calls, submission, and GPU completion.
The optional raw RGBA output permits byte comparison across builds. This offscreen
probe does not measure GPUI/window presentation; see the native
[first-frame guide](../../tools/benchmarks/README.md#viewport-first-frame).

## Build and test

For native Metal, run on macOS from the repository root:

```sh
cargo check --locked -p zipzi-editor-render
cargo test --locked -p zipzi-editor-render --lib
# Application-level Metal/rendering integration:
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --features test-support --test native_render
```

Use `./tools/test-editor.sh acceptance` for the broader native render/screenshot
suite. A successful compile does not establish GPU execution coverage; see
[test gates](../../tools/README.md#test-coverage).

Licensed **GPL-3.0-only**; not published to crates.io. The underlying SDK render
crates retain their own MIT licensing. See [component licensing](../../LICENSING.md).

## wgpu model adapter

The `wgpu-backend` feature exposes `WgpuSceneRenderer` with the same scene
snapshot, content-keyed texture cache and viewport inputs. `wgpu_preview` loads
an Editor model3 and captures three views through both model backends, reporting
pixel differences and geometry uploads. On macOS `WgpuMetalSceneRenderer` bridges
the completed wgpu output into the existing GPUI Metal surface without readback.
It waits for the wgpu queue on the rendering worker before publishing; this is a
correctness bridge, not yet a shared-event asynchronous presentation path.

```sh
cargo run --locked -p zipzi-editor-render --features wgpu-backend --example wgpu_preview -- \
  models/local/mao/runtime/mao_pro.model3.json target/wgpu-mao
```

Canvas geometry uses an ordered native instance stream on both platforms.
`canvas_overlay::CanvasDrawList` compiles Warp outlines/grids, Bezier controls,
Rotation bones, Glue, transform frames, reference/guide/image lines, origin axes,
marquee and even-odd lasso fills into lines, bordered round/square markers and
triangles. The palette and painter order are shared with the GPUI fallback.
Geometry and interaction remain in `zipzi-editor-core`; text and widgets remain
in GPUI. The normal path composites one immutable surface with one instanced draw,
using 4x MSAA plus analytic marker coverage. There is no normal-path GPUI geometry
painting or CPU readback.

`CanvasOverlayCache` belongs to each viewport and reuses prepared instances on
unchanged repaints. Each renderer reuses its texture for unchanged instances,
viewport and consumer queue. Hover, theme, view or geometry changes invalidate
the appropriate cache. Retained scenes keep immutable old textures across later
updates and renderer teardown. `canvas_stats` reports instances, upload bytes,
draw calls and texture-cache hits. Shader/device initialization failures use the
same prepared geometry through the GPUI fallback.

The older Mesh edge/vertex diagnostic APIs remain available on both
platforms: Metal (`overlay.rs`, `overlay.metal`) and D3D11 (`overlay_windows.rs`,
`overlay.hlsl`). They share instance packing/cache keys in `overlay_data.rs`,
including selection/hover/restriction flags and normal-before-emphasized ordering.
Edges use 4x MSAA; markers use analytic circle coverage. Marker hover recolors the inner border
without changing the six-logical-pixel silhouette; the shared radius and hover
color come from `zipzi-editor-core::canvas_style`. Separate edge and vertex
textures support the historical mesh-only comparison tests. Each published texture is
immutable, so a retained old scene can be redrawn after later poses or renderer
teardown. Shader/device failures retain the GPUI fallback.

The Windows overlay renderer is confined to its owning UI thread. It records on
a private deferred context and executes with state restoration on GPUI's same
D3D11 immediate context; ready/consumed fences use the existing surface contract.
No D3D12 copy or CPU readback is needed for these D3D11-native overlay textures.
`ZIPZI_TRACE_BATCH_MESH_EDGES=1` with tracing enabled selects the unified GPUI
canvas fallback for same-binary comparisons on both platforms. Trace metadata
includes `diagnostics.native_canvas_overlay`; historical `native_mesh_overlay`
and `metal_mesh_overlay` keys remain available. `canvas_overlay_draw` records
instance/upload/draw/cache counts; Windows `overlay_shader_surface` pass 0 is the
complete canvas surface. Detailed CPU spans include `overlay.canvas_prepare`,
`overlay.metal_canvas` / `overlay.d3d11_canvas`, and `overlay.gpui_canvas_fallback`.

Focused coverage:

```sh
cargo test --locked -p zipzi-editor-render --lib canvas_overlay
cargo test --locked -p zipzi-editor-render --test canvas_overlay_metal
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --features test-support --test native_render overlay::
# Explicit Release CPU comparison (GPUI emission vs prepared native submission):
env -u ZIPZI_DEV_SESSION_DIR cargo test --release --locked -p zipzi-editor --features test-support --test native_render native_canvas_cpu_submission_benchmark -- --ignored --nocapture
```

The Metal tests execute on a real GPU and cover themes, 1x/1.5x/2x DPI,
fractional origins, control states, translucent bones, even-odd fills, cache reuse
and retained surfaces. The native Editor tests compare complete surfaces with
the shared GPUI fallback and the pre-migration deformer painter. D3D11 has the
matching entry point, packed instances and Windows hardware tests. The
[Windows validation record](../../docs/archive/benchmarks/2026-11-05-windows-native-canvas-validation.md)
covers RTX 5060 hardware execution, pixel assertions and native-window workloads;
complete Editor-surface parity with the GPUI fallback remains a separate gate.
The explicit CPU benchmark writes `target/canvas-overlay-cpu.json`, with separate
P50/P95 preparation, legacy/shared GPUI emission and native encode/submission
times. It drains the queue outside timing to bound in-flight resources. This
headless measurement excludes model evaluation, OS input, display and readback;
it does not establish input-to-display latency.

Windows model previews use `D3d11Preview` with GPUI's D3D11 device. The worker
records model commands on a deferred context, and the UI executes the list on
GPUI's immediate context before sampling the output texture. Each list
establishes its own mask and offscreen contents so a coalesced frame can be
dropped safely. Output slots remain immutable while retained by a GPUI scene
or still in use by its consumer fence.

Build the Editor with `--features wgpu-backend` to compare the two Windows
preview paths in the same binary. This build selects the wgpu preview by
default. Set `ZIPZI_PREVIEW_BACKEND=d3d11` to select native D3D11 in that build.
The wgpu path uses the D3D12-to-D3D11 shared texture bridge in
`src/wgpu_windows.rs`. The Editor selects
the D3D12 adapter by the GPUI device's LUID, renders to a normal wgpu RGBA8 target,
then submits one native GPU copy into a shared committed texture. The source's
COPY_SRC transition goes through wgpu's tracker; the native destination returns
to COMMON before D3D11 samples it. No pixel readback or image-atlas upload occurs.

Each immutable surface has separate ready/consumed shared fence timelines. GPUI
waits on ready on its GPU queue and signals consumed after every actual draw,
including retained redraws. Reuse requires exclusive pool ownership AND both
fences completed. Each presenter bounds shared textures to eight slots / 256 MiB,
with at most 32 MiB per frame so retained frames leave room for replacement slots.
The main preview fits larger targets to this pixel budget by reducing raster DPI;
logical viewport coordinates and interaction remain unchanged. Direct acquisitions
above the per-frame budget fail immediately instead of entering backpressure.
Source textures and render attachments are separate allocations. Main preview
requests coalesce during backpressure; candidate previews report budget exhaustion.
Changing dimensions retires reusable old sizes. Pending copies retire on a separate
thread when their presenter closes, keeping native resources and the wgpu backend
alive without a UI-thread wait. Device identity changes rebuild the
preview worker; GPUI skips old-device surfaces. Unsupported sharing/fences produce
an explicit preview error rather than silently falling back to CPU pixels.

`WgpuImageReadback` remains available for export, tests and baseline measurements;
it is no longer the Windows presentation mechanism. It converts premultiplied
linear RGBA/BGRA to straight BGRA on the GPU before mapping a tightly packed buffer.
Its existing exhaustive conversion test and benchmark remain useful independently.

```powershell
cargo test --locked -p zipzi-editor-render --features wgpu-backend --test windows_shared_texture --test windows_overlay -- --nocapture
```

The overlay hardware tests check all eight selection/hover/restriction states
against the Metal marker fragment formula at 1x/1.5x/2x DPI and fractional origins.
They also check MSAA edge output, edge/control/marker layering, retained immutable
frames, cached draws and empty/non-mesh inputs. Readback is confined to these tests.

The shared-texture hardware test uses the production GPUI Windows Fast pipeline offscreen and
reads back only the final test target. It checks premultiplied alpha, opacity,
clipping, subsequent UI primitives, repeated draws, retained old frames, resizing,
bounded-pool exhaustion/reuse, real model output, retained consumer frames after
producer teardown, and closing while a GPU copy is deliberately stalled. It does
not measure DWM or display latency.

To verify the optional Windows wgpu scene adapter on a real GPU independently
of GPUI:

```powershell
cargo test --locked -p zipzi-editor-render --features wgpu-backend --test wgpu_readback -- --nocapture
```

This test requires a GPU, asserts pixels and frame immutability, and reports the
adapter/backend. It also exhausts all 256×256 channel/alpha combinations for RGBA
and BGRA sources, including zero alpha, saturation, odd row widths and staging
reuse/resizing. It does not establish native window performance. The existing
Metal comparison examples explicitly require macOS.

For a paired worker-stage comparison with the former CPU conversion path:

```powershell
cargo run --locked -p zipzi-editor-render --features wgpu-backend --example wgpu_readback_benchmark -- 1061 1478 60
cargo run --locked --release -p zipzi-editor-render --features wgpu-backend --example wgpu_readback_benchmark -- 1061 1478 60
```

The harness warms up 20 frames, alternates readback order, changes the view each
frame and requires byte-identical output from both paths. CSV separates CPU
submission, readback and CPU conversion; the new readback includes GPU conversion.
Readback also includes any pending render completion when that path runs first.
GPUI upload, presentation and OS input latency are excluded. See the
[Windows P5 record](../../docs/archive/plans/windows-development.md#p5-回读成本与未完成项)
for paired results and separate native-window evidence.
