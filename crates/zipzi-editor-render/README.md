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
background WGPU readback-to-image bridge in `zipzi-editor/src/preview/readback.rs`.

On Windows, the Editor enables `wgpu-backend` automatically. To verify the
scene adapter on a real GPU independently of GPUI:

```powershell
cargo test --locked -p zipzi-editor-render --features wgpu-backend --test wgpu_readback -- --nocapture
```

This test requires a GPU, asserts pixels and frame immutability, and reports the
adapter/backend. It does not establish native window performance. The existing
Metal comparison examples explicitly require macOS.
