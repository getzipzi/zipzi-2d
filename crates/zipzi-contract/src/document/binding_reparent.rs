#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Select the target domain for an ordinary binding grid.
pub enum BindingGridTarget {
    /// Bind a mesh.
    Mesh(#[doc = "Stable ID of the target mesh whose ordinary grid is being created."] String),
    /// Bind a scene transform or Part.
    Scene(
        /// Stable ID of the target scene transform or Part whose ordinary grid is being created.
        String,
    ),
    /// Bind Glue intensity.
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
/// Create, edit or delete an ordinary binding grid on a selected owner.
pub enum BindingGridEdit {
    /// Explicitly discard owned shapes and return to the unchanged base target.
    Delete {
        /// Stable mesh/scene binding ID, or Glue ID for its owned grid.
        owner: String,
    },
    /// Create an ordinary grid on the selected target.
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Mesh, scene transform/Part or Glue that the new ordinary grid drives.
        target: BindingGridTarget,
    },
    /// Apply an axis or sample operation to an existing grid.
    Edit {
        /// Stable mesh/scene binding ID, or Glue ID for its owned grid.
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
/// Choose how deformation reparenting treats local shape data and preservation.
pub enum ReparentStrategy {
    /// Retain local data; world-space appearance may change.
    KeepLocalData,
    /// Attempt preservation at the frozen parameter values.
    PreserveEditState,
    /// Attempt preservation across the explicitly supplied parameter domains.
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
/// Request deformation reparenting with explicit strategy, placement and tolerances.
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
/// Optional insertion position in the destination deformation collection.
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
/// Frozen parameter values, optional domains and reference time for preservation planning.
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
