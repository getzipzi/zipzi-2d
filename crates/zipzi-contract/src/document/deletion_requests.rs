#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionKind {
    Mesh,
    Glue,
    Offscreen,
    MeshBinding,
    GlueBinding,
    BlendBinding,
    Axis,
    Sample,
    GluePair,
    HitArea,
    DrawingGroup,
    Part,
    Deformer,
    SceneBinding,
    Motion,
    MotionTrack,
    MotionKey,
    Expression,
    ExpressionTrack,
    ExpressionKey,
    PoseEntry,
    PoseLink,
    CdiPart,
    DisplayInfo,
    Parameter,
    ParameterFolder,
    Texture,
    Attachment,
    BlendKeyTable,
    BlendConstraint,
    MotionGroup,
    MotionRegistration,
    MotionEvent,
    ExpressionEntry,
    Pose,
    PoseGroup,
    Physics,
    PhysicsGroup,
    PhysicsInput,
    PhysicsOutput,
    PhysicsNode,
    ModelSettings,
    ModelParameterGroup,
    ModelParameterMember,
    CdiCombinedSet,
    CdiCombinedMember,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionObject {
    /// Native identity domain; Part and its DrawingGroup role remain distinct.
    pub kind: DeletionKind,
    /// Persistent native ID; embedded identities also require their owner namespace.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional owning Expression ID for embedded source-scene track/key identities.
    pub owner_id: Option<String>,
}

/// Typed deletion scopes and native business identities.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionTarget {
    Mesh(
        /// Delete this Mesh and its owned identities; shared resources remain.
        String,
    ),
    Glue(
        /// Delete this Glue connection, pairs and owned bindings; endpoint meshes remain.
        String,
    ),
    Offscreen(
        /// Delete this Offscreen composite and its own shapes/bindings; its Part remains.
        String,
    ),
    Part(
        /// Delete this Part organization subtree; external deformation members remain and migrate.
        String,
    ),
    Deformer(
        /// Delete this deformation subtree, including meshes across organization Parts.
        String,
    ),
    DissolvePart(
        /// Remove this Part while retaining and migrating its organization members.
        String,
    ),
    RemoveDeformer(
        /// Remove only this Deformer and retain/migrate its deformation children.
        String,
    ),
    Parameter(
        /// Delete this parameter and reduce ordinary axes at its frozen default unless explicitly overridden.
        String,
    ),
    ParameterFolder(
        /// Dissolve this parameter folder and retain its parameters and nested folders in the retained parent.
        String,
    ),
    ParameterFolderContents(
        /// Delete this parameter folder subtree and explicitly include its contained parameters.
        String,
    ),
    Texture(
        /// Delete this image record; retained users require an explicit replacement or unconfigured choice.
        String,
    ),
    Attachment(
        /// Delete this owned attachment record and clear resolved optional sound/UserData references.
        String,
    ),
    Record(
        /// Typed non-scene business record; its native domain validates ownership, child identities and choices.
        DeletionObject,
    ),
}
impl DeletionTarget {
    pub fn object(&self) -> DeletionObject {
        let (kind, id) = match self {
            Self::Parameter(id) => (DeletionKind::Parameter, id),
            Self::ParameterFolder(id) | Self::ParameterFolderContents(id) => {
                (DeletionKind::ParameterFolder, id)
            }
            Self::Texture(id) => (DeletionKind::Texture, id),
            Self::Attachment(id) => (DeletionKind::Attachment, id),
            Self::Record(object) => return object.clone(),
            Self::Mesh(id) => (DeletionKind::Mesh, id),
            Self::Glue(id) => (DeletionKind::Glue, id),
            Self::Offscreen(id) => (DeletionKind::Offscreen, id),
            Self::Part(id) | Self::DissolvePart(id) => (DeletionKind::Part, id),
            Self::Deformer(id) | Self::RemoveDeformer(id) => (DeletionKind::Deformer, id),
        };
        DeletionObject {
            kind,
            id: id.clone(),
            owner_id: None,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionRequest {
    /// Explicit typed roots and individual scope choices; order does not change the result.
    pub roots: Vec<DeletionTarget>,
    #[serde(default)]
    /// Explicit migration strategy by surviving object ID; KeepLocalData on a dissolved Part accepts its state loss.
    pub reparent: BTreeMap<String, ReparentStrategy>,
    #[serde(default)]
    /// Frozen parameter inputs, finite domains and reference time; None uses document defaults and full declared domains.
    pub conversion_context: Option<ReparentContext>,
    #[serde(default)]
    /// Explicit retained Part owners and mapping resolutions for surviving Offscreens.
    pub offscreen_placements: BTreeMap<String, DeletionOffscreenPlacement>,
    #[serde(default)]
    /// Absolute and geometry-scale-relative position error allowed by conversion proofs.
    pub tolerance: DeletionTolerance,
    #[serde(default)]
    /// Explicit finite parameter slice coordinates; absent entries use frozen document defaults, never preview sliders.
    pub parameter_slices: BTreeMap<String, f32>,
    #[serde(default)]
    /// Resolution by deleted texture UUID; Some redirects to a retained texture and None explicitly leaves users unconfigured.
    pub texture_replacements: BTreeMap<String, Option<String>>,
    #[serde(default)]
    /// Explicit replacement by removed Physics node UUID; replacement must remain in that native setting group.
    pub physics_node_replacements: BTreeMap<String, String>,
    #[serde(default)]
    /// Explicit retained base sample when deleting a Blend table base.
    pub blend_base_replacements: BTreeMap<String, String>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionTolerance {
    /// Finite positive absolute error tolerance in model units.
    pub absolute: f64,
    /// Finite nonnegative tolerance times the original geometry scale.
    pub relative: f64,
}
impl Default for DeletionTolerance {
    fn default() -> Self {
        Self {
            absolute: 1e-3,
            relative: 1e-6,
        }
    }
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
pub struct DeletionOffscreenPlacement {
    /// Retained destination Part UUID; it must have an available Offscreen ownership role.
    pub part_id: String,
    /// Exact mapping in the destination Part's keyform order, if rebinding is required.
    pub mapping: Option<Vec<i32>>,
}
