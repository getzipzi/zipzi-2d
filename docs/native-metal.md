# Native Metal integration

Native Metal is the sole editor rendering path. No backend flags are needed.
`editor-render` owns this adapter; `editor-core` supplies SDK-evaluated frames
and immutable decoded source textures without depending on Metal or GPUI.

```sh
cargo run --locked -p kasane-editor
cargo test --locked --workspace
./scripts/build-app.sh
```

## Integration

GPUI retains its original native Metal renderer and AppKit/IME implementation.
Window/PlatformWindow exposes that renderer's MTLDevice and MTLCommandQueue.
Kasane uses that device and calls `MetalRenderer::encode` on a command buffer from
**GPUI's queue**. `MetalContext::from_device` creates a convenience queue, but the
editor deliberately does not submit to it.

Each update publishes an immutable texture after command submission. A later GPUI
submission on the same queue samples it. No CPU readback or GPU completion wait is
performed in preview production. Scene ownership retains old textures for replay.
Each worker frame drains a Metal autorelease pool.

Source textures come from the document's validated PNG assets, not a renderer-owned
placeholder. Decoded pixels are shared across snapshots; the Metal adapter caches
uploads by asset ID, content hash and dimensions. It uploads a straight-RGBA mip chain
for each texture; the Metal sampler uses linear minification, magnification and mip
filtering. A monotonically increasing preview request revision prevents old frames
from replacing a newer edit or opened project. The viewport fits the document's
actual canvas dimensions.

`SurfaceSource::MetalTexture` uses GPUI's existing Surface vertex/clip path with a
small premultiplied RGBA fragment shader. It applies element opacity once and blends
in scene order. CoreVideo YUV surfaces keep their original pipeline. Kit components
and macOS backend selection are unchanged. The three required patches are listed
in `../vendor/README.md`.

## Tests and limits

`crates/editor/tests/native_metal.rs` uses the real native GPUI headless renderer
and Kasane Metal renderer. It checks colors, combined document/element opacity,
content masks, UI overlay order, hidden content, immutable previous frames, resizing,
and transparent RGBA/BGRA targets. Only the final output is read back for assertions;
there is no intermediate readback between renderers.

The user has confirmed that the native Metal editor works. Detailed UI/IME and
window lifecycle verification remain manual; no computer-use testing is performed.

The preview resolution follows the canvas area and display scale. Output frame textures
and Kasane's internal geometry/attachments are not pooled. Frame-rate and memory
performance have not been benchmarked. The
local Kasane checkout must include the new native backend and accompanying changes;
the earlier pinned revision alone is insufficient.
