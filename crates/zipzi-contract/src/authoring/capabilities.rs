#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape)]
/// Native identity domain used when capturing a source-bound SDK object reference.
pub enum ObjectKind {
    /// Identifies a document image resource.
    Asset,
    /// Identifies a drawable mesh.
    Mesh,
    /// Identifies an authored parameter.
    Parameter,
    /// Identifies an ordinary mesh binding.
    MeshBinding,
    /// Identifies a binding axis owned by a grid.
    BindingAxis,
    /// Identifies a sample owned by a binding axis, BlendShape table or constraint.
    BindingSample,
    /// Identifies an organizational Part.
    Part,
    /// Identifies a deformation transform.
    Transform,
    /// Identifies an ordinary scene binding.
    SceneBinding,
    /// Identifies a BlendShape key table.
    BlendKeyTable,
    /// Identifies a BlendShape constraint.
    BlendConstraint,
    /// Identifies a BlendShape target binding.
    BlendBinding,
    /// Identifies a connection between mesh endpoints.
    Glue,
    /// Identifies a vertex pair owned by a Glue.
    GlueVertexPair,
    /// Identifies a Part-owned Offscreen composite.
    Offscreen,
    /// Identifies a drawing-group role of a Part or root.
    DrawOrderGroup,
    /// Identifies a CDI parameter organization group.
    CdiParameterGroup,
    /// Identifies a CDI combined-parameter set.
    CdiCombinedSet,
    /// Identifies a member of a CDI combined-parameter set.
    CdiCombinedMember,
    /// Identifies a model3 parameter metadata group.
    ModelParameterGroup,
    /// Identifies a member of a model3 parameter group.
    ModelParameterMember,
    /// Identifies a model3 hit-area registration.
    ModelHitArea,
    /// Identifies an expression asset.
    Expression,
    /// Identifies an entry owned by an expression.
    ExpressionEntry,
    /// Identifies a motion clip asset.
    Motion,
    /// Identifies a motion registration group.
    MotionGroup,
    /// Identifies a motion registration referencing a clip.
    MotionRegistration,
    /// Identifies a track owned by a motion clip.
    MotionTrack,
    /// Identifies a key owned by a motion track.
    MotionKey,
    /// Identifies an event owned by a motion clip.
    MotionEvent,
    /// Identifies a package attachment.
    Attachment,
    /// Identifies a Pose singleton.
    Pose,
    /// Identifies a group owned by Pose.
    PoseGroup,
    /// Identifies an entry owned by a Pose group.
    PoseEntry,
    /// Identifies a link owned by a Pose entry.
    PoseLink,
    /// Identifies a Physics singleton.
    Physics,
    /// Identifies a Physics rig group.
    PhysicsGroup,
    /// Identifies an input owned by a Physics rig.
    PhysicsInput,
    /// Identifies an output owned by a Physics rig.
    PhysicsOutput,
    /// Identifies a node owned by a Physics rig.
    PhysicsNode,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, zipzi_api_derive::ApiShape)]
/// Current capability verdict with a reason when unavailable or unsupported; it does not reserve execution.
pub enum CapabilityState {
    /// The operation is currently available subject to later validation and admission.
    Available,
    /// The operation is not supported for this object or domain.
    Unsupported {
        /// Stable reason code explaining why this operation is unsupported for this object/domain.
        code: &'static str,
    },
    /// The operation is currently unavailable for the captured state.
    Unavailable {
        /// Stable reason code explaining why this operation is currently unavailable.
        code: &'static str,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, zipzi_api_derive::ApiShape)]
/// Current expression capability states; they do not reserve later execution.
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
/// Current Pose capability states; they do not reserve later execution.
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
/// Current image-resource capability states; they do not reserve later execution.
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
/// Current scene-content capability states; they do not reserve later execution.
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
/// Current organization capability states; they do not reserve later execution.
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
/// Current attachment capability states; they do not reserve later execution.
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
/// Current motion capability states; they do not reserve later execution.
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
/// Current Physics capability states; they do not reserve later execution.
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
/// Current scene capability states; they do not reserve later execution.
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
/// Current collection capability states; they do not reserve later execution.
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
/// Current reference capability states; they do not reserve later execution.
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
/// Current binding capability states; they do not reserve later execution.
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
