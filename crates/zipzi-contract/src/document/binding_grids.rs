#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ConversionDomain {
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    /// Lower endpoint of the parameter domain, in parameter units.
    pub minimum: f32,
    /// Upper endpoint of the parameter domain, in parameter units.
    pub maximum: f32,
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
pub enum ParameterRangePolicy {
    KeepKeyValues,
    /// Map every dependent key by the same increasing affine change of domain.
    MapKeyValues,
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
pub struct ParameterRangeEdit {
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    /// Lower endpoint of the parameter domain, in parameter units.
    pub minimum: f32,
    /// Upper endpoint of the parameter domain, in parameter units.
    pub maximum: f32,
    /// Default parameter value within the declared domain.
    pub default_value: f32,
    /// Explicit native conflict/conversion policy; no implicit removal or remapping is requested.
    pub policy: ParameterRangePolicy,
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
pub enum MeshGridEdit {
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
    },
    AddAxis {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Axis definition with a parameter ID and ordered samples; axis identity is distinct
        /// from the parameter ID.
        axis: BindingAxis,
    },
    ReorderAxes {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Document parameter IDs in the requested axis order.
        parameter_ids: Vec<String>,
    },
    InsertSample {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    MoveSample {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Existing sample value in the parameter domain, not a sample-array index.
        from: f32,
        /// New sample value in the same parameter domain.
        to: f32,
    },
    DeleteSample {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    RemoveAxis {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit parameter-domain reduction used when removing or replacing an axis.
        reduction: AxisReduction,
    },
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
pub enum AxisReduction {
    Slice(
        /// Parameter coordinate at which to slice the old grid while removing an axis.
        f32,
    ),
    Resample(
        /// Parameter coordinate used to resample the old grid while reducing an axis.
        f32,
    ),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridGuarantee {
    BaseShape,
    /// Piecewise multilinear Mesh positions, appearance and effective draw order
    /// over the old grid's effective parameter domain, with float32 arithmetic.
    PreservedOnDomain,
    /// The underlying multilinear interpolant is preserved. Runtime precision
    /// snapping around the new sample can change values within its snap window.
    InterpolantPreserved {
        parameter_id: String,
        value: f32,
    },
    ParameterPlacementChanged,
    DimensionRemoved {
        parameter_id: String,
        value: f32,
    },
    SamplesRemoved {
        parameter_id: String,
        value: f32,
    },
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
pub enum GridOperation {
    AddAxis {
        /// Axis definition with a parameter ID and ordered samples; axis identity is distinct
        /// from the parameter ID.
        axis: BindingAxis,
    },
    RemapParameter {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: ParameterRangePolicy,
    },
    SetParameter {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit parameter-domain reduction used when removing or replacing an axis.
        reduction: Option<AxisReduction>,
    },
    ReorderAxes {
        /// Axis IDs in the requested new axis order; IDs refer to axes, not parameters.
        axis_ids: Vec<String>,
    },
    InsertSample {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    MoveSample {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    DeleteSample {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
    },
    RemoveAxis {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// Explicit parameter-domain reduction used when removing or replacing an axis.
        reduction: Option<AxisReduction>,
        /// Keep an axis with no parameter as pending instead of removing it from the binding.
        keep_pending: bool,
    },
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
pub enum BlendTableOperation {
    Insert {
        /// New stable sample UUID and finite parameter coordinate to insert into this key table.
        sample: BindingSample,
    },
    Move {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    Delete {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
        /// Replacement base sample ID when deleting the current base; None supplies no replacement.
        replacement_base: Option<String>,
    },
    SetBase {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
    },
    SetParameter {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
    },
    RemapParameter {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: ParameterRangePolicy,
    },
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
pub enum BlendAuthoringEdit {
    CreateTable {
        /// Complete BlendShape key table with stable sample IDs and an explicit base sample.
        table: BlendShapeKeyTable,
    },
    CreateConstraint {
        /// Complete BlendShape constraint with its driving parameter and weighted samples.
        constraint: BlendShapeConstraint,
    },
    CreateBinding {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Stable object ID driven by this binding; the declared target kind determines its domain.
        target_id: String,
        /// Category of the BlendShape target; must agree with target_id and keyform payload kind.
        target_kind: BlendShapeTargetKind,
        /// Stable BlendShape key-table ID controlling the binding.
        key_table_id: String,
    },
    Table {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Typed sample/base/parameter edit to perform on this BlendShape key table.
        operation: BlendTableOperation,
    },
    Constraint {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Replacement ordered parameter samples and constraint weights.
        samples: Vec<ConstraintSample>,
    },
    SetBindingTable {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Stable BlendShape key-table ID controlling the binding.
        key_table_id: String,
        /// Whether to reset shape data when switching the binding to another key table.
        reset: bool,
    },
}
