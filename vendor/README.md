# Vendored GPUI dependencies

This directory contains the GPUI sources and local patches used by the Editor.
Keep upstream license files, package metadata and patch provenance when updating
these dependencies.

## Dependencies

| Dependency | Source and local use |
| --- | --- |
| GPUI Kit 0.7.3 | [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit), pinned to `507e82b8115ac73af92b7d3856501f9d7b73b2c9`. `gpui-component` is patched locally; base, assets and macros use the same Git revision. |
| GPUI Fast 0.1.7 | The crates.io `gpui-fast`, `gpui-fast-macos`, `gpui-fast-apple` and `gpui-fast-windows` packages are patched locally. |
| Metal 0.33 | crates.io dependency used by the macOS renderer. |
| Zipzi SDK | Workspace crates under [`crates/`](../crates), built from the same checkout. |

The root [Cargo.toml](../Cargo.toml) selects the local patches and enables GPUI
Kit's `gpui-fast` feature. The workspace shares [Cargo.lock](../Cargo.lock); use
`--locked` for builds and tests. Path dependencies are not pinned to a source
revision by the lockfile. No sibling GPUI checkout is required.

The Fast crates retain their published `Cargo.toml.orig` and Apache-2.0 licenses.
The vendored Kit component retains its upstream manifest and license. Its
reduced workspace manifest provides inherited dependencies and lints; the
upstream README and files needed by relative test paths are also retained.

## Local patches

| Patch | Purpose |
| --- | --- |
| [gpui-diagnostics.patch](gpui-diagnostics.patch) | Adds opt-in cross-platform CPU frame timing. |
| [path-buffer-reuse.patch](path-buffer-reuse.patch) | Reuses path vertex buffers when scaling. |
| [windows-shared-texture.patch](windows-shared-texture.patch) | Adds Windows shared GPU surface composition. |
| [native-metal.patch](native-metal.patch) | Exposes the window's Metal device and queue, composites native textures, and adds optional frame tracing and headless support. |
| [drag-ownership.patch](drag-ownership.patch) | Adds drag identity and targeted cancellation so a stale token cannot stop a later drag. |
| [cache-boundary.patch](cache-boundary.patch) | Preserves explicit virtual-list cache boundaries across frame reuse and splicing. |
| [list-height-estimates.patch](list-height-estimates.patch) | Retains list height estimates across width changes until rows are remeasured. |
| [kit-workspace.patch](kit-workspace.patch) | Defines the reduced Kit workspace and pinned sibling dependencies. |
| [compact-menu.patch](compact-menu.patch) | Adds compact menu sizing and inherits the parent size in submenus. |
| [compact-tooltip.patch](compact-tooltip.patch) | Adjusts tooltip sizing, wrapping and per-window identity handling. |
| [dock-handle-line.patch](dock-handle-line.patch) | Allows persistent dock resize lines to be hidden without disabling resizing. |
| [select-invalidation.patch](select-invalidation.patch) | Avoids redundant searchable-list invalidation and exposes Select open state. |

The three Fast crates serve different roles: the core defines interfaces, the
macOS crate connects window access, and the Apple crate implements Metal
composition. `gpui-fast-apple/src/fast/release_gpui` is shader-binding input used
by `build.rs` and must be retained.

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

## Metal surface contract

Native surfaces use premultiplied RGBA/BGRA textures with `ShaderRead` usage on
the window's `SharedMetalContext.device`. Submit producer work to
`SharedMetalContext.queue` before handing the texture to GPUI; sharing a device
does not synchronize separate queues. The Editor publishes immutable frames,
and GPUI retains texture handles while they are displayed.

The patch adds `SurfaceSource::MetalTexture` and changes `PaintSurface` fields;
code constructing or matching these types must account for the new interface.

## Updating and testing

When upgrading, replace the Fast crates and retained Kit files with the new
upstream versions, review each patch against its new baseline, and regenerate
those still needed. Update the root dependency pins, the reduced Kit workspace
and lockfile. Preserve the upstream notices and validate the reconstructed file
set, not just patch applicability.

From the repository root, check the recorded patches without modifying files:

```sh
for patch in vendor/*.patch; do
  git apply --reverse --check "$patch" && printf "OK: %s\n" "$patch"
done
```

`native-metal.patch` does not pass the reverse check against the current sources;
review its differences before using it to reconstruct or upgrade the vendored
files. Reverse applicability alone does not prove that a patch covers every local change.
For a completeness check, reconstruct the vendored files from the original
packages and Kit commit in a temporary directory, apply the patches, and compare
the resulting files and file set with this directory.

Use focused Editor tests while iterating. Direct Editor Cargo tests must clear
`ZIPZI_DEV_SESSION_DIR`:

```sh
./tools/test-editor.sh ui headless_tests::compact_popups
./tools/test-editor.sh ui headless_tests::tooltips
./tools/test-editor.sh ui headless_tests::view_cache
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor \
  --features test-support --test native_render
```

Native rendering tests require macOS and Metal. See
[test coverage](../tools/README.md#test-coverage) for broader Editor gates,
screenshots and prerequisites, and [distribution checks](../tools/README.md#local-distribution-artifacts)
for app builds.

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
