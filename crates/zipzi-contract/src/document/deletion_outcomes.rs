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
pub enum DeletionChoice {
    PreserveEditState,
    KeepLocalData,
    ChoosePart,
    SupplyOffscreenMapping,
    ChooseTexture,
    ChooseBlendBase,
    ClearTexture,
    ChoosePhysicsNode,
    DeleteSubtree,
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
pub enum DeletionDecisionReason {
    ConversionUnproven {
        /// Native conversion diagnostic code; available resolutions come from typed choices.
        code: String,
    },
    OffscreenOwnerUnavailable,
    OffscreenMappingRequired,
    PartStateUnpreserved,
    TextureReplacementRequired,
    BlendBaseReplacementRequired,
    PhysicsNodeReplacementRequired,
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
pub enum DeletionField {
    Owned,
    MeshMask {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    MeshInvertedMask,
    OffscreenMask {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    GlueMeshA,
    GlueMeshB,
    HitAreaMesh,
    DrawingItem {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    OrganizationParent,
    DeformationParent,
    ConvertedData,
    PosePart,
    PoseLinkPart,
    MotionPart,
    ExpressionSourcePart,
    CdiPart {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    RequestedScope,
    ParameterAxis,
    BlendTableParameter,
    BlendConstraintParameter,
    BlendTable,
    BlendConstraint {
        /// Zero-based constraint address in the frozen BlendBinding.
        slot: usize,
    },
    MotionParameter,
    ExpressionParameter,
    ExpressionSourceParameter,
    PhysicsParameter,
    PhysicsNode,
    Texture,
    AttachmentSound,
    AttachmentUserData,
    MotionClip,
    MotionRegistrationContainer,
    ParameterFolder,
    ModelParameter,
    CdiParameter {
        /// Frozen zero-based CDI Parameters record address; imported unresolved records remain untouched.
        slot: usize,
    },
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
pub enum DeletionReason {
    Requested,
    Owned,
    Dependency,
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
pub enum DeletionChannel {
    Appearance,
    Enabled,
    GluePostprocessing,
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
pub enum DeletionEffect {
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
    BindingContributionRemoved {
        /// Retained target whose authored contribution disappears.
        object: DeletionObject,
        /// Deleted ordinary or Blend binding that previously supplied this contribution.
        binding: DeletionObject,
    },
    ResourceReplaced {
        /// Retained consumer whose texture or optional attachment reference changes.
        object: DeletionObject,
        /// Deleted resource previously referenced by this consumer.
        source: DeletionObject,
        /// Retained replacement resource; None means an explicit clear/unconfigured result.
        replacement: Option<DeletionObject>,
    },
    ParameterAnimationRemoved(
        /// Removed Motion/Expression parameter record; other tracks, events and resource registrations remain.
        DeletionObject,
    ),
    PhysicsChannelRemoved {
        /// Removed input or output channel, with a stable native identity.
        object: DeletionObject,
        /// Retained Physics setting group whose behavior changes.
        group: DeletionObject,
    },
    MaskChanged(
        /// Retained Mask consumer whose clipping effect changes.
        DeletionObject,
    ),
    GlueRemoved(
        /// Deleted connection whose seam or postprocessing contribution disappears.
        DeletionObject,
    ),
    HitAreaRemoved(
        /// Deleted model3 Hit Area registration.
        DeletionObject,
    ),
    CompositionChanged(
        /// Affected Offscreen or Part whose compositing can change.
        DeletionObject,
    ),
    PartAnimationRemoved(
        /// Removed Part animation/Pose record; its container remains.
        DeletionObject,
    ),
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
    OrganizationChanged(
        /// Surviving member whose organization owner changes.
        DeletionObject,
    ),
    DeformationChanged(
        /// Surviving object whose deformation is not fully preserved.
        DeletionObject,
    ),
    UnpreservedChannels {
        /// Surviving object affected by channels outside the position proof.
        object: DeletionObject,
        /// Explicit channels not covered by the converter position guarantee.
        channels: Vec<DeletionChannel>,
    },
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
