//! Shared Runtime requests, responses, handles and events.

use crate::authoring::{
    AttachmentCapabilities, BindingCapabilities, BindingFieldPatch, CapabilityState,
    CollectionCapabilities, EditReceipt, ExpressionCapabilities, MotionCapabilities,
    OrganizationCapabilities, PhysicsCapabilities, PhysicsCollection, PoseCapabilities,
    ReferenceCapabilities, ResourceCapabilities, SaveReceipt, SceneCapabilities,
    SceneContentCapabilities, Version,
};
use crate::document::{
    DocumentField, DocumentValue, OrganizationCollection, OrganizationEdit, ReferenceCollection,
    ReferenceFields, ReferenceSelectionField, SceneCollection, SceneContentTarget,
    SceneContentValue, SceneField, SceneFieldValue,
};
use crate::evaluation::PreviewValues;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

// Include the domain files in this module to retain type paths and wire signatures.
include!("runtime/limits_context.rs");
include!("runtime/observation_textures.rs");
include!("runtime/identity.rs");
include!("runtime/snapshots.rs");
include!("runtime/sampling_preview.rs");
include!("runtime/capabilities.rs");
include!("runtime/addressing.rs");
include!("runtime/modeling_commands.rs");
include!("runtime/edit_resource_operations.rs");
include!("runtime/sessions_events.rs");
