//! Shared modeling queries, frozen sampling and domain plans.
use serde::Serialize;
pub use zipzi_contract::authoring::ModelingQuery;

#[derive(Debug, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct ModelingRecords<'a> {
    pub document_id: &'a str,
    pub setup: zipzi_core::ModelSetup,
    pub hierarchy_order: &'a zipzi_core::document::HierarchyOrder,
    pub editor_state: &'a zipzi_core::document::EditorState,
    pub display_info: &'a zipzi_core::document::DisplayInfo,
    pub model3_settings: &'a zipzi_core::document::Model3Settings,
    pub draw_order_groups: Option<&'a [zipzi_core::draw_order::DrawOrderGroup]>,
    pub ids: std::collections::BTreeMap<&'static str, Vec<&'a str>>,
    pub assets: Vec<Option<&'a zipzi_core::ImageAsset>>,
    pub meshes: Vec<Option<&'a zipzi_core::Mesh>>,
    pub parameters: Vec<Option<&'a zipzi_core::Parameter>>,
    pub parts: Vec<Option<&'a zipzi_core::Part>>,
    pub transforms: Vec<Option<&'a zipzi_core::Transform>>,
    pub bindings: Vec<Option<&'a zipzi_core::MeshBinding>>,
    pub scene_bindings: Vec<Option<&'a zipzi_core::SceneBinding>>,
    pub blend_key_tables: Vec<Option<&'a zipzi_core::BlendShapeKeyTable>>,
    pub blend_constraints: Vec<Option<&'a zipzi_core::BlendShapeConstraint>>,
    pub blend_bindings: Vec<Option<&'a zipzi_core::BlendShapeBinding>>,
    pub glues: Vec<Option<&'a zipzi_core::Glue>>,
    pub offscreens: Vec<Option<&'a zipzi_core::Offscreen>>,
    pub parameter_groups: Vec<Option<&'a zipzi_core::document::CdiParameterGroup>>,
    pub attachments: Vec<Option<&'a zipzi_core::document::PackageAttachment>>,
    pub motions: Vec<Option<&'a zipzi_core::document::MotionClip>>,
    pub motion_groups: Vec<Option<&'a zipzi_core::document::MotionGroup>>,
    pub motion_registrations: Vec<Option<&'a zipzi_core::document::MotionRegistration>>,
    pub expressions: Vec<Option<&'a zipzi_core::document::ExpressionAsset>>,
    pub physics: Option<&'a zipzi_core::document::PhysicsAsset>,
    pub pose: Option<&'a zipzi_core::document::PoseAsset>,
}
pub use zipzi_contract::authoring::OwnedModelingRecords;
impl ModelingRecords<'_> {
    pub fn into_owned(self) -> OwnedModelingRecords {
        OwnedModelingRecords {
            model3_settings: self.model3_settings.clone(),
            draw_order_groups: self.draw_order_groups.map(<[_]>::to_vec),
            document_id: self.document_id.into(),
            setup: self.setup,
            hierarchy_order: self.hierarchy_order.clone(),
            editor_state: self.editor_state.clone(),
            display_info: self.display_info.clone(),
            ids: self
                .ids
                .into_iter()
                .map(|(k, v)| (k.into(), v.into_iter().map(String::from).collect()))
                .collect(),
            assets: self.assets.into_iter().map(|v| v.cloned()).collect(),
            meshes: self.meshes.into_iter().map(|v| v.cloned()).collect(),
            parameters: self.parameters.into_iter().map(|v| v.cloned()).collect(),
            parts: self.parts.into_iter().map(|v| v.cloned()).collect(),
            transforms: self.transforms.into_iter().map(|v| v.cloned()).collect(),
            bindings: self.bindings.into_iter().map(|v| v.cloned()).collect(),
            scene_bindings: self
                .scene_bindings
                .into_iter()
                .map(|v| v.cloned())
                .collect(),
            blend_key_tables: self
                .blend_key_tables
                .into_iter()
                .map(|v| v.cloned())
                .collect(),
            blend_constraints: self
                .blend_constraints
                .into_iter()
                .map(|v| v.cloned())
                .collect(),
            blend_bindings: self
                .blend_bindings
                .into_iter()
                .map(|v| v.cloned())
                .collect(),
            glues: self.glues.into_iter().map(|v| v.cloned()).collect(),
            offscreens: self.offscreens.into_iter().map(|v| v.cloned()).collect(),
            parameter_groups: self
                .parameter_groups
                .into_iter()
                .map(|v| v.cloned())
                .collect(),
            attachments: self.attachments.into_iter().map(|v| v.cloned()).collect(),
            motions: self.motions.into_iter().map(|v| v.cloned()).collect(),
            motion_groups: self.motion_groups.into_iter().map(|v| v.cloned()).collect(),
            motion_registrations: self
                .motion_registrations
                .into_iter()
                .map(|v| v.cloned())
                .collect(),
            expressions: self.expressions.into_iter().map(|v| v.cloned()).collect(),
            physics: self.physics.cloned(),
            pose: self.pose.cloned(),
        }
    }
}
pub fn modeling_records<'a>(
    document: &'a zipzi_core::Document,
    query: &ModelingQuery,
) -> ModelingRecords<'a> {
    let mut ids = std::collections::BTreeMap::new();
    ids.insert(
        "assets",
        document.asset_order().iter().map(String::as_str).collect(),
    );
    ids.insert(
        "meshes",
        document.mesh_order().iter().map(String::as_str).collect(),
    );
    ids.insert(
        "parameters",
        document
            .parameter_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "parts",
        document.part_order().iter().map(String::as_str).collect(),
    );
    ids.insert(
        "transforms",
        document
            .transform_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "bindings",
        document
            .binding_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "scene_bindings",
        document
            .scene_binding_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "blend_key_tables",
        document
            .blend_key_table_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "blend_constraints",
        document
            .blend_constraint_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "blend_bindings",
        document
            .blend_binding_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "glues",
        document.glue_order().iter().map(String::as_str).collect(),
    );
    ids.insert(
        "offscreens",
        document
            .offscreen_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "parameter_groups",
        document
            .display_info()
            .parameter_groups
            .iter()
            .flatten()
            .map(|g| g.id.as_str())
            .collect(),
    );
    ids.insert(
        "attachments",
        document
            .package_attachments()
            .iter()
            .map(|a| a.id.as_str())
            .collect(),
    );
    // Collection identities remain available without loading animation payloads.
    ids.insert(
        "motions",
        document.motion_order().iter().map(String::as_str).collect(),
    );
    ids.insert(
        "expressions",
        document
            .expression_order()
            .iter()
            .map(String::as_str)
            .collect(),
    );
    ids.insert(
        "motion_groups",
        document
            .motion_groups()
            .iter()
            .map(|g| g.id.as_str())
            .collect(),
    );
    ids.insert(
        "motion_registrations",
        document
            .motion_registrations()
            .map(|e| e.id.as_str())
            .collect(),
    );
    ids.insert(
        "physics",
        document
            .physics()
            .map(|v| vec![v.id.as_str()])
            .unwrap_or_default(),
    );
    ids.insert(
        "pose",
        document
            .pose()
            .map(|v| vec![v.id.as_str()])
            .unwrap_or_default(),
    );
    ModelingRecords {
        document_id: document.id(),
        setup: document.model_setup(),
        hierarchy_order: document.hierarchy_order(),
        editor_state: document.editor_state(),
        display_info: document.display_info(),
        model3_settings: document.model3_settings(),
        draw_order_groups: document.draw_order_groups(),
        ids,
        assets: query
            .assets
            .iter()
            .map(|id| document.get_asset(id))
            .collect(),
        meshes: query
            .meshes
            .iter()
            .map(|id| document.get_mesh(id))
            .collect(),
        parameters: query
            .parameters
            .iter()
            .map(|id| document.get_parameter(id))
            .collect(),
        parts: query.parts.iter().map(|id| document.get_part(id)).collect(),
        transforms: query
            .transforms
            .iter()
            .map(|id| document.get_transform(id))
            .collect(),
        bindings: query
            .bindings
            .iter()
            .map(|id| document.get_binding(id))
            .collect(),
        scene_bindings: query
            .scene_bindings
            .iter()
            .map(|id| document.get_scene_binding(id))
            .collect(),
        blend_key_tables: query
            .blend_key_tables
            .iter()
            .map(|id| document.get_blend_key_table(id))
            .collect(),
        blend_constraints: query
            .blend_constraints
            .iter()
            .map(|id| document.get_blend_constraint(id))
            .collect(),
        blend_bindings: query
            .blend_bindings
            .iter()
            .map(|id| document.get_blend_binding(id))
            .collect(),
        glues: query.glues.iter().map(|id| document.get_glue(id)).collect(),
        offscreens: query
            .offscreens
            .iter()
            .map(|id| document.get_offscreen(id))
            .collect(),
        parameter_groups: query
            .parameter_groups
            .iter()
            .map(|id| document.get_parameter_group(id))
            .collect(),
        attachments: query
            .attachments
            .iter()
            .map(|id| document.get_attachment(id))
            .collect(),
        motions: query
            .motions
            .iter()
            .map(|id| document.get_motion(id))
            .collect(),
        motion_groups: query
            .motion_groups
            .iter()
            .map(|id| document.get_motion_group(id))
            .collect(),
        motion_registrations: query
            .motion_registrations
            .iter()
            .map(|id| document.get_motion_registration(id).map(|(_, entry)| entry))
            .collect(),
        expressions: query
            .expressions
            .iter()
            .map(|id| document.get_expression(id))
            .collect(),
        physics: query.physics.then(|| document.physics()).flatten(),
        pose: query.pose.then(|| document.pose()).flatten(),
    }
}
pub use zipzi_contract::authoring::ModelingSamples;
impl crate::AuthoringSnapshot {
    /// All samples use the exact immutable source and explicit parameter maps.
    pub fn sample_modeling(
        &self,
        parameters: Vec<zipzi_core::PreviewValues>,
        checks: crate::GeometryChecks,
    ) -> Result<ModelingSamples, crate::SdkError> {
        self.sample_modeling_with_control(parameters, checks, &crate::ConversionControl::default())
    }
    pub fn sample_modeling_with_control(
        &self,
        parameters: Vec<zipzi_core::PreviewValues>,
        checks: crate::GeometryChecks,
        control: &crate::ConversionControl,
    ) -> Result<ModelingSamples, crate::SdkError> {
        let mut sampler = ModelingSampler::new(self, parameters, checks);
        loop {
            if let Some(samples) = sampler.step(control)? {
                return Ok(samples);
            }
        }
    }
}

