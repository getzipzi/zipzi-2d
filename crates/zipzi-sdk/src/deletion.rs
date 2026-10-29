//! Native dependency cleanup, bound to an exact SDK/Core source. These APIs
//! are also coordinated through Runtime-owned modeling plans and report pages.
use crate::{
    AuthoringSession, AuthoringSnapshot, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use std::collections::BTreeSet;
mod review;
pub use review::*;
use zipzi_core::document::{AuthoringLimits, ConversionControl, ConversionCost};
pub use zipzi_core::document::{
    DeletionChannel, DeletionChoice, DeletionDecision, DeletionDecisionReason, DeletionEffect,
    DeletionField, DeletionKind, DeletionLock, DeletionMigration, DeletionObject,
    DeletionOffscreenPlacement, DeletionOrganizationMigration, DeletionReason, DeletionRelation,
    DeletionRemoval, DeletionReport, DeletionRequest, DeletionStructuralBlocker, DeletionTarget,
    DeletionTolerance,
};

pub use zipzi_project::{OpaqueExportIssue as DeletionExportFailure, OpaqueExportProblem};
#[derive(Debug, Clone, Default)]
pub struct DeletionOptions {
    /// Exact export regressions explicitly accepted by the caller after review.
    pub accepted_export_failures: BTreeSet<DeletionExportFailure>,
}
#[derive(Debug, Clone)]
pub struct DeletionAnalysis {
    pub native: DeletionReport,
    /// Existing failures of this same namespace guard are not new regressions.
    pub new_export_failures: BTreeSet<DeletionExportFailure>,
    pub existing_export_failures: BTreeSet<DeletionExportFailure>,
    pub export_analysis_complete: bool,
}
#[derive(Debug, Clone)]
pub struct DeletionPlan {
    version: Version,
    edit_source: Option<(std::sync::Arc<()>, u64)>,
    control: ConversionControl,
    domain: zipzi_core::document::DeletionPlan,
    analysis: std::sync::Arc<DeletionAnalysis>,
}
impl DeletionPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn report(&self) -> &DeletionAnalysis {
        &self.analysis
    }
    pub fn cost(&self) -> ConversionCost {
        self.domain.cost()
    }
}
fn error(status: zipzi_core::Status) -> SdkError {
    SdkError::from_status(status, "deletion", vec![])
}
fn analyze(
    document: &zipzi_core::Document,
    request: &DeletionRequest,
    limits: AuthoringLimits,
    control: &ConversionControl,
) -> Result<DeletionAnalysis, SdkError> {
    control.check().map_err(error)?;
    let prepared = document
        .prepare_deletion(request, limits, control)
        .map_err(error)?;
    let existing_export_failures =
        zipzi_project::preflight_opaque_exports(document, control).map_err(error)?;
    let new_export_failures = if let Some(plan) = &prepared.plan {
        zipzi_project::preflight_opaque_exports(plan.result(), control)
            .map_err(error)?
            .difference(&existing_export_failures)
            .cloned()
            .collect()
    } else {
        BTreeSet::new()
    };
    Ok(DeletionAnalysis {
        native: prepared.report,
        export_analysis_complete: prepared.plan.is_some(),
        existing_export_failures,
        new_export_failures,
    })
}
fn plan(
    document: &zipzi_core::Document,
    version: Version,
    request: &DeletionRequest,
    options: &DeletionOptions,
    limits: AuthoringLimits,
    control: &ConversionControl,
) -> Result<DeletionPlan, SdkError> {
    control.check().map_err(error)?;
    // Admit the source and metadata analysis copies before constructing them.
    limits
        .check_conversion(0, 0, document.estimated_content_bytes())
        .map_err(error)?;
    let domain = document
        .plan_deletion(request, limits, control)
        .map_err(error)?;
    let existing_export_failures =
        zipzi_project::preflight_opaque_exports(document, control).map_err(error)?;
    let after = zipzi_project::preflight_opaque_exports(domain.result(), control).map_err(error)?;
    let new_export_failures = after
        .difference(&existing_export_failures)
        .cloned()
        .collect();
    let analysis = DeletionAnalysis {
        native: domain.report().clone(),
        existing_export_failures,
        new_export_failures,
        export_analysis_complete: true,
    };
    if !analysis
        .new_export_failures
        .is_subset(&options.accepted_export_failures)
    {
        return Err(SdkError::new(
            "DELETION_EXPORT_DECISION_REQUIRED",
            "Review and explicitly accept the reported export failures",
            "plan_deletion",
        ));
    }
    Ok(DeletionPlan {
        version,
        edit_source: None,
        control: control.clone(),
        domain,
        analysis: std::sync::Arc::new(analysis),
    })
}
impl AuthoringSession {
    /// Translate epoch-checked handles to native deletion roots.
    pub fn deletion_request(&self, roots: &[ObjectHandle]) -> Result<DeletionRequest, SdkError> {
        let roots = roots
            .iter()
            .map(|h| {
                self.resolve_handle(h)?;
                Ok(match h.kind() {
                    ObjectKind::Mesh => DeletionTarget::Mesh(h.id().into()),
                    ObjectKind::Glue => DeletionTarget::Glue(h.id().into()),
                    ObjectKind::Offscreen => DeletionTarget::Offscreen(h.id().into()),
                    ObjectKind::Part => DeletionTarget::Part(h.id().into()),
                    ObjectKind::Transform => DeletionTarget::Deformer(h.id().into()),
                    ObjectKind::Asset => DeletionTarget::Texture(h.id().into()),
                    ObjectKind::Parameter => DeletionTarget::Parameter(h.id().into()),
                    ObjectKind::CdiParameterGroup => DeletionTarget::ParameterFolder(h.id().into()),
                    ObjectKind::Attachment => DeletionTarget::Attachment(h.id().into()),
                    ObjectKind::MeshBinding => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::MeshBinding,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::BindingAxis => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::Axis,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::BindingSample => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::Sample,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::SceneBinding => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::SceneBinding,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::BlendKeyTable => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::BlendKeyTable,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::BlendConstraint => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::BlendConstraint,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::BlendBinding => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::BlendBinding,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::GlueVertexPair => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::GluePair,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::DrawOrderGroup => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::DrawingGroup,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::CdiCombinedSet => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::CdiCombinedSet,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::CdiCombinedMember => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::CdiCombinedMember,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::ModelParameterGroup => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::ModelParameterGroup,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::ModelParameterMember => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::ModelParameterMember,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::ModelHitArea => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::HitArea,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::Expression => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::Expression,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::ExpressionEntry => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::ExpressionEntry,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::Motion => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::Motion,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::MotionGroup => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::MotionGroup,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::MotionRegistration => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::MotionRegistration,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::MotionTrack => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::MotionTrack,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::MotionKey => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::MotionKey,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::MotionEvent => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::MotionEvent,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::Pose => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::Pose,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PoseGroup => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PoseGroup,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PoseEntry => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PoseEntry,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PoseLink => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PoseLink,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::Physics => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::Physics,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PhysicsGroup => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PhysicsGroup,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PhysicsInput => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PhysicsInput,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PhysicsOutput => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PhysicsOutput,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                    ObjectKind::PhysicsNode => DeletionTarget::Record(DeletionObject {
                        kind: DeletionKind::PhysicsNode,
                        id: h.id().into(),
                        owner_id: None,
                    }),
                })
            })
            .collect::<Result<_, SdkError>>()?;
        Ok(DeletionRequest {
            roots,
            ..Default::default()
        })
    }
    pub fn analyze_deletion(
        &self,
        request: &DeletionRequest,
    ) -> Result<DeletionAnalysis, SdkError> {
        self.authoring_limits()
            .check_conversion(0, 0, self.document().estimated_content_bytes())
            .map_err(error)?;
        analyze(
            self.document(),
            request,
            self.authoring_limits(),
            &ConversionControl::default(),
        )
    }
    pub fn plan_deletion(
        &self,
        request: &DeletionRequest,
        options: &DeletionOptions,
    ) -> Result<DeletionPlan, SdkError> {
        plan(
            self.document(),
            self.version(),
            request,
            options,
            self.authoring_limits(),
            &ConversionControl::default(),
        )
    }
    pub fn execute_deletion_plan(&mut self, plan: &DeletionPlan) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit(
            "Delete and clean dependencies",
            Some(plan.version),
            |edit| edit.apply_deletion_plan(plan),
        )?;
        Ok(receipt)
    }
}
impl AuthoringSnapshot {
    pub fn plan_deletion_with_control(
        &self,
        request: &DeletionRequest,
        options: &DeletionOptions,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<DeletionPlan, SdkError> {
        let mut plan = plan(
            self.document(),
            self.version(),
            request,
            options,
            limits,
            control,
        )?;
        plan.edit_source = self.edit_source.clone();
        Ok(plan)
    }
}
impl EditSession<'_> {
    pub fn analyze_deletion(
        &self,
        request: &DeletionRequest,
    ) -> Result<DeletionAnalysis, SdkError> {
        self.ensure_active("analyze_deletion")?;
        self.authoring_limits
            .check_conversion(0, 0, self.candidate_document().estimated_content_bytes())
            .map_err(error)?;
        analyze(
            self.candidate_document(),
            request,
            self.authoring_limits,
            &ConversionControl::default(),
        )
    }
    /// Plan after earlier edits against this exact candidate, not the formal revision.
    pub fn plan_deletion(
        &self,
        request: &DeletionRequest,
        options: &DeletionOptions,
    ) -> Result<DeletionPlan, SdkError> {
        self.ensure_active("plan_deletion")?;
        let mut plan = plan(
            self.candidate_document(),
            self.base_version(),
            request,
            options,
            self.authoring_limits,
            &ConversionControl::default(),
        )?;
        plan.edit_source = Some((self.identity.clone(), self.sequence));
        Ok(plan)
    }
    pub fn apply_deletion_plan(&mut self, plan: &DeletionPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(plan.version, "apply_deletion_plan", "Replan deletion")?;
        let valid = match &plan.edit_source {
            Some((id, sequence)) => {
                std::sync::Arc::ptr_eq(id, &self.identity) && *sequence == self.sequence
            }
            None => self.sequence == 0,
        };
        if !valid {
            return Err(self.abort_with(SdkError::new(
                "STALE_PLAN",
                "Deletion belongs to another Edit",
                "apply_deletion_plan",
            )));
        }
        crate::binding_authoring::check_cost(plan.cost(), self.authoring_limits)
            .map_err(|e| self.abort_with(e))?;
        self.apply_authoring_change("apply_deletion_plan", "", false, |document| {
            document.apply_deletion_plan(&plan.domain)
        })?;
        self.binding_cost = Some(plan.cost());
        self.binding_controls.push(plan.control.clone());
        Ok(())
    }
}
