# zipzi-render-wgpu

Cross-platform `ScenePlan` renderer using wgpu. `WgpuContext` creates a device,
queue, premultiplied source textures and readable output textures. Hosts may also
provide their own device, queue and texture views.

Call `sync_model` or `sync_model_shared`, `update_view`, then `encode`. Submit each
encoder before encoding the next frame with that renderer, on the same queue.
Model snapshots and packed geometry slabs are immutable; changing the view does
not upload geometry. `sync_model_shared` retains an `Arc<DrawableFrame>`.

Source views must contain **premultiplied RGBA8/BGRA8**. `upload_rgba8` accepts
straight authored RGBA and optional mip levels from
`zipzi_render::texture::straight_rgba_mipmaps`, validating and premultiplying each
level before upload. Give catalog entries monotonic revisions when their content
changes; unversioned sources redraw masks on every encoding. Repeat sampling is
controlled per catalog entry.

The renderer supports normal/additive/multiplicative and raw extended blends,
normal/inverted raw masks, nested offscreens, mipmaps, culling, resize, and
Replace/Composite output. Group crops share the screen pixel grid; command-local
color/snapshot pools follow the same allocation traversal as their preflight.
Mask coverage uses viewport crops, snapped densities, source geometry/texture
revisions and atlases for large mask sets. The 512 MiB attachment budget includes
host output, color targets, masks and conservative atlas overhead. Mask resolution
falls back when necessary. A private main color target supports destination reads
when the host only supplies a texture view without `COPY_SRC` usage.
Replace output renders directly to the host when the main target neither reads
its destination nor samples the host texture. Composite output keeps the private
target. Device creation uses the adapter's texture resolution limits.

`WgpuRenderStats` reports GPU draws/passes, attachment bytes, mask hits/redraws,
and geometry uploads. Uniforms are immutable per draw so commands do not observe
later queue writes. `read_rgba8` handles padded rows and converts BGRA output into
RGBA bytes.
Immutable source/mask bindings are reused across frames and evicted when unused.
Transient color and destination snapshots are excluded from this cache. Geometry
is packed explicitly as XY floats, independent of Rust's `Vec2` memory layout.

## Local GPU checks

```sh
cargo test --locked -p zipzi-render-wgpu --test gpu_smoke
cargo test --locked -p zipzi-render-wgpu --test cache_contract
# macOS: compare identical inputs through wgpu and native Metal
cargo test --locked -p zipzi-render-wgpu --test parity
cargo run --locked -p zipzi-render-wgpu --example offscreen -- target/wgpu-minimal.png
uv run --locked python tools/validation/compare_wgpu_blends.py
```

The parity gate covers filtered transparency, all raw color/alpha combinations,
inverted masks, nested cropped groups, atlas invalidation and winding. It requires
a working GPU and does not silently skip unavailable adapters. The
`benchmark-global-surfaces` feature selects common group extents for crop A/B
comparisons; leave it disabled in hosts.

## Application routes on macOS

```sh
cargo test --locked -p zipzi-observe --features wgpu-backend --test capture
cargo run --locked -p zipzi-editor --features wgpu-backend
cargo run --locked -p zipzi-editor-render --features wgpu-backend --example wgpu_preview -- \
  models/local/mao/runtime/mao_pro.model3.json target/wgpu-mao
```

`wgpu-backend` selects wgpu observation on macOS and is forwarded by Runtime,
Editor and the Python native adapter. Non-macOS observation uses wgpu by default.
The Editor feature selects wgpu for model previews. Its current macOS/GPUI bridge
retains the native output texture after wgpu completion on the rendering worker;
there is no image readback or UI-thread wait. GPUI window rendering and interaction
overlays still use native Metal. Windows/Linux window/overlay integration and
platform acceptance remain separate work.

## Backend benchmark on macOS

```sh
cargo run --locked --release -p zipzi-editor-render --features wgpu-backend \
  --example wgpu_benchmark -- models/local/mao/runtime/mao_pro.model3.json \
  target/wgpu-mao.csv 90 3
```

The harness fits the full model using the Editor preview's view, then compares
the same scene through wgpu and native Metal at 1280×800: idle, pan, and synthetic
vertex deformation. Each case has 20 warmup
frames and alternates backend order between rounds. CSV records CPU submission,
completed-frame wall time and allocation/pass counters. It excludes loading,
scene mutation and readback; completed-frame time includes queue scheduling and
CPU waits, and is not a GPU timestamp or full Editor interaction latency.

[Local review and acceptance evidence](../../docs/archive/wgpu-review-2026-10-28.md)
records the tested revision, pixel gates, before/after measurements and limits.
