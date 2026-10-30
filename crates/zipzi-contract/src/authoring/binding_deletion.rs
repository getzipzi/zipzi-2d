#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum BindingAuthoringEdit {
    Grid(
        /// Typed intent for ordinary binding-grid edits; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        BindingGridEdit,
    ),
    Blend(
        /// Typed intent for BlendShape authoring; apply/plan it against a Session/Edit source
        /// rather than mutating this value.
        BlendAuthoringEdit,
    ),
    MeshGrid(
        /// Typed intent for mesh binding grid edits; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        MeshGridEdit,
    ),
    ParameterRange(
        /// Typed intent for parameter range conversion; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        ParameterRangeEdit,
    ),
    Reparent(
        /// Typed intent for deformation reparenting; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        ReparentEdit,
    ),
}

#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
pub enum BindingFieldPatch {
    SampleValue(#[doc = "Scalar value in the referenced parameter domain."] f32),
    ConstraintSample {
        /// Scalar value in the referenced parameter domain.
        value: f32,
        /// Dimensionless interpolation weight applied to the referenced source vertex.
        weight: f32,
    },
    AxisParameter {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Explicit parameter-domain reduction used when removing or replacing an axis.
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
pub struct DeletionIntent {
    /// Exact typed roots, scope options and conversion resolutions to prepare.
    pub request: crate::document::DeletionRequest,
    #[serde(default)]
    /// Exact export regressions accepted after reviewing this source.
    pub accepted_export_failures: Vec<crate::project::OpaqueExportIssue>,
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
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
pub enum DeletionRow {
    Removal { /// Deleted identity and its ownership/scope/dependency reason.
        removal: DeletionRemoval },
    Relation { /// Native source field and target explaining a retained rewrite or dependency.
        relation: DeletionRelation },
    Effect { /// Typed appearance, animation, composition or export consequence.
        effect: DeletionEffect },
    Decision { /// Outstanding native choice and actually available alternatives.
        decision: DeletionDecision },
    Lock { /// Affected identity and the inherited lock source.
        blocker: DeletionLock },
    Structural { /// Native invalid-result address and diagnostic code.
        blocker: DeletionStructuralBlocker },
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
    Organization {
        /// Surviving organization member.
        object: DeletionObject,
        /// Retained Part owner, or empty for an unassigned member.
        parent_id: String,
        /// Explicit Offscreen mapping slot count, without copying its full array.
        mapping_slots: Option<usize>,
    },
    GuaranteeLimit {
        /// Object whose standalone position proof is limited by another migration.
        object: DeletionObject,
        /// Retained ancestor with an explicitly chosen local-data migration.
        ancestor: DeletionObject,
    },
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
