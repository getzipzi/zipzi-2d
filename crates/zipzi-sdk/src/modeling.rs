//! Shared modeling queries, frozen sampling and domain plans.
use serde::{Deserialize, Serialize};
/// Select explicit record IDs; empty lists return collection identities only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape)]
pub struct ModelingQuery {
    pub assets: Vec<String>,
    pub meshes: Vec<String>,
    pub parameters: Vec<String>,
    pub parts: Vec<String>,
    pub transforms: Vec<String>,
    pub bindings: Vec<String>,
    pub scene_bindings: Vec<String>,
    pub blend_key_tables: Vec<String>,
    pub blend_constraints: Vec<String>,
    pub blend_bindings: Vec<String>,
    pub glues: Vec<String>,
    pub offscreens: Vec<String>,
    pub parameter_groups: Vec<String>,
    pub attachments: Vec<String>,
}
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
}
/// Owned selected query results. No mutation or query behavior lives in this DTO.
#[derive(Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct OwnedModelingRecords {
    pub document_id: String,
    pub setup: zipzi_core::ModelSetup,
    pub hierarchy_order: zipzi_core::document::HierarchyOrder,
    pub editor_state: zipzi_core::document::EditorState,
    pub display_info: zipzi_core::document::DisplayInfo,
    pub model3_settings: zipzi_core::document::Model3Settings,
    pub draw_order_groups: Option<Vec<zipzi_core::draw_order::DrawOrderGroup>>,
    pub ids: std::collections::BTreeMap<String, Vec<String>>,
    pub assets: Vec<Option<zipzi_core::ImageAsset>>,
    pub meshes: Vec<Option<zipzi_core::Mesh>>,
    pub parameters: Vec<Option<zipzi_core::Parameter>>,
    pub parts: Vec<Option<zipzi_core::Part>>,
    pub transforms: Vec<Option<zipzi_core::Transform>>,
    pub bindings: Vec<Option<zipzi_core::MeshBinding>>,
    pub scene_bindings: Vec<Option<zipzi_core::SceneBinding>>,
    pub blend_key_tables: Vec<Option<zipzi_core::BlendShapeKeyTable>>,
    pub blend_constraints: Vec<Option<zipzi_core::BlendShapeConstraint>>,
    pub blend_bindings: Vec<Option<zipzi_core::BlendShapeBinding>>,
    pub glues: Vec<Option<zipzi_core::Glue>>,
    pub offscreens: Vec<Option<zipzi_core::Offscreen>>,
    pub parameter_groups: Vec<Option<zipzi_core::document::CdiParameterGroup>>,
    pub attachments: Vec<Option<zipzi_core::document::PackageAttachment>>,
}
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
    // IO selection identities are metadata; animation record/edit APIs remain
    // separate from the modeling projection.
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
    }
}
#[derive(Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ModelingSamples {
    pub version: crate::Version,
    pub parameters: Vec<zipzi_core::PreviewValues>,
    pub frames: Vec<zipzi_core::DrawableFrame>,
    pub diagnostics: Vec<crate::GeometryDiagnostic>,
}
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
        let check = || {
            control
                .check()
                .map_err(|s| crate::SdkError::from_status(s, "sample_modeling", vec![]))
        };
        check()?;
        let diagnostics = self.diagnose_geometry_with_control(checks, control)?;
        let mut frames = Vec::with_capacity(parameters.len());
        for p in &parameters {
            check()?;
            frames.push(self.evaluate(p)?);
        }
        check()?;
        Ok(ModelingSamples {
            version: self.version(),
            parameters,
            frames,
            diagnostics,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "domain",
    content = "intent",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[derive(zipzi_api_derive::ApiShape)]
pub enum ModelingIntent {
    Scene(crate::SceneEdit),
    Organization(crate::OrganizationEdit),
    Binding(crate::BindingAuthoringEdit),
    Content(crate::SceneContentEdit),
    References(crate::ReferenceEdit),
    Resource(crate::ResourceEdit),
    Document(crate::DocumentAuthoringEdit),
    Attachment(zipzi_core::document::AttachmentEdit),
}
#[derive(Debug, Clone)]
pub enum ModelingPlan {
    Scene(crate::SceneEditPlan),
    Organization(crate::OrganizationEditPlan),
    Binding(crate::BindingEditPlan),
    Content(crate::SceneContentEditPlan),
    References(crate::ReferenceEditPlan),
    Resource(crate::ResourceEditPlan),
    Document(crate::DocumentEditPlan),
    Attachment(crate::AttachmentEditPlan),
}
zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize)]
#[derive(zipzi_api_derive::BinaryOutput)]
pub struct ModelingPlanReport {
    pub version: crate::Version,
    pub affected_ids: Vec<String>,
    pub copied_ids: std::collections::BTreeMap<String, String>,
    pub cost: Option<crate::ConversionCost>,
    pub losses: Vec<String>,
    pub no_op: Option<bool>,
}
}
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
                }
            };
        }
        match self {
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
            Self::Attachment(p) => ModelingPlanReport {
                version: p.version(),
                affected_ids: vec![],
                copied_ids: Default::default(),
                cost: None,
                losses: vec![],
                no_op: None,
            },
        }
    }
    pub fn apply(&self, edit: &mut crate::EditSession<'_>) -> Result<(), crate::SdkError> {
        match self {
            Self::Scene(p) => edit.apply_scene_plan(p),
            Self::Organization(p) => edit.apply_organization_plan(p),
            Self::Binding(p) => edit.apply_binding_plan(p),
            Self::Content(p) => edit.apply_scene_content_plan(p),
            Self::References(p) => edit.apply_reference_plan(p),
            Self::Resource(p) => edit.apply_resource_plan(p),
            Self::Document(p) => edit.apply_document_plan(p),
            Self::Attachment(p) => edit.apply_attachment_plan(p),
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