/// Resumable sampling of one frozen source. Validation and result assembly are
/// shared with `sample_modeling_with_control`; hosts only schedule `step` calls.
/// A completed or failed sampler must be discarded.
pub struct ModelingSampler<S> {
    source: S,
    requested: Vec<zipzi_core::PreviewValues>,
    checks: crate::GeometryChecks,
    output: Option<ModelingSamples>,
    quality_reference: Option<zipzi_core::DrawableFrame>,
    finished: bool,
}
impl<S: std::borrow::Borrow<crate::AuthoringSnapshot>> ModelingSampler<S> {
    pub fn new(
        source: S,
        requested: Vec<zipzi_core::PreviewValues>,
        checks: crate::GeometryChecks,
    ) -> Self {
        Self {
            source,
            requested,
            checks,
            output: None,
            quality_reference: None,
            finished: false,
        }
    }
    /// Evaluate at most one pose. `None` means there are more poses to sample.
    pub fn step(
        &mut self,
        control: &crate::ConversionControl,
    ) -> Result<Option<ModelingSamples>, crate::SdkError> {
        let check = || {
            control
                .check()
                .map_err(|s| crate::SdkError::from_status(s, "sample_modeling", vec![]))
        };
        check()?;
        if self.finished {
            return Err(crate::SdkError::new(
                "SAMPLING_FINISHED",
                "Sampler already completed",
                "sample_modeling",
            ));
        }
        let source = self.source.borrow();
        if self.output.is_none() {
            if self.checks.quality {
                self.quality_reference = Some(source.quality_reference()?);
            }
            self.output = Some(ModelingSamples {
                version: source.version(),
                parameters: vec![],
                frames: Vec::with_capacity(self.requested.len()),
                diagnostics: source.diagnose_geometry_with_control(self.checks, control)?,
                quality: vec![],
            });
        }
        let output = self.output.as_mut().unwrap();
        if let Some(pose) = self.requested.get(output.frames.len()) {
            let frame = if self.checks.quality {
                // Hidden drawables otherwise contain zero-filled positions, which
                // would be reported as collapsed geometry against the reference.
                let mut frame = zipzi_core::DrawableFrame::default();
                let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
                    source.document(),
                    pose,
                    &mut frame,
                );
                if !status.is_ok() {
                    return Err(crate::SdkError::from_status(
                        status,
                        "sample_modeling",
                        vec![],
                    ));
                }
                frame
            } else {
                source.evaluate(pose)?
            };
            if let Some(reference) = &self.quality_reference {
                output
                    .quality
                    .push(crate::mesh_quality(&frame, reference, control)?);
            }
            output.frames.push(frame);
        }
        check()?;
        if output.frames.len() < self.requested.len() {
            return Ok(None);
        }
        output.parameters = std::mem::take(&mut self.requested);
        self.finished = true;
        Ok(self.output.take())
    }
}

