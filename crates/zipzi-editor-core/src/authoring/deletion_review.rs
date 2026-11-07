//! Bounded read-only pages over one captured deletion report.
use std::{collections::BTreeSet, sync::Arc};
use zipzi_contract::authoring::{
    DeletionPage, DeletionPositionGuarantee, DeletionReviewState, DeletionRow, DeletionSection,
    DeletionSectionCount, DeletionSummary,
};
use zipzi_core::document::*;
use zipzi_project::OpaqueExportIssue as DeletionExportFailure;
#[derive(Debug, Clone)]
pub(super) struct DeletionReview {
    native: DeletionReport,
    new_export_failures: BTreeSet<DeletionExportFailure>,
    existing_export_failures: BTreeSet<DeletionExportFailure>,
    export_analysis_complete: bool,
    state: DeletionReviewState,
    index: Arc<DeletionPageIndex>,
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
    fn build(analysis: &DeletionReview) -> Self {
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
    fn admission_bytes(analysis: &DeletionReview) -> usize {
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
impl DeletionReview {
    pub(super) fn build(
        native: DeletionReport,
        before: BTreeSet<DeletionExportFailure>,
        after: Option<BTreeSet<DeletionExportFailure>>,
        accepted: BTreeSet<DeletionExportFailure>,
        limits: zipzi_core::state::ConversionLimits,
    ) -> Result<Self, zipzi_core::Status> {
        let complete = after.is_some();
        let failures: BTreeSet<_> = after
            .unwrap_or_default()
            .difference(&before)
            .cloned()
            .collect();
        let state = if !native.blockers.is_empty() || !native.structural_blockers.is_empty() {
            DeletionReviewState::Blocked
        } else if !native.decisions.is_empty() || !failures.is_subset(&accepted) {
            DeletionReviewState::NeedsChoice
        } else if native.requires_confirmation() || !failures.is_empty() {
            DeletionReviewState::NeedsConfirmation
        } else {
            DeletionReviewState::Ready
        };
        let mut review = Self {
            native,
            new_export_failures: failures,
            existing_export_failures: before,
            export_analysis_complete: complete,
            state,
            index: Arc::new(DeletionPageIndex {
                removed: vec![],
                owned: vec![],
                limits: vec![],
                domains: vec![],
                exports: vec![],
            }),
        };
        limits.check_conversion(0, DeletionPageIndex::admission_bytes(&review))?;
        review.index = Arc::new(DeletionPageIndex::build(&review));
        Ok(review)
    }
    pub(super) fn executable(&self) -> bool {
        self.export_analysis_complete
            && matches!(
                self.state,
                DeletionReviewState::Ready | DeletionReviewState::NeedsConfirmation
            )
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
            requested: self.native.requested.len(),
            sections,
            export_analysis_complete: self.export_analysis_complete,
        }
    }
    fn section_count(&self, section: DeletionSection) -> usize {
        let r = &self.native;
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
    ) -> Result<DeletionPage, zipzi_core::Status> {
        if !(1..=256).contains(&limit) {
            return Err(zipzi_core::Status::error(
                "INVALID_PAGE_SIZE",
                "Deletion pages contain 1 to 256 rows",
            ));
        }
        let total = self.section_count(section);
        if offset > total {
            return Err(zipzi_core::Status::error(
                "INVALID_PAGE_OFFSET",
                "Deletion offset exceeds the section",
            ));
        }
        let r = &self.native;
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
}
