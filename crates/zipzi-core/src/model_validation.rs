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
mod view;
pub(crate) use view::*;
mod queries;
pub(crate) struct Model<'a> {
    pub content: ModelContent<'a>,
    binding_ids: OnceLock<Vec<BindingIdentity>>,
    reference_ids: OnceLock<Vec<ReferenceIdentity>>,
}
impl<'a> Model<'a> {
    pub fn new(content: ModelContent<'a>) -> Self {
        Self {
            content,
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
