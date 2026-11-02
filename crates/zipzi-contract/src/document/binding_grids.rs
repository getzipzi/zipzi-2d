#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Parameter range captured for a binding conversion or reparenting plan.
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
/// Policy for dependent sample coordinates when changing a parameter range or mapping.
pub enum ParameterRangePolicy {
    /// Keep numeric coordinates unchanged; validation rejects coordinates outside the new domain.
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
/// Proposed parameter range and default change with an explicit dependent-key policy.
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
/// Edit an ordinary mesh binding grid using parameter IDs and sample coordinates.
pub enum MeshGridEdit {
    /// Create a zero-axis binding that uses the current mesh base shape until driven.
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
    },
    /// Add a driving parameter axis to the binding.
    AddAxis {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Axis definition with a parameter ID and ordered samples; axis identity is distinct
        /// from the parameter ID.
        axis: BindingAxis,
    },
    /// Reorder the axes by their driving parameter IDs.
    ReorderAxes {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Document parameter IDs in the requested axis order.
        parameter_ids: Vec<String>,
    },
    /// Insert a parameter coordinate on an existing axis.
    InsertSample {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    /// Move an existing coordinate to another value on the same axis.
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
    /// Delete the selected parameter coordinate.
    DeleteSample {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    /// Remove a parameter axis using the specified reduction.
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
/// Select the coordinate retained when an ordinary binding axis is removed.
pub enum AxisReduction {
    /// Use an existing sample coordinate; a coordinate absent from the axis is rejected.
    Slice(
        /// Parameter coordinate at which to slice the old grid while removing an axis.
        f32,
    ),
    /// Evaluate at a coordinate within the grid domain, including between existing samples.
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
/// Edit an ordinary binding grid using stable axis and sample identities.
pub enum GridOperation {
    /// Add an axis and convert the existing grid for that dimension.
    AddAxis {
        /// Axis definition with a parameter ID and ordered samples; axis identity is distinct
        /// from the parameter ID.
        axis: BindingAxis,
    },
    /// Change the axis parameter with an explicit coordinate mapping policy.
    RemapParameter {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: ParameterRangePolicy,
    },
    /// Assign or clear the driving parameter of an existing axis.
    SetParameter {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit parameter-domain reduction used when removing or replacing an axis.
        reduction: Option<AxisReduction>,
    },
    /// Reorder the grid using stable axis IDs.
    ReorderAxes {
        /// Axis IDs in the requested new axis order; IDs refer to axes, not parameters.
        axis_ids: Vec<String>,
    },
    /// Insert a new sample with the supplied identity and coordinate.
    InsertSample {
        /// Stable axis ID owned by this binding, not a parameter or sample ID.
        axis_id: String,
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    /// Change the coordinate of an existing stable sample.
    MoveSample {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    /// Remove an existing stable sample.
    DeleteSample {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
    },
    /// Remove the axis or keep it pending, with explicit reduction when required.
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
/// Edit samples, neutral base or driving parameter of a BlendShape key table.
pub enum BlendTableOperation {
    /// Insert a stable sample into the key table.
    Insert {
        /// New stable sample UUID and finite parameter coordinate to insert into this key table.
        sample: BindingSample,
    },
    /// Change an existing sample coordinate.
    Move {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
        /// Scalar value in the referenced parameter domain.
        value: f32,
    },
    /// Remove a sample, supplying a replacement if its base role requires one.
    Delete {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
        /// Replacement base sample ID when deleting the current base; None supplies no replacement.
        replacement_base: Option<String>,
    },
    /// Select an existing sample as the neutral base.
    SetBase {
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
    },
    /// Assign the table driving parameter.
    SetParameter {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
    },
    /// Change the driving parameter using the chosen coordinate mapping policy.
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
/// Create or edit BlendShape tables, constraints and target bindings.
pub enum BlendAuthoringEdit {
    /// Create a key table with its samples and base.
    CreateTable {
        /// Complete BlendShape key table with stable sample IDs and an explicit base sample.
        table: BlendShapeKeyTable,
    },
    /// Create a constraint with weighted samples.
    CreateConstraint {
        /// Complete BlendShape constraint with its driving parameter and weighted samples.
        constraint: BlendShapeConstraint,
    },
    /// Create a target binding to a BlendShape key table.
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
    /// Edit an existing key table.
    Table {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Typed sample/base/parameter edit to perform on this BlendShape key table.
        operation: BlendTableOperation,
    },
    /// Replace the driving parameter and weighted samples of a constraint.
    Constraint {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Replacement ordered parameter samples and constraint weights.
        samples: Vec<ConstraintSample>,
    },
    /// Switch a binding to another key table, optionally resetting its shape data.
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
