# zipzi-render-metal

Native macOS Metal renderer for `ScenePlan`. Geometry, mask layout and scene
validation are shared with [`zipzi-render`](../zipzi-render/README.md).

Create a `MetalContext`, upload images with `upload_rgba8`, and build a
`MetalTextureCatalog`. Call `MetalRenderer::sync_model` (or `sync_model_shared`),
`update_view` and `encode`, then commit the command buffer. Output may be an
`MTLTexture` or a `CAMetalDrawable` from the same device. Use the same command
queue for a renderer's lifetime so cached mask reads and writes stay ordered.
`render` is a convenience method that commits one frame.

`upload_rgba8` accepts straight RGBA pixels and premultiplies them for GPU
storage. External catalog textures must already be premultiplied. Use
`zipzi_render::texture::straight_rgba_mipmaps` for authored image mipmaps.
Assign increasing texture revisions when content changes; unversioned textures
redraw masks every frame.

The renderer supports blend modes, masks, nested offscreens, repeat and mipmap
sampling, resizing, and Replace/Composite output. Geometry and masks are cached
across frames. Attachments share a 512 MiB budget; mask resolution can fall back
when needed. `MetalRenderStats` reports draws, passes, uploads, cache use and
attachment allocation.

On macOS, `zipzi-observe` uses Metal by default; its `wgpu-backend` feature
selects the [wgpu renderer](../zipzi-render-wgpu/README.md).

## Tests

Requires macOS and a usable Metal device:

```sh
cargo test --locked -p zipzi-render-metal --test metal_smoke
cargo test --locked -p zipzi-observe
```

For the offscreen crop benchmark and pixel comparison:

```sh
uv run --locked python tools/benchmarks/benchmark_offscreens.py --rounds 3
```

The benchmark uses `benchmark-global-surfaces` for comparison; leave that feature
disabled in application builds. See the [tool guide](../../tools/README.md#benchmarks-and-research-tools)
for other benchmarks.
