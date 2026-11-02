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
/// Readiness of a frozen deletion review; review and confirmation do not themselves apply it.
pub enum DeletionReviewState {
    /// A validated executable candidate is ready without additional review choices.
    Ready,
    /// An executable candidate has consequences requiring explicit confirmation before applying.
    NeedsConfirmation,
    /// Outstanding decisions or unaccepted export regressions require a revised request.
    NeedsChoice,
    /// Locks or structural blockers prevent an executable candidate.
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
/// Select one independently paged section of a frozen deletion review.
pub enum DeletionSection {
    /// Business identities proposed for removal.
    Removed,
    /// Owned internal identities removed with their owners.
    Owned,
    /// References proposed for rewriting.
    Rewritten,
    /// Predicted semantic and presentation effects.
    Effects,
    /// Outstanding choices and their available alternatives.
    Decisions,
    /// Locks preventing the deletion.
    Locks,
    /// Structural blockers preventing a valid result.
    Structure,
    /// Surviving objects with deformation migration.
    Deformation,
    /// Surviving objects with organization migration.
    Organization,
    /// Limits on deformation-preservation guarantees.
    GuaranteeLimits,
    /// Frozen parameter inputs and intervals used by conversions.
    ConversionDomain,
    /// Existing and newly introduced export failures.
    Exports,
    /// References and dependencies relevant to the review.
    Relations,
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
/// Row count for one section of a frozen deletion review.
pub struct DeletionSectionCount {
    /// Frozen report section addressed by report_page.
    pub section: DeletionSection,
    /// Total rows in this section, before paging.
    pub count: usize,
}
}
crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
/// Readiness and section counts of a frozen deletion review.
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
/// Frozen summary of a planned modeling operation, including predicted effects and losses.
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
