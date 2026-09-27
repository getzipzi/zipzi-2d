# Native Metal integration

Native Metal is the sole editor rendering path. No backend flags are needed.

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

The preview remains 768×768. Frame textures and Kasane's internal geometry/attachments
are not pooled. Frame-rate and memory performance have not been benchmarked. The
local Kasane checkout must include the new native backend and accompanying changes;
the earlier pinned revision alone is insufficient.
