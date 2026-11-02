#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub struct Version {
    pub session_id: u64,
    pub generation: u64,
    pub revision: u64,
}

impl crate::api_shape::ApiShape for Version {
    fn api_shape() -> crate::api_shape::Shape {
        crate::api_shape::Shape::Opaque {
            name: concat!(module_path!(), "::Version").into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// Input for creating a document parameter; the host validates its range and supplies omitted defaults.
pub struct ParameterCreateSpec {
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Lower endpoint of the parameter domain, in model-defined parameter units.
    pub minimum: f32,
    /// Upper endpoint of the parameter domain, in the same units as minimum.
    pub maximum: f32,
    /// Default parameter value in its declared domain; native validation checks it against
    /// minimum and maximum.
    pub default_value: f32,
    /// Optional new parameter UUID; None allocates a fresh UUID when the create operation executes.
    pub id: Option<String>,
    /// Optional external runtime identifier; None or an empty string uses the parameter's
    /// stable ID when creation executes.
    pub runtime_id: Option<String>,
    /// Optional parameter kind; None uses Normal.
    pub kind: Option<ParameterKind>,
    /// Optional decimal precision for runtime parameter handling; None uses the native default
    /// of 6.
    pub decimal_places: Option<i32>,
    /// Optional range-repeat flag; None uses the native default false.
    pub repeat: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// Partial replacement of a parameter's authored fields, applied by ParameterOperation::Patch.
/// Omitted or None fields preserve their current values; supplied values are validated as replacements.
pub struct ParameterPatch {
    /// Human-readable display name; use the stable ID for identity because names can change.
    /// None preserves the current value; it does not reset the field to its default.
    pub name: Option<String>,
    /// Lower endpoint of the parameter domain, in model-defined parameter units. None preserves
    /// the current value; it does not reset the field to its default.
    pub minimum: Option<f32>,
    /// Upper endpoint of the parameter domain, in the same units as minimum. None preserves the
    /// current value; it does not reset the field to its default.
    pub maximum: Option<f32>,
    /// Default parameter value in its declared domain; native validation checks it against
    /// minimum and maximum. None preserves the current value; it does not reset the field to
    /// its default.
    pub default_value: Option<f32>,
    /// Replacement external runtime identifier; an empty string is rejected by native validation.
    /// None preserves the current value; it does not reset the field to its default.
    pub runtime_id: Option<String>,
    /// Replacement parameter kind. None preserves the current value; it does not reset the
    /// field to its default.
    pub kind: Option<ParameterKind>,
    /// Replacement decimal precision for runtime parameter handling. None preserves the current
    /// value; it does not reset the field to its default.
    pub decimal_places: Option<i32>,
    /// Replacement repeat flag; false explicitly disables repeating. None preserves the current
    /// value; it does not reset the field to its default.
    pub repeat: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// Parameter creation or partial replacement in an Edit candidate; publication is separate.
pub enum ParameterOperation {
    /// Create a parameter from its specification, allocating an ID when none is supplied.
    Create {
        /// Creation specification; native admission supplies identities/defaults and rejects
        /// invalid input.
        spec: ParameterCreateSpec,
    },
    /// Replace only the supplied fields of the parameter selected by stable document ID.
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Typed patch for the addressed object; apply through the Edit to mutate candidate state.
        patch: ParameterPatch,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// One driving parameter and its ordered sample coordinates for a new ordinary mesh binding.
pub struct MeshBindingAxisSpec {
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    /// Strictly increasing finite parameter sample values for this axis, in parameter units;
    /// not sample indices.
    pub keys: Vec<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// An ordinary mesh shape at one combination of parameter coordinates, using absolute vertex positions.
pub struct MeshKeyformSpec {
    /// One actual parameter sample value per binding axis, in axis order; each must match a
    /// declared axis sample.
    pub keys: Vec<f32>,
    /// Absolute mesh-local positions, one per target mesh vertex in vertex_ids order; not
    /// displacement vectors.
    pub positions: Vec<Vec2>,
    /// Optional finite nonnegative opacity factor; None uses the native neutral default 1.
    /// Values above 1 are retained for import roundtrips.
    pub opacity: Option<f32>,
    /// Optional RGB multiply factors; None uses neutral (1, 1, 1).
    pub multiply: Option<[f32; 3]>,
    /// Optional RGB screen factors; None uses neutral (0, 0, 0).
    pub screen: Option<[f32; 3]>,
    /// Optional keyform drawing-order coordinate; None leaves the order unspecified for native
    /// evaluation.
    pub draw_order: Option<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// Input for an ordinary mesh binding, including its target, axes and shape samples.
pub struct MeshBindingCreateSpec {
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Ordered distinct Normal-parameter axes; a driven binding supports at most 16 axes. Each has finite,
    /// increasing keys within its parameter range. Keyform coordinates follow this order.
    pub axes: Vec<MeshBindingAxisSpec>,
    /// For driven axes, supply exactly one shape per Cartesian key combination; input order
    /// is normalized with the first axis varying fastest. With no active axes this must be empty.
    pub keyforms: Vec<MeshKeyformSpec>,
    /// Optional new binding UUID; None allocates a fresh UUID when the create operation executes.
    pub id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
/// Create an ordinary mesh binding or write a shape sample in an Edit candidate.
pub enum MeshBindingOperation {
    /// Create a mesh binding and allocate its identity if omitted.
    Create {
        /// Creation specification; native admission supplies identities/defaults and rejects
        /// invalid input.
        spec: MeshBindingCreateSpec,
    },
    /// Insert or replace the shape sample at the supplied axis coordinates.
    SetKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: MeshKeyformSpec,
    },
}
