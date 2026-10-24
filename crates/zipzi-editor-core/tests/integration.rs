//! Core behavior suites share one set of SDK fixtures.
mod support;

#[path = "cases/boundless_canvas.rs"]
mod boundless_canvas;

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
#[path = "cases/mesh_properties.rs"]
mod mesh_properties;
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
#[path = "cases/sorting.rs"]
mod sorting;
#[path = "cases/timeline/mod.rs"]
mod timeline;
#[path = "cases/viewport_isolation.rs"]
mod viewport_isolation;
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

#[path = "cases/expression_authoring.rs"]
mod expression_authoring;

#[path = "cases/binding_authoring.rs"]
mod binding_authoring;
#[path = "cases/organization_authoring.rs"]
mod organization_authoring;
#[path = "cases/pose_authoring.rs"]
mod pose_authoring;
#[path = "cases/scene_lifecycle.rs"]
mod scene_lifecycle;

#[path = "cases/scene_content.rs"]
mod scene_content;

#[path = "cases/reference_authoring.rs"]
mod reference_authoring;

#[path = "cases/resource_authoring.rs"]
mod resource_authoring;

#[path = "cases/palette_moves.rs"]
mod palette_moves;

#[path = "cases/deformation_creation.rs"]
mod deformation_creation;
#[path = "cases/scene_creation.rs"]
mod scene_creation;

#[path = "cases/palette_steps.rs"]
mod palette_steps;

#[path = "cases/palette_reparent.rs"]
mod palette_reparent;

#[path = "cases/authoring_coordination.rs"]
mod authoring_coordination;

#[path = "cases/animation_library.rs"]
mod animation_library;

#[path = "cases/animation_management.rs"]
mod animation_management;
#[path = "cases/business_authoring.rs"]
mod business_authoring;

#[path = "cases/ordinary_binding.rs"]
mod ordinary_binding;

#[path = "cases/runtime.rs"]
mod runtime;
