#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape)]
pub enum ObjectKind {
    Asset,
    Mesh,
    Parameter,
    MeshBinding,
    BindingAxis,
    BindingSample,
    Part,
    Transform,
    SceneBinding,
    BlendKeyTable,
    BlendConstraint,
    BlendBinding,
    Glue,
    GlueVertexPair,
    Offscreen,
    DrawOrderGroup,
    CdiParameterGroup,
    CdiCombinedSet,
    CdiCombinedMember,
    ModelParameterGroup,
    ModelParameterMember,
    ModelHitArea,
    Expression,
    ExpressionEntry,
    Motion,
    MotionGroup,
    MotionRegistration,
    MotionTrack,
    MotionKey,
    MotionEvent,
    Attachment,
    Pose,
    PoseGroup,
    PoseEntry,
    PoseLink,
    Physics,
    PhysicsGroup,
    PhysicsInput,
    PhysicsOutput,
    PhysicsNode,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, zipzi_api_derive::ApiShape)]
pub enum CapabilityState {
    Available,
    Unsupported {
        /// Stable reason code explaining why this operation is unsupported for this object/domain.
        code: &'static str,
    },
    Unavailable {
        /// Stable reason code explaining why this operation is currently unavailable.
        code: &'static str,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct ExpressionCapabilities {
    /// Read this object while its identity/context remains valid.
    pub inspect: CapabilityState,
    /// Create an owned expression entry.
    pub create_entry: CapabilityState,
    /// Modify typed fields through a validated native intent/address.
    pub patch: CapabilityState,
    /// Transfer expression entries between compatible owners.
    pub move_entry: CapabilityState,
    /// Duplicate the selected object while allocating fresh owned identities.
    pub duplicate: CapabilityState,
    /// Delete the selected object subject to native reference and lock rules.
    pub delete: CapabilityState,
    /// Repair an explicitly unresolved native reference.
    pub repair: CapabilityState,
    /// Native preview eligibility; host preview API availability is advertised separately.
    pub preview: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct PoseCapabilities {
    /// Read this object while its identity/context remains valid.
    pub inspect: CapabilityState,
    /// Modify typed fields through a validated native intent/address.
    pub patch: CapabilityState,
    /// Create owned child rows in this object.
    pub create_child: CapabilityState,
    /// Duplicate the selected object while allocating fresh owned identities.
    pub duplicate: CapabilityState,
    /// Delete the selected object subject to native reference and lock rules.
    pub delete: CapabilityState,
    /// Move the selected object between compatible native owners.
    pub move_object: CapabilityState,
    /// Change ordered sibling membership without changing identity.
    pub reorder: CapabilityState,
    /// Repair an explicitly unresolved native reference.
    pub repair: CapabilityState,
    /// Remove this container while moving owned content to an explicit destination.
    pub dissolve: CapabilityState,
    /// Native preview eligibility; host preview API availability is advertised separately.
    pub preview: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct ResourceCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to load the referenced resource contents; inspect its state/reason
    /// before invoking the operation.
    pub load_content: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct SceneContentCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to edit geometry/content through this address; inspect its
    /// state/reason before invoking the operation.
    pub edit: CapabilityState,
    /// Current capability to change topology while preserving or explicitly resolving
    /// dependencies; inspect its state/reason before invoking the operation.
    pub topology: CapabilityState,
}

#[derive(Debug, Clone, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct OrganizationCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to create children in this collection; inspect its state/reason
    /// before invoking the operation.
    pub create_child: CapabilityState,
    /// Current capability to move this object between collections; inspect its state/reason
    /// before invoking the operation.
    pub move_object: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to remove its container while preserving contents; inspect its
    /// state/reason before invoking the operation.
    pub dissolve: CapabilityState,
}

#[derive(Debug, Clone, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct AttachmentCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to load the referenced resource contents; inspect its state/reason
    /// before invoking the operation.
    pub load_content: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct MotionCapabilities {
    /// Read this object while its identity/context remains valid.
    pub inspect: CapabilityState,
    /// Modify typed fields through a validated native intent/address.
    pub patch: CapabilityState,
    /// Duplicate the selected object while allocating fresh owned identities.
    pub duplicate: CapabilityState,
    /// Delete the selected object subject to native reference and lock rules.
    pub delete: CapabilityState,
    /// Move the selected object between compatible native owners.
    pub move_object: CapabilityState,
    /// Native preview eligibility; host preview API availability is advertised separately.
    pub preview: CapabilityState,
    /// Create owned child rows in this object.
    pub create_child: CapabilityState,
    /// Change ordered sibling membership without changing identity.
    pub reorder: CapabilityState,
    /// Repair an explicitly unresolved native reference.
    pub repair: CapabilityState,
    /// Edit the owning clip content, preserving stable track/key/event identities.
    pub edit_content: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct PhysicsCapabilities {
    /// Read this object while its identity/context remains valid.
    pub inspect: CapabilityState,
    /// Modify typed fields through a validated native intent/address.
    pub patch: CapabilityState,
    /// Create owned child rows in this object.
    pub create_child: CapabilityState,
    /// Duplicate the selected object while allocating fresh owned identities.
    pub duplicate: CapabilityState,
    /// Delete the selected object subject to native reference and lock rules.
    pub delete: CapabilityState,
    /// Move the selected object between compatible native owners.
    pub move_object: CapabilityState,
    /// Change ordered sibling membership without changing identity.
    pub reorder: CapabilityState,
    /// Repair an explicitly unresolved native reference.
    pub repair: CapabilityState,
    /// Native preview eligibility; host preview API availability is advertised separately.
    pub preview: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct SceneCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to move this object between collections; inspect its state/reason
    /// before invoking the operation.
    pub move_object: CapabilityState,
    /// Current capability to change its deformation parent; inspect its state/reason before
    /// invoking the operation.
    pub reparent: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to remove its container while preserving contents; inspect its
    /// state/reason before invoking the operation.
    pub dissolve: CapabilityState,
}

#[derive(Clone, Debug, PartialEq, Eq, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct CollectionCapabilities {
    /// Current capability to create objects in this collection; inspect its state/reason before
    /// invoking the operation.
    pub create: CapabilityState,
    /// Current capability to move existing objects into this collection; inspect its
    /// state/reason before invoking the operation.
    pub move_into: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
}

#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct ReferenceCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to create children in this collection; inspect its state/reason
    /// before invoking the operation.
    pub create_child: CapabilityState,
    /// Current capability to move this object between collections; inspect its state/reason
    /// before invoking the operation.
    pub move_object: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to remove its container while preserving contents; inspect its
    /// state/reason before invoking the operation.
    pub dissolve: CapabilityState,
}

#[derive(Debug, Clone, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct BindingCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to create children in this collection; inspect its state/reason
    /// before invoking the operation.
    pub create_child: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to repair an unresolved or unavailable binding; inspect its
    /// state/reason before invoking the operation.
    pub repair: CapabilityState,
    /// Current capability to evaluate a conversion preview; inspect its state/reason before
    /// invoking the operation.
    pub preview: CapabilityState,
}
