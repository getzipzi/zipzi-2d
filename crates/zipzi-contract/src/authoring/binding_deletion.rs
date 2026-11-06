#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Typed binding, parameter-range and reparenting intents for planning or editing.
pub enum BindingAuthoringEdit {
    /// Edit an ordinary binding grid.
    Grid(
        /// Typed intent for ordinary binding-grid edits; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        BindingGridEdit,
    ),
    /// Edit BlendShape authoring data.
    Blend(
        /// Typed intent for BlendShape authoring; apply/plan it against a Session/Edit source
        /// rather than mutating this value.
        BlendAuthoringEdit,
    ),
    /// Change a parameter range and its dependent coordinates.
    ParameterRange(
        /// Typed intent for parameter range conversion; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        ParameterRangeEdit,
    ),
    /// Plan a deformation reparenting operation.
    Reparent(
        /// Typed intent for deformation reparenting; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        ReparentEdit,
    ),
}

#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
/// Replacement value for a selected binding field address.
pub enum BindingFieldPatch {
    /// Replace a sample coordinate.
    SampleValue(#[doc = "Scalar value in the referenced parameter domain."] f32),
    /// Replace a constraint sample coordinate and weight.
    ConstraintSample {
        /// Scalar value in the referenced parameter domain.
        value: f32,
        /// Dimensionless BlendShape constraint weight at this parameter sample.
        weight: f32,
    },
    /// Change the parameter assigned to an axis.
    AxisParameter {
        /// Stable document parameter ID; an empty string clears the axis parameter.
        parameter_id: String,
        /// Required reduction when clearing an active axis; otherwise no reduction is applied.
        reduction: Option<AxisReduction>,
    },
}

#[derive(
    Debug,
    Clone,
    Default,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Deletion request and explicitly accepted export regressions to review against one source.
pub struct DeletionIntent {
    /// Exact typed roots, scope options and conversion resolutions to prepare.
    pub request: crate::document::DeletionRequest,
    #[serde(default)]
    /// Exact export regressions accepted after reviewing this source.
    pub accepted_export_failures: Vec<crate::project::OpaqueExportIssue>,
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
/// Position-preservation proof and tolerance recorded for a deletion migration.
pub struct DeletionPositionGuarantee {
    /// Native conversion strategy; position preservation has this strategy's scope.
    pub strategy: crate::document::ReparentStrategy,
    /// Absolute tolerance in model units, before Glue postprocessing.
    pub tolerance: f64,
    /// Scale-relative tolerance used by the native proof.
    pub relative_tolerance: f64,
    /// Maximum observed conversion error in the checked reference data.
    pub measured_error: f64,
    /// Native conservative error bound; it does not certify unlisted channels.
    pub error_bound: f64,
    /// Number of explicitly declared parameter-domain intervals; values remain in the plan.
    pub domain_parameters: usize,
    /// Frozen reference-pose time, if the conversion used a context.
    pub edit_time_seconds: Option<f64>,
}
}
crate::api_union! {
tag = "kind";
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
/// One typed row from a frozen deletion-review section.
pub enum DeletionRow {
    /// An identity proposed for removal.
    Removal { /// Deleted identity and its ownership/scope/dependency reason.
        removal: DeletionRemoval },
    /// A retained reference or dependency relation.
    Relation { /// Native source field and target explaining a retained rewrite or dependency.
        relation: DeletionRelation },
    /// A predicted semantic or presentation consequence.
    Effect { /// Typed appearance, animation, composition or export consequence.
        effect: DeletionEffect },
    /// An outstanding choice and available alternatives.
    Decision { /// Outstanding native choice and actually available alternatives.
        decision: DeletionDecision },
    /// A lock blocking deletion.
    Lock { /// Affected identity and the inherited lock source.
        blocker: DeletionLock },
    /// A structural blocker of a valid result.
    Structural { /// Native invalid-result address and diagnostic code.
        blocker: DeletionStructuralBlocker },
    /// A surviving deformation object and its migration.
    Deformation {
        /// Surviving object whose deformation parent changes.
        object: DeletionObject,
        /// Retained destination parent; empty means the model root.
        parent_id: String,
        /// Requested and validated conversion strategy.
        strategy: crate::document::ReparentStrategy,
        /// Position proof only, without copying the frozen model parameter map.
        guarantee: Option<DeletionPositionGuarantee>,
    },
    /// A surviving organization member and its migration.
    Organization {
        /// Surviving organization member.
        object: DeletionObject,
        /// Retained Part owner, or empty for an unassigned member.
        parent_id: String,
        /// Explicit Offscreen mapping slot count, without copying its full array.
        mapping_slots: Option<usize>,
    },
    /// An ancestor limiting a standalone preservation guarantee.
    GuaranteeLimit {
        /// Object whose standalone position proof is limited by another migration.
        object: DeletionObject,
        /// Retained ancestor with an explicitly chosen local-data migration.
        ancestor: DeletionObject,
    },
    /// A parameter interval or fixed input used by a conversion proof.
    Domain {
        /// Object whose preservation proof uses this parameter interval or fixed input.
        object: DeletionObject,
        /// Actual dependency parameter, not an unrelated model parameter.
        parameter_id: String,
        /// Lower endpoint of the proved domain; equal to maximum for a fixed input.
        minimum: f32,
        /// Upper endpoint of the proved domain.
        maximum: f32,
        /// Frozen reference value used by the conversion.
        reference_value: f32,
    },
    /// An existing or newly introduced export failure.
    Export {
        /// Resource, format and exact native export regression.
        failure: crate::project::OpaqueExportIssue,
        /// True for pre-existing export problems; false for new regressions.
        existing: bool,
    },
}
}
crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
/// Bounded rows from one immutable deletion-review section; offsets are row positions, not IDs.
pub struct DeletionPage {
    /// Report section in the frozen plan.
    pub section: DeletionSection,
    /// Requested zero-based row offset.
    pub offset: usize,
    /// Total rows available in this section.
    pub total: usize,
    /// Bounded owned rows from the captured source, not current live records.
    pub rows: Vec<DeletionRow>,
    /// Next offset in the same immutable section, or None at the end.
    pub next_offset: Option<usize>,
}
}
