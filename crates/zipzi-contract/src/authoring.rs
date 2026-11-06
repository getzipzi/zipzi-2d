//! Shared authoring request and result values.

use crate::document::{
    BindingGridEdit, BlendAuthoringEdit, DeletionDecision, DeletionEffect, DeletionLock,
    DeletionObject, DeletionRelation, DeletionRemoval, DeletionStructuralBlocker,
    ParameterRangeEdit, ReparentEdit,
};
use crate::model::{MeshMeshing, ParameterKind, Vec2, VertexId};
use serde::{Deserialize, Serialize};

// Include the domain files in this module to retain type paths and wire signatures.
include!("authoring/meshes.rs");
include!("authoring/parameter_binding.rs");
include!("authoring/deletion_review.rs");
include!("authoring/sampling_quality.rs");
include!("authoring/modeling_query.rs");
include!("authoring/capabilities.rs");
include!("authoring/binding_deletion.rs");
include!("authoring/scene_specs.rs");
include!("authoring/scene_content_specs.rs");
include!("authoring/modeling_records.rs");
include!("authoring/manual_mesh.rs");
include!("authoring/receipts.rs");
