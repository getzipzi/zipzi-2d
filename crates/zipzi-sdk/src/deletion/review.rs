//! Transport-sized summaries and source-bound report pages. Native addresses and
//! conversion contexts stay in the plan instead of being copied through IPC.
use super::*;
use serde::{Deserialize, Serialize};

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
    pub request: DeletionRequest,
    #[serde(default)]
    /// Exact export regressions accepted after reviewing this source.
    pub accepted_export_failures: Vec<DeletionExportFailure>,
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

zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct DeletionSectionCount {
    /// Frozen report section addressed by report_page.
    pub section: DeletionSection,
    /// Total rows in this section, before paging.
    pub count: usize,
}
}
zipzi_core::api_record! {
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
zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct DeletionPositionGuarantee {
    /// Native conversion strategy; position preservation has this strategy's scope.
    pub strategy: zipzi_core::document::ReparentStrategy,
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
zipzi_core::api_union! {
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
        strategy: zipzi_core::document::ReparentStrategy,
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
        failure: DeletionExportFailure,
        /// True for pre-existing export problems; false for new regressions.
        existing: bool,
    },
}
}
zipzi_core::api_record! {
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

#[derive(Debug, Clone)]
pub struct DeletionReviewPlan {
    version: Version,
    analysis: std::sync::Arc<DeletionAnalysis>,
    executable: Option<DeletionPlan>,
    state: DeletionReviewState,
    cost: ConversionCost,
    index: std::sync::Arc<DeletionPageIndex>,
}
#[derive(Debug)]
struct DeletionPageIndex {
    removed: Vec<usize>,
    owned: Vec<usize>,
    limits: Vec<(usize, usize)>,
    domains: Vec<(usize, String)>,
    exports: Vec<(DeletionExportFailure, bool)>,
}
impl DeletionPageIndex {
    fn build(analysis: &DeletionAnalysis) -> Self {
        let mut removed = vec![];
        let mut owned = vec![];
        for (i, row) in analysis.native.removed.iter().enumerate() {
            let internal_child = matches!(
                row.object.kind,
                DeletionKind::Axis
                    | DeletionKind::Sample
                    | DeletionKind::GluePair
                    | DeletionKind::MotionKey
                    | DeletionKind::ExpressionKey
            );
            if (row.reason == DeletionReason::Owned && row.object.kind.is_internal())
                || (internal_child && row.reason != DeletionReason::Requested)
            {
                owned.push(i);
            } else {
                removed.push(i);
            }
        }
        let limits = analysis
            .native
            .migrations
            .iter()
            .enumerate()
            .flat_map(|(i, m)| (0..m.limited_by.len()).map(move |j| (i, j)))
            .collect();
        let domains = analysis
            .native
            .migrations
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                m.guarantee
                    .as_ref()
                    .and_then(|g| g.context.as_ref())
                    .into_iter()
                    .flat_map(move |c| c.parameter_values.keys().map(move |id| (i, id.clone())))
            })
            .collect();
        let exports = analysis
            .new_export_failures
            .iter()
            .map(|v| (v.clone(), false))
            .chain(
                analysis
                    .existing_export_failures
                    .iter()
                    .map(|v| (v.clone(), true)),
            )
            .collect();
        Self {
            removed,
            owned,
            limits,
            domains,
            exports,
        }
    }
    fn admission_bytes(analysis: &DeletionAnalysis) -> usize {
        analysis
            .native
            .removed
            .len()
            .saturating_mul(std::mem::size_of::<usize>())
            .saturating_add(
                analysis
                    .native
                    .migrations
                    .iter()
                    .map(|m| {
                        m.limited_by.len().saturating_mul(16).saturating_add(
                            m.guarantee
                                .as_ref()
                                .and_then(|g| g.context.as_ref())
                                .map_or(0, |c| c.parameter_values.len().saturating_mul(128)),
                        )
                    })
                    .fold(0usize, usize::saturating_add),
            )
            .saturating_add(
                (analysis.new_export_failures.len() + analysis.existing_export_failures.len())
                    .saturating_mul(256),
            )
    }
}
impl DeletionReviewPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
    pub fn analysis(&self) -> &DeletionAnalysis {
        &self.analysis
    }
    pub fn summary(&self) -> DeletionSummary {
        use DeletionSection::*;
        let sections = [
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
        ]
        .into_iter()
        .map(|section| DeletionSectionCount {
            section,
            count: self.section_count(section),
        })
        .collect();
        DeletionSummary {
            state: self.state,
            requested: self.analysis.native.requested.len(),
            sections,
            export_analysis_complete: self.analysis.export_analysis_complete,
        }
    }
    fn section_count(&self, section: DeletionSection) -> usize {
        let r = &self.analysis.native;
        match section {
            DeletionSection::Removed => self.index.removed.len(),
            DeletionSection::Owned => self.index.owned.len(),
            DeletionSection::Rewritten => r.rewritten.len(),
            DeletionSection::Effects => r.effects.len(),
            DeletionSection::Decisions => r.decisions.len(),
            DeletionSection::Locks => r.blockers.len(),
            DeletionSection::Structure => r.structural_blockers.len(),
            DeletionSection::Deformation => r.migrations.len(),
            DeletionSection::Organization => r.organization_migrations.len(),
            DeletionSection::GuaranteeLimits => self.index.limits.len(),
            DeletionSection::ConversionDomain => self.index.domains.len(),
            DeletionSection::Exports => self.index.exports.len(),
            DeletionSection::Relations => r.relations.len(),
        }
    }
    pub fn report_page(
        &self,
        section: DeletionSection,
        offset: usize,
        limit: usize,
    ) -> Result<DeletionPage, SdkError> {
        if !(1..=256).contains(&limit) {
            return Err(SdkError::new(
                "INVALID_PAGE_SIZE",
                "Deletion pages contain 1 to 256 rows",
                "deletion_report_page",
            ));
        }
        let total = self.section_count(section);
        if offset > total {
            return Err(SdkError::new(
                "INVALID_PAGE_OFFSET",
                "Deletion offset exceeds the section",
                "deletion_report_page",
            ));
        }
        let r = &self.analysis.native;
        // Map only the requested slice: never build or clone the whole report to page it.
        macro_rules! rows {
            ($iter:expr, $map:expr) => {
                $iter.skip(offset).take(limit).map($map).collect()
            };
        }
        let rows: Vec<DeletionRow> = match section {
            DeletionSection::Removed | DeletionSection::Owned => rows!(
                (if section == DeletionSection::Owned {
                    &self.index.owned
                } else {
                    &self.index.removed
                })
                .iter(),
                |&i| DeletionRow::Removal {
                    removal: r.removed[i].clone()
                }
            ),
            DeletionSection::Rewritten => rows!(r.rewritten.iter(), |v| DeletionRow::Relation {
                relation: v.clone()
            }),
            DeletionSection::Effects => rows!(r.effects.iter(), |v| DeletionRow::Effect {
                effect: v.clone()
            }),
            DeletionSection::Decisions => rows!(r.decisions.iter(), |v| DeletionRow::Decision {
                decision: v.clone()
            }),
            DeletionSection::Locks => rows!(r.blockers.iter(), |v| DeletionRow::Lock {
                blocker: v.clone()
            }),
            DeletionSection::Structure => rows!(r.structural_blockers.iter(), |v| {
                DeletionRow::Structural { blocker: v.clone() }
            }),
            DeletionSection::Deformation => {
                rows!(r.migrations.iter(), |v| DeletionRow::Deformation {
                    object: v.object.clone(),
                    parent_id: v.parent_id.clone(),
                    strategy: v.strategy,
                    guarantee: v.guarantee.as_ref().map(|g| DeletionPositionGuarantee {
                        strategy: g.strategy,
                        tolerance: g.tolerance,
                        relative_tolerance: g.relative_tolerance,
                        measured_error: g.measured_error,
                        error_bound: g.error_bound,
                        domain_parameters: g
                            .context
                            .as_ref()
                            .map_or(0, |c| c.parameter_domain.len()),
                        edit_time_seconds: g.context.as_ref().map(|c| c.edit_time_seconds)
                    }),
                })
            }
            DeletionSection::Organization => rows!(r.organization_migrations.iter(), |v| {
                DeletionRow::Organization {
                    object: v.object.clone(),
                    parent_id: v.parent_id.clone(),
                    mapping_slots: v.offscreen_mapping.as_ref().map(Vec::len),
                }
            }),
            DeletionSection::GuaranteeLimits => rows!(self.index.limits.iter(), |&(i, j)| {
                DeletionRow::GuaranteeLimit {
                    object: r.migrations[i].object.clone(),
                    ancestor: r.migrations[i].limited_by[j].clone(),
                }
            }),
            DeletionSection::ConversionDomain => rows!(self.index.domains.iter(), |(i, id)| {
                let m = &r.migrations[*i];
                let c = m.guarantee.as_ref().unwrap().context.as_ref().unwrap();
                let value = c.parameter_values[id];
                let domain = c.parameter_domain.iter().find(|d| &d.parameter_id == id);
                DeletionRow::Domain {
                    object: m.object.clone(),
                    parameter_id: id.clone(),
                    minimum: domain.map_or(value, |d| d.minimum),
                    maximum: domain.map_or(value, |d| d.maximum),
                    reference_value: value,
                }
            }),
            DeletionSection::Exports => rows!(self.index.exports.iter(), |(failure, existing)| {
                DeletionRow::Export {
                    failure: failure.clone(),
                    existing: *existing,
                }
            }),
            DeletionSection::Relations => rows!(r.relations.iter(), |v| DeletionRow::Relation {
                relation: v.clone()
            }),
        };
        let end = offset + rows.len();
        Ok(DeletionPage {
            section,
            offset,
            total,
            rows,
            next_offset: (end < total).then_some(end),
        })
    }
    pub fn apply(&self, edit: &mut EditSession<'_>) -> Result<(), SdkError> {
        let Some(plan) = &self.executable else {
            return Err(edit.abort_with(SdkError::new(
                "DELETION_NOT_READY",
                "Resolve the frozen deletion report and replan before applying",
                "apply_deletion_review",
            )));
        };
        edit.apply_deletion_plan(plan)
    }
}

