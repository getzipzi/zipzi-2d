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
pub enum DeletionReviewState {
    Ready,
    NeedsConfirmation,
    NeedsChoice,
    Blocked,
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
pub enum DeletionSection {
    Removed,
    Owned,
    Rewritten,
    Effects,
    Decisions,
    Locks,
    Structure,
    Deformation,
    Organization,
    GuaranteeLimits,
    ConversionDomain,
    Exports,
    Relations,
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct DeletionSectionCount {
    /// Frozen report section addressed by report_page.
    pub section: DeletionSection,
    /// Total rows in this section, before paging.
    pub count: usize,
}
}
crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct DeletionSummary {
    /// Whether the plan is executable, requires confirmation/choices, or is blocked.
    pub state: DeletionReviewState,
    /// Explicit root count after typed identity deduplication.
    pub requested: usize,
    /// Section counts; owned internal identities are separate from business removals.
    pub sections: Vec<DeletionSectionCount>,
    /// False until a valid candidate allowed complete export preflight.
    pub export_analysis_complete: bool,
}
}

crate::api_record! {
#[derive(Debug, Clone, Serialize)]
#[derive(zipzi_api_derive::BinaryOutput)]
pub struct ModelingPlanReport {
    /// Exact SDK version captured for this value; not a best-effort latest-version marker.
    pub version: Version,
    /// Stable IDs conservatively affected by the planned operation.
    pub affected_ids: Vec<String>,
    /// Mapping from original object IDs to the new IDs allocated for copied objects.
    pub copied_ids: std::collections::BTreeMap<String, String>,
    /// Native plan cost estimates; inspect losses separately before applying the plan.
    pub cost: Option<crate::document::ConversionCost>,
    /// Explicit semantic losses predicted by conversion; empty means no losses were reported.
    pub losses: Vec<String>,
    /// Whether applying the plan would leave the document unchanged.
    pub no_op: Option<bool>,
    /// Bounded deletion readiness/count summary; detailed rows use frozen report pages.
    pub deletion: Option<DeletionSummary>,
}
}
