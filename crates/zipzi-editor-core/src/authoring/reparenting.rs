//! Frozen preservation requests. Native preflight remains cheap; conversion runs
//! against one immutable snapshot, and publication applies exactly that plan.
use super::*;
use anyhow::{Result, ensure};

use zipzi_contract::document::ReparentEdit;
use zipzi_contract::document::ReparentPlacement;
use zipzi_contract::document::ReparentStrategy;
use zipzi_contract::document::SceneCollection;
use zipzi_contract::document::SceneEdit;
use zipzi_core::document::{ConversionDomain, ReparentContext};
use zipzi_core::state::{ConversionControl, ConversionLimits};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalettePreservation {
    ParameterDomain,
    EditState,
}

#[derive(Clone, Debug)]
pub struct PaletteReparentCapture {
    source: crate::runtime_host::HostedSource,
    limits: ConversionLimits,
    context: ReparentContext,
    preview_revision: u64,
}
#[derive(Clone, Debug)]
pub struct PaletteReparentPlan {
    scene: crate::EditPreview<zipzi_core::state::SceneOutcome>,
    preview_revision: u64,
}
impl PaletteReparentPlan {
    pub fn scene(&self) -> &crate::EditPreview<zipzi_core::state::SceneOutcome> {
        &self.scene
    }
}
impl PaletteReparentCapture {
    pub fn plan(
        &self,
        request: PaletteMove,
        scope: PalettePreservation,
        control: &ConversionControl,
    ) -> Result<PaletteReparentPlan> {
        let tolerance = zipzi_contract::document::DeletionTolerance::default();
        self.plan_with_tolerance(
            request,
            scope,
            tolerance.absolute,
            tolerance.relative,
            control,
        )
    }
    pub fn plan_with_tolerance(
        &self,
        request: PaletteMove,
        scope: PalettePreservation,
        tolerance: f64,
        relative_tolerance: f64,
        control: &ConversionControl,
    ) -> Result<PaletteReparentPlan> {
        let mut edit = reparent_edit(
            self.source.captured.snapshot.document(),
            &request,
            self.context.clone(),
            scope,
        )?;
        edit.tolerance = tolerance;
        edit.relative_tolerance = relative_tolerance;
        Ok(PaletteReparentPlan {
            scene: {
                let limits = self.limits;
                crate::EditPreview::convert(
                    &self.source,
                    "Reparent scene",
                    limits,
                    control,
                    move |commands, control| {
                        commands.edit_scene(SceneEdit::Reparent { edit }, limits, control)
                    },
                )?
            },
            preview_revision: self.preview_revision,
        })
    }
}

pub(super) fn deformation_destination(
    model: &zipzi_core::state::DocumentState,
    request: &PaletteMove,
) -> Result<(SceneCollection, Option<String>)> {
    let parent = match (&request.target, request.position) {
        (Some(target), PalettePosition::Inside) => {
            ensure!(
                target.kind == ObjectKind::Deformer && model.transform(&target.id).is_some(),
                "Target is not a deformer"
            );
            target.id.clone()
        }
        (Some(target), _) => model
            .scene_deformer_parent(&target.id)
            .ok_or_else(|| anyhow::anyhow!("Target has no deformation collection"))?
            .to_owned(),
        (None, _) => String::new(),
    };
    let collection = SceneCollection::Deformation {
        parent_id: (!parent.is_empty()).then_some(parent),
    };
    let before = if let Some(target) = request
        .target
        .as_ref()
        .filter(|_| request.position != PalettePosition::Inside)
    {
        let mut members = model
            .scene_collection(&collection)
            .map_err(|s| super::stepping::PaletteMovePlanningError(s))?;
        if matches!(
            &collection,
            SceneCollection::Deformation { parent_id: None }
        ) {
            members.retain(|id| {
                model.transform(id).is_some() == (target.kind == ObjectKind::Deformer)
            });
        }
        super::moving::insertion(
            &members,
            &request
                .sources
                .iter()
                .map(|o| o.id.clone())
                .collect::<Vec<_>>(),
            target,
            request.position,
        )?
    } else {
        None
    };
    Ok((collection, before))
}
pub(super) fn reparent_edit(
    model: &zipzi_core::state::DocumentState,
    request: &PaletteMove,
    mut context: ReparentContext,
    scope: PalettePreservation,
) -> Result<ReparentEdit> {
    ensure!(
        request.projection == PaletteProjection::Deformation,
        "Expected deformation projection"
    );
    ensure!(
        !request.sources.is_empty()
            && request
                .sources
                .iter()
                .all(|s| matches!(s.kind, ObjectKind::Mesh | ObjectKind::Deformer)),
        "Select meshes or deformers"
    );
    for source in &request.sources {
        ensure!(
            match source.kind {
                ObjectKind::Mesh => model.mesh(&source.id).is_some(),
                ObjectKind::Deformer => model.transform(&source.id).is_some(),
                _ => false,
            },
            "Source identity changed"
        );
    }
    let (SceneCollection::Deformation { parent_id }, before) =
        deformation_destination(model, request)?
    else {
        unreachable!()
    };
    if scope == PalettePreservation::EditState {
        context.parameter_domain.clear();
    }
    let tolerance = zipzi_contract::document::DeletionTolerance::default();
    Ok(ReparentEdit {
        sources: request.sources.iter().map(|s| s.id.clone()).collect(),
        parent_id: parent_id.unwrap_or_default(),
        strategy: match scope {
            PalettePreservation::ParameterDomain => ReparentStrategy::PreserveParameterDomain,
            PalettePreservation::EditState => ReparentStrategy::PreserveEditState,
        },
        placement: Some(ReparentPlacement { before }),
        tolerance: tolerance.absolute,
        relative_tolerance: tolerance.relative,
        context: Some(context),
    })
}
impl Document {
    pub(super) fn preservation_context(&self) -> ReparentContext {
        let parameters = self.parameters();
        ReparentContext {
            parameter_values: parameters.iter().map(|p| (p.id.clone(), p.value)).collect(),
            parameter_domain: parameters
                .iter()
                .map(|p| ConversionDomain {
                    parameter_id: p.id.clone(),
                    minimum: p.minimum,
                    maximum: p.maximum,
                })
                .collect(),
            edit_time_seconds: 0.,
        }
    }
    pub fn capture_palette_reparent(&self) -> Result<PaletteReparentCapture> {
        self.ensure_authoring_idle()?;
        Ok(PaletteReparentCapture {
            source: self.session.capture_authoring(),
            limits: zipzi_core::state::ConversionLimits::default(),
            context: self.preservation_context(),
            preview_revision: self.preview_revision(),
        })
    }
    pub fn execute_palette_reparent(
        &mut self,
        plan: &PaletteReparentPlan,
    ) -> Result<OperationResult> {
        ensure!(
            self.preview_revision() == plan.preview_revision,
            "Frozen parameter values changed; reopen the move dialog"
        );
        self.execute_scene_authoring(&plan.scene)
    }
}
impl crate::Editor {
    pub fn execute_palette_reparent(
        &mut self,
        document: crate::DocumentId,
        plan: &PaletteReparentPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(document, "Palette reparent")?;
        ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling),
            "Modeling context changed"
        );
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|d| d.execute_palette_reparent(plan))
    }
}
