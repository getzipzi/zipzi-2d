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
/// Native identity domain used in frozen deletion reports and typed roots.
pub enum DeletionKind {
    /// Identifies a drawable mesh.
    Mesh,
    /// Identifies a connection between mesh endpoints.
    Glue,
    /// Identifies a Part-owned Offscreen composite.
    Offscreen,
    /// Identifies an ordinary mesh binding.
    MeshBinding,
    /// Identifies an ordinary Glue intensity binding.
    GlueBinding,
    /// Identifies a BlendShape target binding.
    BlendBinding,
    /// Identifies an axis owned by a binding grid.
    Axis,
    /// Identifies a sample owned by a binding axis, BlendShape table or constraint.
    Sample,
    /// Identifies a vertex pair owned by a Glue.
    GluePair,
    /// Identifies a model3 hit-area registration.
    HitArea,
    /// Identifies a drawing-group role of a Part or root.
    DrawingGroup,
    /// Identifies an organizational Part.
    Part,
    /// Identifies a deformation transform.
    Deformer,
    /// Identifies an ordinary scene binding.
    SceneBinding,
    /// Identifies a motion clip asset.
    Motion,
    /// Identifies a track owned by a motion clip.
    MotionTrack,
    /// Identifies a key owned by a motion track.
    MotionKey,
    /// Identifies an expression asset.
    Expression,
    /// Identifies a track in expression source data.
    ExpressionTrack,
    /// Identifies a key in expression source data.
    ExpressionKey,
    /// Identifies an entry owned by a Pose group.
    PoseEntry,
    /// Identifies a link owned by a Pose entry.
    PoseLink,
    /// Identifies a CDI Part metadata entry.
    CdiPart,
    /// Identifies a CDI display metadata.
    DisplayInfo,
    /// Identifies an authored parameter.
    Parameter,
    /// Identifies a parameter organization folder.
    ParameterFolder,
    /// Identifies a document image resource.
    Texture,
    /// Identifies a package attachment.
    Attachment,
    /// Identifies a BlendShape key table.
    BlendKeyTable,
    /// Identifies a BlendShape constraint.
    BlendConstraint,
    /// Identifies a motion registration group.
    MotionGroup,
    /// Identifies a motion registration referencing a clip.
    MotionRegistration,
    /// Identifies an event owned by a motion clip.
    MotionEvent,
    /// Identifies an entry owned by an expression.
    ExpressionEntry,
    /// Identifies a Pose singleton.
    Pose,
    /// Identifies a group owned by Pose.
    PoseGroup,
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
    /// Identifies a model3 settings metadata.
    ModelSettings,
    /// Identifies a model3 parameter metadata group.
    ModelParameterGroup,
    /// Identifies a member of a model3 parameter group.
    ModelParameterMember,
    /// Identifies a CDI combined-parameter set.
    CdiCombinedSet,
    /// Identifies a member of a CDI combined-parameter set.
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
/// Typed native identity used by a deletion report; embedded identities may require an owner.
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
    /// Delete a Mesh and its owned identities, retaining shared resources.
    Mesh(
        /// Delete this Mesh and its owned identities; shared resources remain.
        String,
    ),
    /// Delete a Glue connection and owned content, retaining endpoint meshes.
    Glue(
        /// Delete this Glue connection, pairs and owned bindings; endpoint meshes remain.
        String,
    ),
    /// Delete an Offscreen and its content while retaining its Part.
    Offscreen(
        /// Delete this Offscreen composite and its own shapes/bindings; its Part remains.
        String,
    ),
    /// Delete a Part organization subtree; migrate external deformation members.
    Part(
        /// Delete this Part organization subtree; external deformation members remain and migrate.
        String,
    ),
    /// Delete a deformation subtree, including meshes across Parts.
    Deformer(
        /// Delete this deformation subtree, including meshes across organization Parts.
        String,
    ),
    /// Remove a Part and migrate its retained organization members.
    DissolvePart(
        /// Remove this Part while retaining and migrating its organization members.
        String,
    ),
    /// Remove only a Deformer and migrate retained deformation children.
    RemoveDeformer(
        /// Remove only this Deformer and retain/migrate its deformation children.
        String,
    ),
    /// Delete a parameter and reduce dependent axes at a frozen slice.
    Parameter(
        /// Delete this parameter and reduce ordinary axes at its frozen default unless explicitly overridden.
        String,
    ),
    /// Dissolve a parameter folder and retain its contents.
    ParameterFolder(
        /// Dissolve this parameter folder and retain its parameters and nested folders in the retained parent.
        String,
    ),
    /// Delete a parameter folder subtree including its parameters.
    ParameterFolderContents(
        /// Delete this parameter folder subtree and explicitly include its contained parameters.
        String,
    ),
    /// Delete an image resource with explicit choices for retained users.
    Texture(
        /// Delete this image record; retained users require an explicit replacement or unconfigured choice.
        String,
    ),
    /// Delete an attachment and clear resolved optional references.
    Attachment(
        /// Delete this owned attachment record and clear resolved optional sound/UserData references.
        String,
    ),
    /// Delete a typed non-scene record subject to its native ownership rules.
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
/// Typed roots and explicit resolutions used to prepare an atomic deletion review.
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
/// Absolute and scale-relative error limits used by deletion conversion proofs.
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
/// Retained Part and optional mapping for a surviving Offscreen during deletion.
pub struct DeletionOffscreenPlacement {
    /// Retained destination Part UUID; it must have an available Offscreen ownership role.
    pub part_id: String,
    /// Exact mapping in the destination Part's keyform order, if rebinding is required.
    pub mapping: Option<Vec<i32>>,
}
