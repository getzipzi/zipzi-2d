#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// A resolution offered for a deletion decision; selecting it requires a new review.
pub enum DeletionChoice {
    /// Attempt to preserve deformation at the frozen edit state.
    PreserveEditState,
    /// Keep local deformation data and accept possible visual change.
    KeepLocalData,
    /// Choose a retained Part destination.
    ChoosePart,
    /// Supply a mapping for a surviving Offscreen.
    SupplyOffscreenMapping,
    /// Choose a retained replacement texture.
    ChooseTexture,
    /// Choose a retained BlendShape base sample.
    ChooseBlendBase,
    /// Explicitly leave the texture reference unconfigured.
    ClearTexture,
    /// Choose a replacement Physics node.
    ChoosePhysicsNode,
    /// Expand the deletion to the indicated subtree.
    DeleteSubtree,
    /// Cancel this deletion request.
    Cancel,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Native condition requiring an explicit deletion decision.
pub enum DeletionDecisionReason {
    /// A requested conversion could not establish its preservation proof.
    ConversionUnproven {
        /// Native conversion diagnostic code; available resolutions come from typed choices.
        code: String,
    },
    /// A surviving Offscreen needs an available retained Part owner.
    OffscreenOwnerUnavailable,
    /// A surviving Offscreen needs an explicit destination mapping.
    OffscreenMappingRequired,
    /// Dissolving a Part would remove inherited state without a chosen resolution.
    PartStateUnpreserved,
    /// Retained texture users require an explicit replacement or clear choice.
    TextureReplacementRequired,
    /// A deleted BlendShape base requires a retained replacement sample.
    BlendBaseReplacementRequired,
    /// A removed Physics node requires a replacement for retained users.
    PhysicsNodeReplacementRequired,
    /// The requested result violates a native container invariant.
    InvalidContainer {
        /// Native container invariant requiring an explicit larger deletion or cancellation.
        code: String,
    },
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Affected object, unresolved reason and available resolutions in a frozen review.
pub struct DeletionDecision {
    /// Affected surviving object that needs an explicit resolution.
    pub object: DeletionObject,
    /// Typed unresolved native condition; it is not an instruction to expand deletion.
    pub reason: DeletionDecisionReason,
    /// Actually available alternatives; selecting one requires a new source-bound plan.
    pub choices: Vec<DeletionChoice>,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Typed frozen-source field or slot identifying a deletion relation.
pub enum DeletionField {
    /// Owned identity relationship.
    Owned,
    /// Mesh clipping-mask entry at a frozen slot.
    MeshMask {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    /// Mesh inverted-mask state.
    MeshInvertedMask,
    /// Offscreen mask entry at a frozen slot.
    OffscreenMask {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    /// Glue endpoint A mesh reference.
    GlueMeshA,
    /// Glue endpoint B mesh reference.
    GlueMeshB,
    /// Hit-area mesh reference.
    HitAreaMesh,
    /// Drawing-group member at a frozen slot.
    DrawingItem {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    /// Organization parent relation.
    OrganizationParent,
    /// Deformation parent relation.
    DeformationParent,
    /// Data produced by a conversion.
    ConvertedData,
    /// Pose Part reference.
    PosePart,
    /// Pose link Part reference.
    PoseLinkPart,
    /// Motion Part reference.
    MotionPart,
    /// Expression source Part reference.
    ExpressionSourcePart,
    /// CDI Part reference at a frozen slot.
    CdiPart {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    /// Relationship included by the requested scope.
    RequestedScope,
    /// Binding axis parameter reference.
    ParameterAxis,
    /// BlendShape key-table parameter reference.
    BlendTableParameter,
    /// BlendShape constraint parameter reference.
    BlendConstraintParameter,
    /// BlendShape binding key-table reference.
    BlendTable,
    /// BlendShape binding constraint at a frozen slot.
    BlendConstraint {
        /// Zero-based constraint address in the frozen BlendBinding.
        slot: usize,
    },
    /// Motion parameter reference.
    MotionParameter,
    /// Expression parameter target reference.
    ExpressionParameter,
    /// Expression source parameter reference.
    ExpressionSourceParameter,
    /// Physics input or output parameter reference.
    PhysicsParameter,
    /// Physics node reference.
    PhysicsNode,
    /// Image texture reference.
    Texture,
    /// Motion sound attachment reference.
    AttachmentSound,
    /// User-data attachment reference.
    AttachmentUserData,
    /// Motion registration clip reference.
    MotionClip,
    /// Motion registration container relation.
    MotionRegistrationContainer,
    /// Parameter organization folder relation.
    ParameterFolder,
    /// Model parameter metadata reference.
    ModelParameter,
    /// CDI parameter reference at a frozen slot.
    CdiParameter {
        /// Frozen zero-based CDI Parameters record address; imported unresolved records remain untouched.
        slot: usize,
    },
    /// CDI combined-parameter member reference.
    CdiCombinedParameter,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Frozen-source owner, typed field and target explaining a dependency or rewrite.
pub struct DeletionRelation {
    /// Native object or role containing this field in the frozen source.
    pub owner: DeletionObject,
    /// Typed field/slot address valid only in the frozen source.
    pub field: DeletionField,
    /// Native target identity explaining this ownership, dependency or rewrite.
    pub target: DeletionObject,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Why an identity is included in the deletion report.
pub enum DeletionReason {
    /// Explicitly requested root.
    Requested,
    /// Identity owned by a removed object.
    Owned,
    /// Record included because of a native dependency.
    Dependency,
    /// Descendant included by the requested deletion scope.
    Scope,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Identity proposed for removal and the reason it is included.
pub struct DeletionRemoval {
    /// Retired identity, including its native domain and embedded namespace.
    pub object: DeletionObject,
    /// Whether this is an explicit root, scoped descendant, owned identity or dependency record.
    pub reason: DeletionReason,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Direct or inherited lock blocking a removal or rewrite.
pub struct DeletionLock {
    /// Actual removed or rewritten object protected by the lock.
    pub object: DeletionObject,
    /// Persistent ID of the direct or inherited lock source to locate and unlock.
    pub source_id: String,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Native structural validation failure preventing an executable candidate.
pub struct DeletionStructuralBlocker {
    /// Native result identity/address associated with the structural failure.
    pub object_id: String,
    /// Native structure-validation code; the candidate cannot be installed.
    pub code: String,
    /// Display diagnostic describing the invalid result; never parsed to decide execution.
    pub reason: String,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// A non-position channel that may lie outside a deletion conversion proof.
pub enum DeletionChannel {
    /// Opacity or color channels.
    Appearance,
    /// Evaluation enabled state.
    Enabled,
    /// Glue seam postprocessing.
    GluePostprocessing,
    /// Drawing-order behavior.
    DrawingOrder,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Typed effect predicted by a deletion review.
pub enum DeletionEffect {
    /// A retained Clip registration moves to the ungrouped root.
    MotionRegistrationMoved {
        /// Preserved last registration of a retained Clip, moved to the ungrouped root.
        registration: DeletionObject,
        /// Removed source group; the registration is not deleted with its other entries.
        group: DeletionObject,
    },
    /// A retained native record loses selected samples, pairs or keys.
    ContentChanged(
        /// Retained owner whose selected stable content identities disappear.
        DeletionObject,
    ),
    /// A surviving grid loses a parameter dimension at a frozen slice.
    ParameterDimensionRemoved {
        /// Surviving ordinary binding or Glue grid reduced by the native grid converter.
        object: DeletionObject,
        /// Driving parameter whose coordinate is frozen for this removed axis.
        parameter: DeletionObject,
        /// Frozen document-default or explicitly selected finite slice coordinate.
        value: f32,
        /// Coordinate after native repeat normalization, key snapping and endpoint selection.
        effective_value: f32,
        /// Whether this fixed coordinate disables the ordinary target outside the native grid.
        disabled: bool,
    },
    /// A retained target loses an authored binding contribution.
    BindingContributionRemoved {
        /// Retained target whose authored contribution disappears.
        object: DeletionObject,
        /// Deleted ordinary or Blend binding that previously supplied this contribution.
        binding: DeletionObject,
    },
    /// A retained consumer redirects or clears a resource reference.
    ResourceReplaced {
        /// Retained consumer whose texture or optional attachment reference changes.
        object: DeletionObject,
        /// Deleted resource previously referenced by this consumer.
        source: DeletionObject,
        /// Retained replacement resource; None means an explicit clear/unconfigured result.
        replacement: Option<DeletionObject>,
    },
    /// A parameter animation record is removed.
    ParameterAnimationRemoved(
        /// Removed Motion/Expression parameter record; other tracks, events and resource registrations remain.
        DeletionObject,
    ),
    /// An input or output channel is removed from retained Physics.
    PhysicsChannelRemoved {
        /// Removed input or output channel, with a stable native identity.
        object: DeletionObject,
        /// Retained Physics setting group whose behavior changes.
        group: DeletionObject,
    },
    /// A retained consumer changes clipping masks.
    MaskChanged(
        /// Retained Mask consumer whose clipping effect changes.
        DeletionObject,
    ),
    /// A deleted Glue connection no longer contributes its seam.
    GlueRemoved(
        /// Deleted connection whose seam or postprocessing contribution disappears.
        DeletionObject,
    ),
    /// A model hit-area registration is removed.
    HitAreaRemoved(
        /// Deleted model3 Hit Area registration.
        DeletionObject,
    ),
    /// Offscreen or Part composition may change.
    CompositionChanged(
        /// Affected Offscreen or Part whose compositing can change.
        DeletionObject,
    ),
    /// A Part animation or Pose record is removed.
    PartAnimationRemoved(
        /// Removed Part animation/Pose record; its container remains.
        DeletionObject,
    ),
    /// A drawing group is flattened into a retained destination.
    DrawingGroupFlattened {
        /// Removed drawing role being flattened.
        group: DeletionObject,
        /// Nearest retained drawing role receiving expanded members.
        destination: DeletionObject,
        /// Original inclusive group order bounds.
        source_range: [i32; 2],
        /// Retained destination order bounds; clipping and relative order may differ.
        destination_range: [i32; 2],
        /// Whether parameter or Blend bindings drove the removed group order.
        dynamic_order: bool,
        /// Original composite whose group scope is lost or migrated.
        offscreen: Option<DeletionObject>,
    },
    /// A surviving object changes organizational owner.
    OrganizationChanged(
        /// Surviving member whose organization owner changes.
        DeletionObject,
    ),
    /// A surviving object has deformation that is not fully preserved.
    DeformationChanged(
        /// Surviving object whose deformation is not fully preserved.
        DeletionObject,
    ),
    /// A surviving object has channels outside the position guarantee.
    UnpreservedChannels {
        /// Surviving object affected by channels outside the position proof.
        object: DeletionObject,
        /// Explicit channels not covered by the converter position guarantee.
        channels: Vec<DeletionChannel>,
    },
    /// A dissolved Part loses its inherited state.
    PartStateRemoved {
        /// Dissolved Part whose inherited state is removed.
        part: DeletionObject,
        /// Original Part enabled state, before dissolution.
        enabled: bool,
        /// Whether the removed Part had parameter-driven drawing order.
        animated_order: bool,
    },
}
impl DeletionKind {
    /// Internal identities folded under their visible business owner in review.
    pub fn is_internal(self) -> bool {
        matches!(
            self,
            Self::Axis
                | Self::Sample
                | Self::GluePair
                | Self::MotionKey
                | Self::ExpressionKey
                | Self::MeshBinding
                | Self::SceneBinding
                | Self::BlendBinding
                | Self::GlueBinding
        )
    }
}
