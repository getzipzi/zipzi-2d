#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum BindingGridTarget {
    Mesh(#[doc = "Stable ID of the target mesh whose ordinary grid is being created."] String),
    Scene(
        /// Stable ID of the target scene transform or Part whose ordinary grid is being created.
        String,
    ),
    Glue(#[doc = "Stable ID of the target Glue whose ordinary grid is being created."] String),
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
pub enum BindingGridEdit {
    /// Explicitly discard owned shapes and return to the unchanged base target.
    Delete {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: BindingGridTarget,
    },
    Edit {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Typed axis/sample grid edit to perform on this owner; not a Runtime operation ID.
        operation: GridOperation,
    },
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
pub enum ReparentStrategy {
    KeepLocalData,
    PreserveEditState,
    PreserveParameterDomain,
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
pub struct ReparentEdit {
    /// Stable IDs of objects to reparent; source order is retained in the planned selection.
    pub sources: Vec<String>,
    /// Empty means the model root. Part organization is an independent relation.
    pub parent_id: String,
    /// Explicit reparenting/conversion strategy, including how shape differences are preserved
    /// or reported.
    pub strategy: ReparentStrategy,
    /// Optional placement in the resulting deformation collection.
    #[serde(default)]
    pub placement: Option<ReparentPlacement>,
    /// Absolute position error in model units, before Glue postprocessing.
    pub tolerance: f64,
    /// Relative error times the original affected geometry's model-space scale.
    #[serde(default = "default_relative_tolerance")]
    pub relative_tolerance: f64,
    /// Required for preservation. These are frozen native parameter inputs;
    /// Motion/Physics must first be resolved to parameters by the caller.
    pub context: Option<ReparentContext>,
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
pub struct ReparentPlacement {
    /// None appends; Some names an unselected member of the destination.
    pub before: Option<String>,
}
fn default_relative_tolerance() -> f64 {
    1e-6
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
pub struct ReparentContext {
    /// Parameter-ID-to-value map used for the reparenting reference pose; values use each
    /// parameter domain.
    pub parameter_values: crate::evaluation::PreviewValues,
    /// Empty for PreserveEditState. For domain preservation every unfixed
    /// dependency must have an explicit finite interval.
    pub parameter_domain: Vec<ConversionDomain>,
    /// Finite reference pose time in seconds; zero is the default reference time.
    pub edit_time_seconds: f64,
}
impl ReparentEdit {
    pub fn keep_local(sources: Vec<String>, parent_id: String) -> Self {
        Self {
            sources,
            parent_id,
            strategy: ReparentStrategy::KeepLocalData,
            placement: None,
            tolerance: 1e-5,
            relative_tolerance: 1e-6,
            context: None,
        }
    }
}

impl MeshGridEdit {
    pub fn binding_id(&self) -> &str {
        match self {
            Self::Create { id, .. } => id,
            Self::AddAxis { binding_id, .. }
            | Self::ReorderAxes { binding_id, .. }
            | Self::InsertSample { binding_id, .. }
            | Self::MoveSample { binding_id, .. }
            | Self::DeleteSample { binding_id, .. }
            | Self::RemoveAxis { binding_id, .. } => binding_id,
        }
    }
}
