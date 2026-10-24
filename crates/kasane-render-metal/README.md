# kasane-render-metal

Native macOS Metal renderer for Kasane `ScenePlan`. It calls `MTLDevice`,
`MTLRenderCommandEncoder` and `MTLBlitCommandEncoder` through `metal-rs`, and
compiles the included MSL shader directly. No wgpu device or pipeline is used
by this crate. The CPU geometry, mask layout and scene validation live in
`kasane-render` and are shared with `kasane-render-wgpu`.

Create `MetalContext` with the system device or an existing host `MTLDevice`.
Upload source images with `upload_rgba8`, build a `MetalTextureCatalog`, then
call `MetalRenderer::sync_model` (or `sync_model_shared` for an `Arc<DrawableFrame>`),
`update_view` and `encode`. The output may be
an offscreen `MTLTexture` or a `CAMetalDrawable` texture from the same device.
`upload_rgba8` accepts straight RGBA pixels and mip levels, converting each to
premultiplied GPU storage before filtering. Externally supplied catalog textures
must also contain premultiplied RGBA. Authored PNG/RGBA assets remain straight;
use `kasane_render::texture::straight_rgba_mipmaps` for alpha-weighted mip levels.
The host commits the command buffer after encoding and uses the **same command
queue for each renderer's lifetime**. Cached mask writes and subsequent reads
depend on queue ordering. `render` is a convenience method that commits one frame.

The renderer supports normal, additive and multiplicative drawing, raw blend
modes with destination snapshots, alpha and inverted masks, nested offscreen
targets, texture repeat and mipmapped sampling, resize, and Replace/Composite
presentation. Consecutive draws to a target share a render pass; destination
snapshots end the pass before the blit. Replace renders directly into the host
output; Composite preserves the host background with an intermediate texture.
Offscreen children are rendered and composited in draw order. Each group's
visible geometry bounds are transformed to screen space, intersected with the
viewport, and rounded outward to whole pixels. Nested groups may have different
crop origins and extents; their completed images are translated on the same
screen pixel grid. Mask lookup stays in model space, and destination snapshots
retain the current parent target's extent and draw order. Hidden raw mask
sources keep their own bounds without expanding a group's color surface.

Color textures and destination snapshots reuse exact-size storage within each
command buffer, ending the previous render pass before a texture is written
again. A dry run follows the same allocation/release order before encoding;
when many different crop sizes would allocate more storage than a common
scene-wide extent, the common extent is used instead. The 512 MiB attachment
budget covers the resulting allocations, snapshots, presentation storage and
masks. Pools are local to a command so submitted frames remain independent.

Mesh buffers are immutable and retained across view changes. Model submissions
replace only buffers whose geometry changed, so earlier submitted frames keep
their original data. Changed meshes share packed vertex/index slabs (normally
up to 4 MiB each), avoiding a Metal allocation per drawable during deformation.
Mask lookup and layout use `ScenePlan`'s indexed sources and cached bounds.
Scenes with at least 32 logical masks pack active masks into atlas shelves
(normally up to 1024×1024 per page), preserving each mask's pixel density and
padding. Scissoring isolates writes and sampling clamps within each tile.
A changed source invalidates the atlas as a unit; unchanged atlases remain
cached. Pages are reused when their sizes match, with writes ordered after
previous frames by the command queue. Oversized masks get dedicated pages;
if shelf storage would exceed the attachment budget, rendering falls back to
individual masks. Small scenes retain individual-mask caching.

Raw masks are shared by consumers and cached across frames
using geometry, physical mask layout, texture identity/revision, and repeat mode.
Their canvas-space bounds are intersected with the inverse-transformed visible
viewport, including a 64-screen-pixel apron snapped to a grid and the usual
filtering border. Fully offscreen sources use cleared one-texel masks, preserving
inverted-mask coverage. Cropped origin and extent participate in cache identity,
so pans and resizes cannot reuse coverage from a different region. Small pans
within the same crop reuse coverage.
At full mask density, crop origins also align to the source's mask texel grid,
so crossing a crop boundary does not shift filtered edges during a pan.
Mask density rounds up to powers of two to share masks across nearby zoom levels,
falling back to exact density when the larger masks exceed the attachment budget.
If visible masks still exceed the budget, their maximum edge is halved from
4096 until the frame fits. This also reduces Offscreen masks below their usual
minimum density when necessary. Color storage that exceeds the budget even with
one-texel masks still fails before encoding.
Give textures
monotonically increasing revisions with `set_revision` when their contents
change. Unversioned textures redraw masks every frame. Unused mask entries are
evicted after encoding; source textures are retained while their identities are
cached. Output and offscreen textures remain per-frame allocations.

`MetalRenderStats` reports draw calls, render passes, mask redraws/cache hits and
buffer uploads. `color_attachment_bytes` reports the color/snapshot storage
allocated for that command, excluding the host output and masks. A warm pan
requires no geometry upload. `attachment_budget_bytes` reports the conservative
reservation including host output, masks and atlas packing overhead;
`mask_resolution_limit` reports the mask edge limit chosen for the frame.

On macOS, `kasane-sdk-observe` and the Python `observe` feature use this
native backend. Other platforms keep `kasane-render-wgpu`. Python builds
without `observe` do not include a GPU renderer.

```sh
cargo test -p kasane-render-metal --test metal_smoke --locked
cargo test -p kasane-sdk-observe --locked
```

For the group-crop A/B benchmark and pixel comparison, run:

```sh
uv run --locked python tools/benchmarks/benchmark_offscreens.py --rounds 3
```

This alternates the default crop policy with the `benchmark-global-surfaces`
feature, which forces the previous common-surface strategy while retaining the
same traversal and preflight. Keep that feature disabled in application builds.
Each run warms 20 frames and measures 90 frames at 2048×2048. Reports include
completed-command GPU timestamps, CPU encoding time, attachment bytes, and raw
RGBA captures; comparisons fail above a one-byte channel difference. Related
[benchmark entrypoints](../../tools/README.md#benchmarks-and-research-tools) are
listed in the tool guide.
