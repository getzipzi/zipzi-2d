//! Shared, read-only model validation. This view borrows persistent records;
//! it owns no Document, session, transaction, save marker or publication state.
use crate::document::*;
use crate::geometry::{validate_positions, validate_render_mesh};
use crate::types::*;
use crate::SceneTrackEvaluation;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, OnceLock},
};
pub(crate) mod bone_display;
pub(crate) mod conversion;
mod export_projection;
mod motion_projection;
pub(crate) mod organization;
pub(crate) mod original_authoring;
pub(crate) mod physics_projection;
pub(crate) mod pose_projection;
pub(crate) mod reference_operations;
pub(crate) mod scene;
pub(crate) mod scene_content;
mod view;
pub(crate) use view::*;
mod attachments;
pub(crate) mod binding_coordinates;
mod expression_authoring;
pub(crate) mod expression_projection;
mod locks;
pub(crate) mod motion_authoring;
pub(crate) mod physics_authoring;
pub(crate) mod physics_patch;
pub(crate) mod pose_authoring;
mod queries;
mod reference_projection;
mod references;
pub(crate) struct Model<'a> {
    pub content: ModelContent<'a>,
    pub(crate) deletion_index: OnceLock<Arc<crate::state::deletion::index::DeletionIndex>>,
    organization_index: OnceLock<Arc<organization::OrganizationIndex>>,
    motion_lookup: OnceLock<Arc<motion_authoring::MotionLookup>>,
    prepared: OnceLock<Result<crate::evaluation::PreparedEvaluation, Status>>,
    projection: OnceLock<Arc<SceneProjection>>,
    binding_ids: OnceLock<Vec<BindingIdentity>>,
    reference_ids: OnceLock<Vec<ReferenceIdentity>>,
}
impl<'a> Model<'a> {
    pub fn new(content: ModelContent<'a>) -> Self {
        Self {
            content,
            deletion_index: OnceLock::new(),
            organization_index: OnceLock::new(),
            motion_lookup: OnceLock::new(),
            prepared: OnceLock::new(),
            projection: OnceLock::new(),
            binding_ids: OnceLock::new(),
            reference_ids: OnceLock::new(),
        }
    }
}
pub(crate) mod binding_authoring_identity;
pub(crate) mod bindings;
pub(crate) mod blendshapes;
pub(crate) mod deformer_original;
pub(crate) mod display_info;
pub(crate) mod editor_state;
pub(crate) mod expressions;
pub(crate) mod glue;
pub(crate) mod hierarchy_order;
pub(crate) mod meshes;
pub(crate) mod model3;
pub(crate) mod motions;
pub(crate) mod offscreen;
pub(crate) mod parameters;
pub(crate) mod parts;
pub(crate) mod physics;
pub(crate) mod physics_model;
pub(crate) mod pose;
pub(crate) mod reference_authoring;
pub(crate) mod structure;
pub(crate) mod transforms;
