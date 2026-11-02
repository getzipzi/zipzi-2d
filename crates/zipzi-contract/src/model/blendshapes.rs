#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Selects the target domain and corresponding delta-keyform payload of a BlendShape binding.
pub enum BlendShapeTargetKind {
    /// Target a mesh.
    Mesh,
    /// Target a warp transform.
    Warp,
    /// Target a rotation transform.
    Rotation,
    /// Target a Part.
    Part,
    /// Target a Glue connection.
    Glue,
    /// Target an Offscreen group.
    Offscreen,
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
/// Ordered sample identities and neutral sample for a BlendShape-driving parameter.
pub struct BlendShapeKeyTable {
    #[serde(default)]
    /// Ordered stable parameter samples with strictly increasing finite values; base_sample_id
    /// names the neutral sample.
    pub samples: Vec<BindingSample>,
    #[serde(default)]
    /// Sample ID defining the neutral/base shape in this key table.
    pub base_sample_id: String,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(skip)]
    pub base_key_idx: usize,
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
/// Parameter-driven weights that attenuate a BlendShape contribution.
pub struct BlendShapeConstraint {
    #[serde(default)]
    /// Ordered parameter samples with weights used to attenuate the BlendShape contribution.
    pub samples: Vec<ConstraintSample>,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(skip)]
    pub weights: Vec<f32>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Mesh displacement and optional appearance deltas at one BlendShape sample.
pub struct DeltaMeshKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Per-vertex XY displacement vectors in mesh vertex order.
    pub positions: Vec<Vec2>,
    /// Additive opacity delta; zero is neutral. None supplies no delta for this channel.
    pub opacity: Option<f32>,
    /// Additive drawing-order delta; zero is neutral. None supplies no delta for this channel.
    pub draw_order: Option<f32>,
    /// Additive RGB multiply-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub multiply: Option<[f32; 3]>,
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Warp lattice displacements and optional appearance deltas at one BlendShape sample.
pub struct DeltaWarpKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Per-point XY displacement vectors in row-major warp point order.
    pub points: Vec<Vec2>,
    /// Additive opacity delta; zero is neutral. None supplies no delta for this channel.
    pub opacity: Option<f32>,
    /// Additive RGB multiply-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub multiply: Option<[f32; 3]>,
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Rotation-transform and appearance deltas at one BlendShape sample.
pub struct DeltaRotationKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Translation delta of the rotation origin in parent coordinates. None supplies no delta
    /// for this channel.
    pub origin: Option<Vec2>,
    /// Additive rotation-angle delta in degrees. None supplies no delta for this channel.
    pub angle: Option<f32>,
    /// Additive scale delta; zero is neutral. None supplies no delta for this channel.
    pub scale: Option<f32>,
    /// Additive opacity delta; zero is neutral. None supplies no delta for this channel.
    pub opacity: Option<f32>,
    /// Additive RGB multiply-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub multiply: Option<[f32; 3]>,
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Part drawing-order delta at one BlendShape sample.
pub struct DeltaPartKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Additive drawing-order delta; zero is neutral.
    pub draw_order: f32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Glue intensity delta at one BlendShape sample.
pub struct DeltaGlueKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Additive Glue intensity delta; zero is neutral.
    pub intensity: f32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Offscreen appearance contribution at one BlendShape sample.
pub struct DeltaOffscreenKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Additive opacity delta; zero is neutral.
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RGB multiply factors stored as multipliers with neutral (1, 1, 1); None supplies
    /// no multiply-color delta. Native conversion subtracts 1 before additive blending.
    pub multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "items", rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Typed BlendShape sample payload; choose the variant matching the binding target domain.
pub enum DeltaKeyforms {
    /// Select mesh displacement samples.
    Mesh(
        /// Ordered Mesh BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
        Vec<DeltaMeshKeyform>,
    ),
    /// Select warp lattice displacement samples.
    Warp(
        /// Ordered Warp BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
        Vec<DeltaWarpKeyform>,
    ),
    /// Select rotation-transform delta samples.
    Rotation(
        /// Rotation and appearance delta keyforms, each keyed by stable sample_id.
        Vec<DeltaRotationKeyform>,
    ),
    /// Select Part drawing-order delta samples.
    Part(
        /// Part drawing-order delta keyforms, each keyed by stable sample_id.
        Vec<DeltaPartKeyform>,
    ),
    /// Select Glue intensity delta samples.
    Glue(
        /// Glue intensity delta keyforms, each keyed by stable sample_id.
        Vec<DeltaGlueKeyform>,
    ),
    /// Select Offscreen appearance samples.
    Offscreen(
        /// Offscreen appearance keyforms keyed by stable sample_id; opacity and screen are
        /// additive deltas, while multiply retains a neutral-one factor.
        Vec<DeltaOffscreenKeyform>,
    ),
}

impl DeltaKeyforms {
    pub fn len(&self) -> usize {
        match self {
            DeltaKeyforms::Mesh(v) => v.len(),
            DeltaKeyforms::Warp(v) => v.len(),
            DeltaKeyforms::Rotation(v) => v.len(),
            DeltaKeyforms::Part(v) => v.len(),
            DeltaKeyforms::Glue(v) => v.len(),
            DeltaKeyforms::Offscreen(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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
/// Binds a target to a BlendShape key table and optional constraints using typed delta shapes.
pub struct BlendShapeBinding {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable object ID driven by this binding; the declared target kind determines its domain.
    pub target_id: String,
    /// Category of the BlendShape target; must agree with target_id and keyform payload kind.
    pub target_kind: BlendShapeTargetKind,
    /// Stable BlendShape key-table ID controlling the binding.
    pub key_table_id: String,
    /// Ordered IDs of BlendShape constraints applied to this binding.
    pub constraint_ids: Vec<String>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: DeltaKeyforms,
}
