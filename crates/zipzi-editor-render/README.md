# zipzi-editor-render

Metal and WGPU scene adapters for the Editor. They consume
[`zipzi-editor-core`](../zipzi-editor-core/README.md) scene snapshots and viewport transforms,
and delegate model rendering to the shared Metal/WGPU backends.
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

macOS overlays remain native Metal. Windows uses GPUI paths/markers and a
D3D12-to-D3D11 shared texture bridge in `src/wgpu_windows.rs`. The Editor selects
the D3D12 adapter by the GPUI device's LUID, renders to a normal wgpu RGBA8 target,
then submits one native GPU copy into a shared committed texture. The source's
COPY_SRC transition goes through wgpu's tracker; the native destination returns
to COMMON before D3D11 samples it. No pixel readback or image-atlas upload occurs.

Each immutable surface has separate ready/consumed shared fence timelines. GPUI
waits on ready on its GPU queue and signals consumed after every actual draw,
including retained redraws. Reuse requires exclusive pool ownership AND both
fences completed. Each presenter bounds shared textures to eight slots / 256 MiB;
source textures and render attachments are separate allocations. Main preview
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
cargo test --locked -p zipzi-editor-render --features wgpu-backend --test windows_shared_texture -- --nocapture
```

This hardware test uses the production GPUI Windows Fast pipeline offscreen and
reads back only the final test target. It checks premultiplied alpha, opacity,
clipping, subsequent UI primitives, repeated draws, retained old frames, resizing,
bounded-pool exhaustion/reuse, real model output, retained consumer frames after
producer teardown, and closing while a GPU copy is deliberately stalled. It does
not measure DWM or display latency.

On Windows, the Editor enables `wgpu-backend` automatically. To verify the
scene adapter on a real GPU independently of GPUI:

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
[Windows P5 record](../../docs/plans/windows-development.md#p5-回读成本与未完成项)
for paired results and separate native-window evidence.
