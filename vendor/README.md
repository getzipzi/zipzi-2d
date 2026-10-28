# Vendored GPUI dependencies

This directory contains the GPUI sources and local patches used by the Editor.
Keep upstream license files, package metadata and patch provenance when updating
these dependencies.

## Dependencies

| Dependency | Source and local use |
| --- | --- |
| GPUI Kit 0.7.3 | [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit), pinned to `507e82b8115ac73af92b7d3856501f9d7b73b2c9`. `gpui-component` is patched locally; base, assets and macros use the same Git revision. |
| GPUI Fast 0.1.7 | The crates.io `gpui-fast`, `gpui-fast-macos` and `gpui-fast-apple` packages are patched locally. |
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
