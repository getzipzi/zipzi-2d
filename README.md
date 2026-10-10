# Kasane Editor

A native macOS editor for Kasane 2D models, built with Rust, GPUI Kit, the Kasane SDK, and Metal.

## Setup

- macOS with a Metal-capable GPU.
- Rust stable and Xcode Command Line Tools (`xcode-select --install`).
- A compatible [kasane-2d](https://github.com/pakholeung37/kasane-2d) checkout at `../kasane-2d`, including `modules/kasane-render-metal` and the required SDK/render changes. See [dependency details](DEPENDENCIES.md).

## Run

From the repository root:

```sh
cargo run --locked -p kasane-editor
```

To explore the UI components:

```sh
cargo run --locked -p component-playground
```

## Generate meshes

Select a mesh and open **Generate mesh** from the toolbar. **Alpha contour**
is selected by default, generating from the original texture's alpha with Light,
Standard and Heavy presets, editable pixel spacing/margins, an alpha threshold, and an
option to preserve transparent holes. The preview runs in the background;
**Apply mesh** creates one undoable edit. Generated meshes can be regenerated,
including after saving and reopening the project.

Outside margins can extend beyond the original layer rectangle. Applying a
mesh adds only the needed transparent texture border and remaps UVs in the same
undoable edit. Regeneration reuses that border without growing it repeatedly.
**Ignore tiny faint islands** is enabled by default; turn it off to retain every
pixel above the alpha threshold. It excludes only isolated components with peak
alpha <= 64 and total alpha <= four opaque pixels, when stronger artwork exists.
Opaque dots, connected faint edges and fully translucent layers are retained.
Distances use original texture pixels. Light, Standard and Heavy now use
outside/inside spacing of 160/160, 85/85 and 40/40 pixels respectively, targeting
roughly half the former vertex counts on typical artwork. Corners, thin features
and minimum boundary counts may limit the reduction on small or detailed layers.
These are modeling presets, not equivalent Cubism settings; custom pixel values
retain their literal meaning.
Smooth convex outlines use more even boundary spacing while preserving the
required padding.

Alpha generation currently requires a dedicated source texture and a consistent
affine mapping between authored positions and UVs (such as imported PSD layers).
Meshes with keyforms, BlendShapes, Glue, shared atlas textures, or non-affine
vertex edits are excluded. Rectangle grid remains available as a secondary
option and is selected automatically when alpha generation is unavailable. It
retains its existing dependency migration behavior.

## Develop

Install `watchexec`, then rebuild and restart automatically when source files change:

```sh
brew install watchexec
./scripts/dev.sh
./scripts/dev.sh playground  # Watch the component playground
```

## Tests

Run all tests and check formatting (the full suite requires macOS and Metal):

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
```

Test locations:

- `crates/editor-core/tests/`: document, command, and editor logic tests.
- `crates/editor/headless_tests/`: UI interaction tests using GPUI's headless window.
- `crates/editor/tests/`: native Metal, mesh overlay, and screenshot tests.

Run a specific test module or target:

```sh
cargo test --locked -p editor-core --test commands
cargo test --locked -p kasane-editor --bin kasane-editor headless_tests::canvas
cargo test --locked -p kasane-editor --test native_metal
```

Add UI tests with `#[gpui_kit::test]` in `crates/editor/headless_tests/`; register new modules in its `mod.rs` and reuse the shared `setup` harness.

## Headless screenshots

Capture the real editor UI without opening a native window. Requires macOS and a Metal-capable GPU.

```sh
unset KASANE_DEV_SESSION_DIR
cargo test --locked -p kasane-editor --test headless_screenshot
```

## Build apps

```sh
./scripts/build-app.sh
./scripts/build-app.sh playground
```

Release bundles are saved to `target/Kasane Editor.app` and `target/Kasane Component Playground.app`.