pub use zipzi_contract::authoring::ModelingIntent;

#[derive(Debug, Clone)]
pub enum ModelingPlan {
    Deletion(crate::DeletionReviewPlan),
    Scene(crate::SceneEditPlan),
    Organization(crate::OrganizationEditPlan),
    Binding(crate::BindingEditPlan),
    Content(crate::SceneContentEditPlan),
    References(crate::ReferenceEditPlan),
    Resource(crate::ResourceEditPlan),
    Document(crate::DocumentEditPlan),
    Attachment(crate::AttachmentEditPlan),
    Motion(crate::MotionEditPlan),
    Expression(crate::ExpressionEditPlan),
    Physics(crate::PhysicsEditPlan),
    Pose(crate::PoseEditPlan),
}
pub use zipzi_contract::authoring::ModelingPlanReport;
impl ModelingPlan {
    pub fn report(&self) -> ModelingPlanReport {
        macro_rules! report {
            ($p:expr) => {
                ModelingPlanReport {
                    version: $p.version(),
                    affected_ids: $p.affected_ids().to_vec(),
                    copied_ids: Default::default(),
                    cost: None,
                    losses: vec![],
                    no_op: Some($p.is_noop()),
                    deletion: None,
                }
            };
        }
        match self {
            Self::Deletion(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: vec![],
                copied_ids: Default::default(),
                cost: Some(p.cost()),
                losses: vec![],
                no_op: Some(false),
                deletion: Some(p.summary()),
            },
            Self::Scene(p) => {
                let mut r = report!(p);
                r.copied_ids = p.copied_ids().clone();
                r.cost = p.cost();
                r.losses = p
                    .conversion_report()
                    .map(|r| r.losses.clone())
                    .unwrap_or_default();
                r
            }
            Self::Organization(p) => {
                let mut r = report!(p);
                r.copied_ids = p.copied_ids().clone();
                r
            }
            Self::Binding(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: p.affected_ids(),
                copied_ids: Default::default(),
                cost: Some(p.cost()),
                losses: p.losses().to_vec(),
                no_op: None,
                deletion: None,
            },
            Self::Content(p) => {
                let mut r = report!(p);
                r.cost = Some(p.cost());
                r.losses = p.report().losses.clone();
                r
            }
            Self::References(p) => {
                let mut r = report!(p);
                r.copied_ids = p.copied_ids().clone();
                r
            }
            Self::Resource(p) => {
                let mut r = report!(p);
                r.copied_ids = p.copied_ids().clone();
                r
            }
            Self::Document(p) => {
                let mut r = report!(p);
                r.losses = p.losses().to_vec();
                r
            }
            Self::Motion(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: p.affected_ids().to_vec(),
                copied_ids: Default::default(),
                cost: None,
                losses: vec![],
                no_op: None,
                deletion: None,
            },
            Self::Expression(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: p
                    .affected_expressions()
                    .iter()
                    .chain(p.affected_entries())
                    .cloned()
                    .collect(),
                copied_ids: Default::default(),
                cost: None,
                losses: vec![],
                no_op: None,
                deletion: None,
            },
            Self::Physics(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: p.affected_ids().to_vec(),
                copied_ids: Default::default(),
                cost: None,
                losses: vec![],
                no_op: None,
                deletion: None,
            },
            Self::Pose(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: p.affected_ids().to_vec(),
                copied_ids: Default::default(),
                cost: None,
                losses: vec![],
                no_op: None,
                deletion: None,
            },
            Self::Attachment(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: vec![],
                copied_ids: Default::default(),
                cost: None,
                losses: vec![],
                no_op: None,
                deletion: None,
            },
        }
    }
    pub fn apply(&self, edit: &mut crate::EditSession<'_>) -> Result<(), crate::SdkError> {
        match self {
            Self::Deletion(p) => p.apply(edit),
            Self::Scene(p) => edit.apply_scene_plan(p),
            Self::Organization(p) => edit.apply_organization_plan(p),
            Self::Binding(p) => edit.apply_binding_plan(p),
            Self::Content(p) => edit.apply_scene_content_plan(p),
            Self::References(p) => edit.apply_reference_plan(p),
            Self::Resource(p) => edit.apply_resource_plan(p),
            Self::Document(p) => edit.apply_document_plan(p),
            Self::Attachment(p) => edit.apply_attachment_plan(p),
            Self::Motion(p) => edit.apply_motion_plan(p),
            Self::Expression(p) => edit.apply_expression_plan(p),
            Self::Physics(p) => edit.apply_physics_plan(p),
            Self::Pose(p) => edit.apply_pose_plan(p),
        }
    }
}
impl crate::AuthoringSnapshot {
    pub fn plan_modeling(
        &self,
        intent: ModelingIntent,
        limits: crate::AuthoringLimits,
        control: &crate::ConversionControl,
    ) -> Result<ModelingPlan, crate::SdkError> {
        Ok(match intent {
            ModelingIntent::Deletion(i) => {
                ModelingPlan::Deletion(self.prepare_deletion_review(i, limits, control)?)
            }
            ModelingIntent::Scene(i) => {
                ModelingPlan::Scene(self.plan_scene_edit_with_control(i, limits, control)?)
            }
            ModelingIntent::Organization(i) => {
                ModelingPlan::Organization(self.plan_organization_edit(i)?)
            }
            ModelingIntent::Binding(i) => {
                ModelingPlan::Binding(self.plan_binding_authoring_with_control(i, limits, control)?)
            }
            ModelingIntent::Content(i) => {
                ModelingPlan::Content(self.plan_scene_content_with_control(i, limits, control)?)
            }
            ModelingIntent::Motion(i) => ModelingPlan::Motion(self.plan_motion_edit(i)?),
            ModelingIntent::Expression(i) => {
                ModelingPlan::Expression(self.plan_expression_edit(i)?)
            }
            ModelingIntent::Physics(i) => ModelingPlan::Physics(self.plan_physics_edit(i)?),
            ModelingIntent::Pose(i) => ModelingPlan::Pose(self.plan_pose_edit(i)?),
            ModelingIntent::References(i) => ModelingPlan::References(self.plan_reference_edit(i)?),
            ModelingIntent::Resource(i) => ModelingPlan::Resource(self.plan_resource_edit(i)?),
            ModelingIntent::Document(i) => ModelingPlan::Document(self.plan_document_edit(i)?),
            ModelingIntent::Attachment(i) => {
                ModelingPlan::Attachment(self.plan_attachment_edit(i)?)
            }
        })
    }
}

// Borrowed query encoding and the owned native projection must never drift.
const _: () = assert!(
    <ModelingRecords<'static> as zipzi_core::binary_wire::WireWrite>::FIELD_LAYOUT
        == <OwnedModelingRecords as zipzi_core::binary_wire::WireWrite>::FIELD_LAYOUT
);
