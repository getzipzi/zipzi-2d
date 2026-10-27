# zipzi-editor-render

Native Metal preview adapter for the Editor. It consumes
[`zipzi-editor-core`](../zipzi-editor-core/README.md) scene snapshots and viewport transforms,
and delegates model rendering to [`zipzi-render-metal`](../zipzi-render-metal/README.md).
Document editing and project I/O remain outside this crate.

## Rendering boundary

`MetalSceneRenderer` uploads and caches scene textures, synchronizes shared model
frames, applies viewport transforms and submits rendering on the consumer's Metal
queue. `SubmittedFrame` retains the output texture and command buffer so callers
can track GPU completion without blocking the UI thread. `MetalOverlayRenderer`
and the overlay types draw Editor interaction overlays.

The implementation is in `src/lib.rs` and `src/overlay.rs`. Backend-independent
render planning belongs in `zipzi-render`; reusable Metal rendering belongs in
`zipzi-render-metal`. This crate adapts those facilities to Editor scene and
interaction data.

## Build and test

Requires macOS and Metal. Run from the repository root:

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

Overlay rendering and GPUI platform integration remain native Metal.
