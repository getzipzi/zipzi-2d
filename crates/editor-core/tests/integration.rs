//! Core behavior suites share one set of SDK fixtures.
mod support;

#[path = "cases/animation/mod.rs"]
mod animation;
#[path = "cases/canvas.rs"]
mod canvas;
#[path = "cases/canvas_objects/mod.rs"]
mod canvas_objects;
#[path = "cases/commands.rs"]
mod commands;
#[path = "cases/document.rs"]
mod document;
#[path = "cases/editor.rs"]
mod editor;
#[path = "cases/keyboard.rs"]
mod keyboard;
#[path = "cases/locks.rs"]
mod locks;
#[path = "cases/mesh_generation.rs"]
mod mesh_generation;
#[path = "cases/mesh_thumbnails.rs"]
mod mesh_thumbnails;
#[path = "cases/panels.rs"]
mod panels;
#[path = "cases/parameter_creation.rs"]
mod parameter_creation;
#[path = "cases/parameter_management.rs"]
mod parameter_management;
#[path = "cases/properties.rs"]
mod properties;
#[path = "cases/psd_import.rs"]
mod psd_import;
#[path = "cases/sorting.rs"]
mod sorting;
#[path = "cases/timeline/mod.rs"]
mod timeline;
#[path = "cases/viewport_rendering.rs"]
mod viewport_rendering;
#[path = "cases/viewport_states.rs"]
mod viewport_states;
#[path = "cases/viewport_targets.rs"]
mod viewport_targets;
#[path = "cases/visibility.rs"]
mod visibility;
#[path = "cases/workspaces.rs"]
mod workspaces;
