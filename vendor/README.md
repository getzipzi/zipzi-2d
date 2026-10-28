# Local GPUI patches

This directory contains the GPUI dependencies and patches used by the Editor.
Keep upstream licenses, original package metadata and patch provenance when
updating these sources.

## Dependencies

| Dependency | Source |
| --- | --- |
| GPUI Kit 0.7.3 | [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit), pinned to `507e82b8115ac73af92b7d3856501f9d7b73b2c9`; local `gpui-component` patches are described under [compact menus and tooltips](#kit-compact-menus-and-tooltips). |
| GPUI Fast 0.1.7 | Local patches for `gpui-fast`, `gpui-fast-macos`, `gpui-fast-apple` and `gpui-fast-windows`; see the [native Metal changes](#native-metal-changes-by-file). |
| Zipzi | SDK sources under [`crates/`](../crates), built from the same repository commit and root `Cargo.lock`. |
| Metal 0.33 | crates.io |

Sources and overrides are defined in [Cargo.toml](../Cargo.toml). All workspace
crates share [Cargo.lock](../Cargo.lock); build and test with `--locked`.
Zipzi uses local path dependencies, so Cargo.lock does not pin their source
revision. Use one checkout for all Zipzi crates, including
`zipzi-sdk-observe` for tests. No sibling GPUI checkout is needed.

The workspace enables `gpui-fast` on GPUI Kit and the optional FPS monitor.
The local Fast patches support native Metal surfaces, shared queues, headless
rendering and frame tracing. Kit retains upstream `gpui-pre` 0.3.9 dependencies
in its graph; its Fast feature selects the Fast re-exports used by the Editor.

`gpui-diagnostics.patch` adds opt-in cross-platform CPU frame boundaries to
`gpui-fast/src/window.rs`: root build, prepaint, paint, image-atlas lookup/upload,
and the platform draw call. Hooks use the existing frame-trace sink; atlas timing
does not read clocks when tracing is disabled. `platform_draw_*` measures the CPU
call (including synchronous driver/Present work on Windows), not GPU execution or
screen presentation. The image event retains cache-hit/miss, byte count and CPU
frame identity. It does not change invalidation, rendering or presentation policy.
Test renderers also use `gpui-fast-apple`.

The Fast backend and local cache-boundary fix are the accepted default. See the
[Kit comparison and adoption decision](../docs/archive/gpui-kit-old-vs-fast.md)
for CPU measurements and presentation-latency tradeoffs.

## Owned path scaling

[path-buffer-reuse.patch](path-buffer-reuse.patch) adds `Path::into_scaled` in
`gpui-fast/src/scene.rs` and uses it from `Window::paint_path`. The owning vertex
iterator lets Rust collect into the original allocation, avoiding a second
buffer for every painted path. The existing borrowed `Path::scale` remains
available; coordinates, content masks, color, bounds and snapping are unchanged.
No raw pointer casts or unsafe allocation conversion are used.

The Editor's `mesh_edges::tests::owned_path_scaling_reuses_vertices_and_preserves_geometry`
checks buffer identity/capacity and exact equality against borrowed scaling at
fractional and integer DPI factors, including large buffers and empty paths.
Keep this regression when upgrading Rust or GPUI because allocation reuse is a
standard-library optimization. Apply this patch after the existing GPUI patches:

```sh
git apply --reverse --check vendor/path-buffer-reuse.patch
```

## Accessibility prepaint transactions

`gpui-fast/src/window.rs`, `window/a11y.rs` and `fast/a11y.rs` roll back
accessibility nodes and retained logs when a virtual list retries speculative
prepaint for autoscroll. The rollback restores open ancestors and focus, removes
discarded node IDs and bounds, and invalidates abandoned retained log ranges.
Without it, expanding Properties details can panic with `Duplicate a11y node id`.
This restores the source fix removed in `f032d769`, with its regression in the
Editor workspace so the root lockfile and normal GPUI feature selection apply:

```sh
cargo test --locked -p zipzi-editor -p zipzi-editor-core -p zipzi-editor-render \
  --features zipzi-editor/test-support --test gpui_prepaint_rollback
```

Reapply these changes when updating GPUI Fast. Mesh selection/Details interaction
coverage lives in `zipzi-editor/headless_tests/mesh_properties.rs`; its ignored Mao
case requires the separately obtained local model.

## Upstream baselines

The four Fast packages come from the crates.io **0.1.7** releases and provide
the `gpui-fast` backend for GPUI Kit 0.7.3. The root `[patch.crates-io]` section
selects these local packages. Kit is pinned to the upstream commit above;
a separate Git-source `[patch]` overrides `gpui-component`.

`Cargo.toml.orig` preserves each published package's original workspace manifest.
Builds use the normalized `Cargo.toml`. Retain the Apache-2.0 license files.

The native Metal integration patch covers 11 files (10 source files and one
manifest), with 495 added and 13 removed lines. The cache-boundary patch changes
`fast/retained.rs` and `fast/splice.rs`; the list-height patch changes
`elements/list.rs`. Together these changes cover 14 files. See the individual
patches and the maintenance procedure below when checking the upstream baseline.

## Drag identity and targeted cancellation

[drag-ownership.patch](drag-ownership.patch) adds `ActiveDragToken`,
`App::active_drag_token` and `App::stop_drag` to `gpui-fast`. An identity is created
for internal drags and incoming platform file drags, and survives platform
handoffs. Weak snapshots do not extend the lifetime of drag resources.
Cancellation checks the identity, so a stale token cannot stop a later drag.
The existing unconditional `stop_active_drag` API remains available.

Timeline number scrubbing holds this token directly, replacing the Editor's
global DragOwner. Regressions in
[input_routing.rs](../crates/zipzi-editor/headless_tests/input_routing.rs)
exercise real GPUI drag replacement and repeated cancellation. `AnyDrag` gains
an internal identity field; downstream code constructing it directly needs
adaptation. Recheck and apply this patch after the existing local patches when
upgrading GPUI.

## Kit compact menus and tooltips

`gpui-kit/crates/component` comes from the pinned Kit commit and retains its
complete package, upstream manifest and Apache-2.0 license. The reduced
`gpui-kit/Cargo.toml` supplies inherited dependencies and lints. Kit's base,
assets and macros packages still come from the same Git commit; the whole Kit
repository is not vendored. Its README and five files referenced by upstream
tests through relative paths are retained without building additional local
packages.

[kit-workspace.patch](kit-workspace.patch) records the complete workspace-manifest
difference from the pinned commit: reduced membership, inherited dependencies
and matching Git revisions. Its baseline is Kit's original root `Cargo.toml`;
its target is `vendor/gpui-kit/Cargo.toml`.

### Compact menus and Selects

[compact-menu.patch](compact-menu.patch) changes `menu/popup_menu.rs`:

- Implement `Sizable` to expose the existing Small density.
- Keep the 20px baseline for ordinary Small rows, scaled with the window's rem
  size. Other densities retain upstream behavior.
- Use `text_xs()` for Small menu items: 12px at 100% UI scale.
- Inherit the parent menu size when rendering submenus, covering both
  `submenu()` and `PopupMenuItem::submenu()`, including sizes set after submenu
  construction.

Horizontal padding remains 8px; Small corner radii use the existing halved-radius
rule. Custom content keeps a minimum row height, allowing project titles and
paths to occupy multiple lines. The Editor uses `ui::compact::menu` and
`ui::compact::select` as shared entrypoints. Selects use native XSmall sizing:
12px text for triggers, options and search fields, a 22px trigger height and
Kit's existing 4px vertical option padding. Fonts follow the existing UI scale.
These patches retain the existing interaction behavior and do not modify
sibling checkouts.

### Select invalidation and open state

[select-invalidation.patch](select-invalidation.patch) makes
`ListState::set_searchable` a no-op when its value is unchanged. Select rendering
sets this flag repeatedly; notifying on an unchanged value invalidates otherwise
retained Properties panels during unrelated canvas frames.

The patch also exposes `SelectState::is_open()`. The Editor reuses closed resource
controls across revision changes, while open Selects rebuild their choices so
locks, history and external edits cannot leave stale options. Structural and
functional regressions live in
[ui_performance.rs](../crates/zipzi-editor/headless_tests/ui_performance.rs).

### Tooltips

[compact-tooltip.patch](compact-tooltip.patch) changes `tooltip.rs` and `root.rs`:
11px text and shortcuts, 14px line height, 6px horizontal and 1px vertical padding,
and a 6px gap between text and shortcuts. All sizes use rem units to follow UI
scaling. Shared overlays and native GPUI tooltips reuse the same content
component. Text tooltips wrap at 360 logical pixels (scaled with rem) and shrink
to the window width with room for popup margins. Content can shrink within the
row instead of extending beyond the viewport.

The shared overlay tracks the triggering element's full GPUI identity per
window. Leaving an old button cannot close a newer button's tooltip or cancel
its pending display. Identity is captured during prepaint and matched across
redraws, avoiding dependence on hover-callback order.

[tooltips.rs](../crates/zipzi-editor/headless_tests/tooltips.rs) covers repeated
switching in both directions, switching before first display, staying beyond
the hide grace period, normal hiding on departure, click dismissal and pending
display cancellation, plus long English/Chinese Mesh Edit and deformer
descriptions in narrow windows at multiple UI scales. Kit's standard `test_support()` interface exposes the outer
tooltip for tests; observation is a no-op in production builds.

[compact_popups.rs](../crates/zipzi-editor/headless_tests/compact_popups.rs)
covers light/dark themes, three UI scales, both submenu constructors, multiline
content and ordinary/searchable Selects in real Editor dialogs. Existing Dock,
Canvas and parameter-management tests cover context-menu actions. Native Metal
screenshots use `ZIPZI_SCREENSHOT_SCENE=compact-popups`.

### Dock resize handles

[dock-handle-line.patch](dock-handle-line.patch) adds
`DockSkin::with_resize_handle_line(false)`. Set it before installing the skin to
hide persistent lines on internal splits and outer dock handles while retaining
capsule animation, hit regions and dragging. Lines remain enabled by default.

The Editor disables them so panel gaps retain the workspace background. Without
the line, vertical capsules shift 0.5px left and horizontal capsules shift 0.5px
up to align with the gaps. Timeline Metal screenshots check that no extra
horizontal line appears in the bottom gap.

### Updating Kit

Replace the component package, README, five test-support files and root workspace
manifest with their counterparts from the new pinned commit. Adapt and apply
`kit-workspace.patch`, `compact-menu.patch`, `compact-tooltip.patch` and
`dock-handle-line.patch` and `select-invalidation.patch`. Update Git revisions in
the reduced workspace, reassess whether each patch is still needed, regenerate
patches against the new baseline,
and update the root dependencies and lockfile.

```sh
git apply --reverse --check vendor/kit-workspace.patch
git apply --reverse --check vendor/compact-menu.patch
git apply --reverse --check vendor/compact-tooltip.patch
git apply --reverse --check vendor/dock-handle-line.patch
git apply --reverse --check vendor/select-invalidation.patch
cargo test --locked -p zipzi-editor --lib headless_tests::compact_popups
cargo test --locked -p zipzi-editor --lib headless_tests::tooltips
```

## Fast virtual-list cache boundaries

[cache-boundary.patch](cache-boundary.patch) makes `RetainedSubtree` store an
explicit `.cached()` identity. It is set when creating the cache record and
preserved through cross-frame reuse and splice copies. `is_cached()` no longer
infers it from an optional `Rebuild`.

A virtual-list item remains a cache boundary even without a parent layout key.
Unnotified state writes in its descendants no longer trigger unnecessary
rebuilds. Notifications, full refreshes and other existing invalidation rules
still apply. Production code contains no experimental environment variables or
diagnostic logging.

[view_cache.rs](../crates/zipzi-editor/headless_tests/view_cache.rs) covers
retention on/off, cached virtual-list rows and ordinary descendants, unnotified
updates, notified redraws, unchanged sibling reuse and full refreshes. These
regressions fail before the fix and pass after it. See the
[fix validation](../docs/archive/gpui-fast-cache-boundary-fix.md) for performance
and window-latency measurements.

## Virtual-list size changes

[list-height-estimates.patch](list-height-estimates.patch) preserves the previous
height or a caller-provided estimate when a width change marks list items as
unmeasured. Visible rows are remeasured at the new width; offscreen rows retain
their estimates. This prevents the scroll range from collapsing, the scrollbar
from shrinking and the last row from becoming unreachable.

The regression is
`variable_list_resize_preserves_scroll_estimates_and_remeasures_visible_rows` in
[view_cache.rs](../crates/zipzi-editor/headless_tests/view_cache.rs). Timeline
Metal acceptance also checks that the final row of a real Motion remains
reachable.

## Native Metal changes by file

[native-metal.patch](native-metal.patch) connects Editor render output directly
to GPUI's Metal compositor.

| File | Changes | Purpose |
| --- | --- | --- |
| [gpui-fast/Cargo.toml](gpui-fast/Cargo.toml) | Add the macOS-only `metal = "0.33"` dependency. | Hold native device, queue and texture types in the core interface. |
| [gpui-fast/src/platform.rs](gpui-fast/src/platform.rs) | Add macOS-only `SharedMetalContext { device, queue }` and `PlatformWindow::metal_context()`, defaulting to `None`. | Expose the renderer's actual resources. |
| [gpui-fast/src/platform/test/window.rs](gpui-fast/src/platform/test/window.rs) | Forward test-window context access to its offscreen renderer; add the default method to `PlatformHeadlessRenderer` and return actual resources from `MetalHeadlessRenderer`. | Use production asynchronous previews and Metal overlays in headless Editor screenshots. |
| [gpui-fast/src/window.rs](gpui-fast/src/window.rs) | Forward `Window::metal_context()`; extend `paint_surface()` from `CFRetained<CVPixelBuffer>` to `impl Into<SurfaceSource>`; record source and element opacity; optionally trace input/CPU-draw boundaries and scene IDs; allow canvas suppression of Space auto-repeat; remove the unused CVPixelBuffer import. | Accept native textures alongside CoreVideo buffers, retain element opacity and avoid redundant draws during Space-drag. |
| [gpui-fast/src/elements/surface.rs](gpui-fast/src/elements/surface.rs) | Add `SurfaceSource::MetalTexture(metal::Texture)`, use texture dimensions with existing object-fit behavior, and implement equality manually. Metal textures compare resource addresses; CoreVideo keeps its existing comparison. | Support ordinary RGBA/BGRA surfaces without requiring `metal::Texture` to implement `Eq`. |
| [gpui-fast/src/gpui.rs](gpui-fast/src/gpui.rs), [frame_trace.rs](gpui-fast/src/frame_trace.rs) | Export optional timing sinks, a shared host clock, process-local IDs and nested input scopes; allocate no events and read no clock while disabled. | Share timing and IDs between the Editor and platform renderer while keeping logging policy in the Editor. |
| [gpui-fast/src/scene.rs](gpui-fast/src/scene.rs) | Replace `PaintSurface.image_buffer` with `source: SurfaceSource`, add `opacity: f32`, and add an optional `frame_trace_id` to Scene. | Retain texture ownership and carry opacity through replay, batching and composition. |
| [gpui-fast-macos/src/window.rs](gpui-fast-macos/src/window.rs) | Add a four-line `PlatformWindow::metal_context()` implementation forwarding to the native renderer. | Connect the platform interface to the window's actual renderer. |
| [gpui-fast-apple/src/metal_renderer.rs](gpui-fast-apple/src/metal_renderer.rs) | Add `shared_metal_context()` and an RGBA surface pipeline. Validate device identity and ShaderRead usage; bind texture/opacity with existing vertices, clipping, instance indices and batching. Use premultiplied blend factors only for the new pipeline. Preserve CoreVideo buffer extraction and optionally trace drawable acquisition, composition submission, GPU completion and display timing. | Composite Zipzi textures without readback, re-upload or double alpha multiplication; retain YUV behavior. |
| [gpui-fast-apple/src/shaders.metal](gpui-fast-apple/src/shaders.metal) | Add `rgba_surface_fragment`, sampling the texture and multiplying by element opacity. | Preserve Zipzi's premultiplied output without multiplying texture alpha again. |

## Interface contracts and compatibility

- Input textures use premultiplied RGBA/BGRA, share the window's
  `SharedMetalContext.device`, and have `ShaderRead` usage.
- Submit producer rendering on `SharedMetalContext.queue` before handing the
  texture to GPUI. A shared device alone does not synchronize separate queues.
- The application publishes immutable frames. GPUI scenes retain texture handles
  so background work cannot overwrite a frame still on display. This policy
  belongs to the Editor; it adds no GPUI frame scheduler. Optional benchmark
  hooks observe boundaries without changing scheduling or waiting for the GPU.
- Downstream code constructing `PaintSurface` directly must adapt to its changed
  fields. Exhaustive matches on `SurfaceSource` must handle the new variant.
- The Fast patches retain macOS renderer selection, window lifecycle, IME,
  fonts and Kit components. Only the canvas suppresses repeated Space KeyDown
  events while it explicitly owns Space; initial presses and releases still
  route normally. Kit menu changes are separate patches described above.

## Why three Fast packages remain

The core package defines rendering and platform interfaces, the macOS package
forwards window access to the renderer, and the Apple package implements native
Metal composition. All three need changes. Apple's `src/fast/release_gpui`
directory is shader-binding generation input included in the published package
and read by `build.rs`; it is not a fourth patch package to remove.

## Validation and maintenance

[Native composition tests](../crates/zipzi-editor/tests/native_render.rs) cover
RGBA/BGRA, transparent backgrounds, combined document/element opacity, clipping,
UI occlusion, hiding, old-frame retention, resizing and scaling. Only the final
result is read back for assertions. Real-window interaction is validated by the
user.

Run commands from the repository root. The Editor gate below is the complete,
expensive suite; use the [focused test modes](../tools/README.md#test-coverage)
while iterating.

```sh
./tools/test-editor.sh full
# Check reverse applicability without modifying files:
git apply --reverse --check vendor/drag-ownership.patch
git apply --reverse --check vendor/native-metal.patch
git apply --reverse --check vendor/cache-boundary.patch
git apply --reverse --check vendor/list-height-estimates.patch
git apply --reverse --check vendor/kit-workspace.patch
git apply --reverse --check vendor/compact-menu.patch
git apply --reverse --check vendor/compact-tooltip.patch
git apply --reverse --check vendor/dock-handle-line.patch
git apply --reverse --check vendor/select-invalidation.patch
```

Reverse applicability alone does not prove that the patches capture every local
change. To check completeness, prepare the original published packages and
retained Kit files in a temporary directory, apply the listed patches using the
same `vendor/` layout, then compare the reconstructed files and file set with the
current dependency directories.

Patch paths are relative to the repository root and apply to upstream 0.1.7
packages arranged in that layout. When upgrading GPUI, recheck the 14 files
covered by the native Metal, cache-boundary and list-height patches. Check
whether upstream has fixed the cache and height issues, regenerate any remaining
patches, update the statistics, and run composition tests, full UI screenshots
and both app build profiles. See the
[native Metal integration record](../docs/archive/native-metal.md) for details.

The [window benchmark record](../docs/archive/window-benchmark.md) documents
commands, timing conventions and limitations. Trace-only probes expose drawable
counts, display synchronization and offscreen stages; diagnostic settings are
read only when the application installs a timing sink. Normal presentation
keeps triple buffering, display synchronization and direct drawing. See the
[frame-latency investigation](../docs/archive/frame-latency-diagnosis.md) for
comparisons.

Large-mesh overlay benchmarks use `submit_scene_for_benchmark()`, available only
in test/bench-support/test-support builds. It returns a submitted command buffer
for GPU timing and reuses size-matched intermediate textures on the headless
path. Existing `render_scene()` forwards to it; production window drawing keeps
its existing behavior.

## Windows shared GPU surfaces

`gpui-fast-windows` is vendored from the crates.io 0.1.7 release, retaining its
normalized/original manifests, source, lockfile and Apache-2.0 license. The root
workspace patch selects it. [windows-shared-texture.patch](windows-shared-texture.patch) records deviations
from that release and the accompanying core GPUI changes relative to the
pre-bridge checkpoint `7e42c511`.

The patch adds a D3D11 device accessor, `SurfaceSource::DirectX` with ready/consumed
fences, and Windows `paint_surface`/`surface` support. Both debugger and Fast draw
paths sample premultiplied textures with content-mask clipping and opacity; Fast
state tracking is invalidated after the external draw. Surface shaders compile
at runtime in debug and through the existing FXC build step in release. Shared
views are created once per pool slot by the producer, not opened on every draw.
The `test-support` offscreen harness exercises production pipelines with final
pixel readback; it is not part of normal presentation.

The patch covers 12 files, with 337 additions and 10 deletions. Apply it after
the existing core patches (including `gpui-diagnostics.patch`); the Windows
package baseline is the unmodified published `gpui-fast-windows` 0.1.7 package.
Paths are relative to the repository root. Root Cargo overrides and Editor
producer/pool integration are outside this vendor patch.

```sh
git apply --reverse --check --ignore-space-change vendor/windows-shared-texture.patch
```

On 2026-10-28, validation reversed the patch in a temporary directory, compared
the restored files with the published Windows package and core checkpoint
`7e42c511`, and reapplied it to reproduce all 12 current files. A complete
Windows package comparison also confirmed there are no unrecorded source
changes. Comparisons normalize CRLF/LF; Cargo's local `.cargo-ok` and
`.cargo_vcs_info.json` metadata are excluded.
