//! Ordinary CPU suites share one binary; alpha mesh keeps its separate geometry target.
//! Register new CPU suites here instead of adding another Cargo test target.

#[path = "authoring_acceptance.rs"]
mod authoring_acceptance;

#[path = "binding_authoring.rs"]
mod binding_authoring;
#[path = "cdi_authoring.rs"]
mod cdi_authoring;
#[path = "cmo3_export.rs"]
mod cmo3_export;
#[path = "document_operations.rs"]
mod document_operations;
#[path = "editor_state.rs"]
mod editor_state;
#[path = "effects_authoring.rs"]
mod effects_authoring;
#[path = "expression_authoring.rs"]
mod expression_authoring;
#[path = "expression_entries.rs"]
mod expression_entries;
#[path = "handles.rs"]
mod handles;
#[path = "hierarchy_order.rs"]
mod hierarchy_order;
#[path = "history_budget.rs"]
mod history_budget;
#[path = "mesh_editing.rs"]
mod mesh_editing;
#[path = "motion_editing.rs"]
mod motion_editing;
#[path = "motion_library.rs"]
mod motion_library;
#[path = "motion_properties.rs"]
mod motion_properties;
#[path = "organization_authoring.rs"]
mod organization_authoring;
#[path = "physics_authoring.rs"]
mod physics_authoring;
#[path = "physics_editing.rs"]
mod physics_editing;
#[path = "pose_authoring.rs"]
mod pose_authoring;
#[path = "project_io.rs"]
mod project_io;
#[path = "scene_authoring.rs"]
mod scene_authoring;
#[path = "scene_creation_placement.rs"]
mod scene_creation_placement;
#[path = "scene_lifecycle.rs"]
mod scene_lifecycle;
#[path = "vertical_slice.rs"]
mod vertical_slice;

#[path = "scene_content.rs"]
mod scene_content;

#[path = "reference_authoring.rs"]
mod reference_authoring;

#[path = "resource_authoring.rs"]
mod resource_authoring;

#[path = "authoring_collections.rs"]
mod authoring_collections;

#[path = "candidate_snapshot.rs"]
mod candidate_snapshot;

#[path = "image_memory.rs"]
mod image_memory;