impl AuthoringSnapshot {
    pub fn prepare_deletion_review(
        &self,
        intent: DeletionIntent,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<DeletionReviewPlan, SdkError> {
        control.check().map_err(error)?;
        let bytes = self.document().estimated_content_bytes();
        limits.check_conversion(0, 0, bytes).map_err(error)?;
        let prepared = self
            .document()
            .prepare_deletion(&intent.request, limits, control)
            .map_err(error)?;
        let existing_export_failures =
            zipzi_project::preflight_opaque_exports(self.document(), control).map_err(error)?;
        let new_export_failures = if let Some(plan) = &prepared.plan {
            zipzi_project::preflight_opaque_exports(plan.result(), control)
                .map_err(error)?
                .difference(&existing_export_failures)
                .cloned()
                .collect()
        } else {
            BTreeSet::new()
        };
        let accepted: BTreeSet<_> = intent.accepted_export_failures.into_iter().collect();
        let state = if !prepared.report.blockers.is_empty()
            || !prepared.report.structural_blockers.is_empty()
        {
            DeletionReviewState::Blocked
        } else if !prepared.report.decisions.is_empty() || !new_export_failures.is_subset(&accepted)
        {
            DeletionReviewState::NeedsChoice
        } else if prepared.report.requires_confirmation() || !new_export_failures.is_empty() {
            // Accepting an export regression resolves that choice; clients
            // must still confirm deletion instead of treating it as a direct,
            // consequence-free operation after the final acceptance click.
            DeletionReviewState::NeedsConfirmation
        } else {
            DeletionReviewState::Ready
        };
        let cost = if let Some(plan) = &prepared.plan {
            plan.cost()
        } else {
            limits
                .check_conversion(0, prepared.report.estimated_bytes(), bytes)
                .map_err(error)?
        };
        let analysis = std::sync::Arc::new(DeletionAnalysis {
            native: prepared.report,
            export_analysis_complete: prepared.plan.is_some(),
            existing_export_failures,
            new_export_failures,
        });
        let payload = cost
            .payload_bytes
            .checked_add(DeletionPageIndex::admission_bytes(&analysis))
            .ok_or_else(|| {
                SdkError::new(
                    "AUTHORING_CAPACITY_OVERFLOW",
                    "Deletion report index",
                    "prepare_deletion_review",
                )
            })?;
        let cost = limits
            .check_conversion(cost.sample_combinations, payload, bytes)
            .map_err(error)?;
        let index = std::sync::Arc::new(DeletionPageIndex::build(&analysis));
        let executable = if matches!(
            state,
            DeletionReviewState::Ready | DeletionReviewState::NeedsConfirmation
        ) {
            Some(DeletionPlan {
                version: self.version(),
                edit_source: self.edit_source.clone(),
                control: control.clone(),
                domain: prepared
                    .plan
                    .expect("executable deletion has a validated candidate"),
                analysis: analysis.clone(),
            })
        } else {
            None
        };
        Ok(DeletionReviewPlan {
            version: self.version(),
            analysis,
            executable,
            state,
            cost,
            index,
        })
    }
}
