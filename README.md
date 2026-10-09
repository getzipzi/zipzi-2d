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

## Canvas navigation

- Trackpad: two-finger scrolling pans in both directions, including momentum; pinch zooms around the pointer.
- Mouse: the wheel zooms proportionally around the pointer, with a short smooth transition; middle-button dragging pans. Reduced Motion disables the transition.
- Ctrl/Cmd + trackpad scrolling also zooms proportionally, with immediate response like pinch. H or held Space enables left-button panning.

## Mesh generation

Select one unlocked, standard four-corner rectangle mesh (such as an imported PSD layer), then click the grid icon in the canvas toolbar or press Shift+M. Set the column and row subdivision counts and choose Generate. The dialog shows the resulting vertex and triangle counts; grids are limited to 65,536 vertices.

This tool uses Kasane SDK's rectangle grid generator. It preserves mesh keyforms, BlendShapes, and corner Glue references in one undoable edit. Meshes with keyforms or BlendShapes require equal row and column counts. Existing non-rectangular or subdivided meshes are unsupported; undo generation before choosing different counts. Alpha-contour mesh generation is not yet implemented in the SDK.

## Parameter management

Use the Parameters toolbar to create a parameter or group. Right-click a parameter or group to edit or delete it; group menus also create parameters in place. The Properties panel provides edit and delete actions for the inspected parameter or group.

The UI currently creates only top-level groups and does not expose a parent-group selector. Existing nested groups can be dragged before or after a top-level group, or onto the Ungrouped header to move them to the end of the top level. The core model still supports nested groups.

Parameters with references cannot be deleted until those references are removed. Deleting a group keeps its contents and moves its direct parameters and child groups to the parent. Each operation supports undo and redo. See [parameter management](docs/parameter-management-design.md) for editing and persistence rules.

## Tree visibility

Use the eye beside a Mesh, Part, Warp Deformer, Rotation Deformer, or Offscreen, including while locked. Eyes affect editor display only; runtime `enabled`, deformation, animation, masks, and model export retain their authored behavior. A Deformer's eye hides its own controls. A Part hides objects in its organization subtree, without following deformation links into other Parts. An Offscreen hides its composited group output. A hidden Mesh still contributes to masks.

Part locks inherit through the organization hierarchy. Deformer locks protect only direct edits to that Deformer; they do not freeze the pose or lock deformation children. Both trees use the same object state. Gray icons indicate inherited restrictions and show their sources; clicking opens options to locate or change a source without silently changing selection or child flags.

Explicit eye and lock flags share a sparse `document.editor_state.objects` map keyed by object ID. These edits support undo, redo, and saving in project format v10. The former `object_locks` and `deformer_display` schemas and APIs are removed; projects containing those fields are rejected without migration. Runtime `enabled` remains separate authoring data. Use **Model enabled** in Properties to edit that authored state separately. Tree collapse, filtering and global overlay preferences remain separate view state.

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
