//! Serializable bulk modeling values shared by native and hosted callers.
//! Commands only dispatch to existing SDK operations; they add no validation or history.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelingCommand {
    CreateAsset {
        asset: zipzi_core::ImageAsset,
    },
    ReplaceAsset {
        asset: zipzi_core::ImageAsset,
    },
    CreateMesh {
        mesh: zipzi_core::Mesh,
    },
    ReplaceMesh {
        mesh: zipzi_core::Mesh,
    },
    CreateParameter {
        parameter: zipzi_core::Parameter,
    },
    ReplaceParameter {
        parameter: zipzi_core::Parameter,
    },
    CreatePart {
        part: zipzi_core::Part,
    },
    ReplacePart {
        part: zipzi_core::Part,
    },
    CreateTransform {
        transform: zipzi_core::Transform,
    },
    ReplaceTransform {
        transform: zipzi_core::Transform,
    },
    CreateBinding {
        binding: zipzi_core::MeshBinding,
    },
    ReplaceBinding {
        binding: zipzi_core::MeshBinding,
    },
    CreateSceneBinding {
        binding: zipzi_core::SceneBinding,
    },
    ReplaceSceneBinding {
        binding: zipzi_core::SceneBinding,
    },
    CreateBlendKeyTable {
        table: zipzi_core::BlendShapeKeyTable,
    },
    ReplaceBlendKeyTable {
        table: zipzi_core::BlendShapeKeyTable,
    },
    CreateBlendConstraint {
        constraint: zipzi_core::BlendShapeConstraint,
    },
    ReplaceBlendConstraint {
        constraint: zipzi_core::BlendShapeConstraint,
    },
    CreateBlendBinding {
        binding: zipzi_core::BlendShapeBinding,
    },
    ReplaceBlendBinding {
        binding: zipzi_core::BlendShapeBinding,
    },
    CreateGlue {
        glue: zipzi_core::Glue,
    },
    ReplaceGlue {
        glue: zipzi_core::Glue,
    },
    CreateOffscreen {
        offscreen: zipzi_core::Offscreen,
    },
    ReplaceOffscreen {
        offscreen: zipzi_core::Offscreen,
    },
    Scene {
        intent: crate::SceneEdit,
    },
    Organization {
        intent: crate::OrganizationEdit,
    },
    Binding {
        intent: crate::BindingAuthoringEdit,
    },
    Content {
        intent: crate::SceneContentEdit,
    },
    References {
        intent: crate::ReferenceEdit,
    },
    Resource {
        intent: crate::ResourceEdit,
    },
    Document {
        intent: crate::DocumentAuthoringEdit,
    },
    Attachment {
        intent: zipzi_core::document::AttachmentEdit,
    },
    ReplaceModelSetup {
        setup: zipzi_core::ModelSetup,
    },
    ReplaceHierarchyOrder {
        order: zipzi_core::document::HierarchyOrder,
    },
    ReplaceEditorState {
        state: zipzi_core::document::EditorState,
    },
    ReplaceDrawOrderGroups {
        groups: Vec<zipzi_core::draw_order::DrawOrderGroup>,
    },
    EraseObject {
        id: String,
    },
    RenameMesh {
        id: String,
        name: String,
    },
    UpdatePositions {
        id: String,
        vertex_ids: Vec<u32>,
        positions: Vec<zipzi_core::Vec2>,
    },
    SetMeshKeyform {
        id: String,
        keyform: zipzi_core::MeshKeyform,
    },
    SetSceneKeyform {
        id: String,
        keyform: zipzi_core::SceneKeyform,
    },
    ReplaceTopology {
        source: crate::GeometrySnapshot,
        replacement: crate::TopologyReplacement,
    },
}
impl ModelingCommand {
    pub fn apply(self, edit: &mut crate::EditSession<'_>) -> Result<(), crate::SdkError> {
        match self {
            Self::CreateAsset { asset } => edit.create_asset(asset),
            Self::ReplaceAsset { asset } => edit.replace_asset(asset),
            Self::CreateMesh { mesh } => edit.create_mesh(mesh),
            Self::ReplaceMesh { mesh } => edit.replace_mesh(mesh),
            Self::CreateParameter { parameter } => edit.create_parameter(parameter),
            Self::ReplaceParameter { parameter } => edit.replace_parameter(parameter),
            Self::CreatePart { part } => edit.create_part(part),
            Self::ReplacePart { part } => edit.replace_part(part),
            Self::CreateTransform { transform } => edit.create_transform(transform),
            Self::ReplaceTransform { transform } => edit.replace_transform(transform),
            Self::CreateBinding { binding } => edit.create_binding(binding),
            Self::ReplaceBinding { binding } => edit.replace_binding(binding),
            Self::CreateSceneBinding { binding } => edit.create_scene_binding(binding),
            Self::ReplaceSceneBinding { binding } => edit.replace_scene_binding(binding),
            Self::CreateBlendKeyTable { table } => edit.create_blend_key_table(table),
            Self::ReplaceBlendKeyTable { table } => edit.replace_blend_key_table(table),
            Self::CreateBlendConstraint { constraint } => edit.create_blend_constraint(constraint),
            Self::ReplaceBlendConstraint { constraint } => {
                edit.replace_blend_constraint(constraint)
            }
            Self::CreateBlendBinding { binding } => edit.create_blend_binding(binding),
            Self::ReplaceBlendBinding { binding } => edit.replace_blend_binding(binding),
            Self::CreateGlue { glue } => edit.create_glue(glue),
            Self::ReplaceGlue { glue } => edit.replace_glue(glue),
            Self::CreateOffscreen { offscreen } => edit.create_offscreen(offscreen),
            Self::ReplaceOffscreen { offscreen } => edit.replace_offscreen(offscreen),
            Self::Scene { intent } => edit.edit_scene(intent),
            Self::Organization { intent } => edit.edit_organization(intent),
            Self::Binding { intent } => edit.binding_authoring(intent),
            Self::Content { intent } => edit.edit_scene_content(intent),
            Self::References { intent } => edit.edit_references(intent),
            Self::Resource { intent } => edit.edit_resource(intent),
            Self::Document { intent } => edit.edit_document(intent),
            Self::Attachment { intent } => edit.edit_attachment(intent),
            Self::ReplaceModelSetup { setup } => edit.replace_model_setup(setup),
            Self::ReplaceHierarchyOrder { order } => edit.replace_hierarchy_order(order),
            Self::ReplaceEditorState { state } => edit.replace_editor_state(state),
            Self::ReplaceDrawOrderGroups { groups } => edit.replace_draw_order_groups(groups),
            Self::EraseObject { id } => edit.erase_object(&id),
            Self::RenameMesh { id, name } => edit.rename_mesh(&id, name),
            Self::UpdatePositions {
                id,
                vertex_ids,
                positions,
            } => edit.update_positions(&id, &vertex_ids, &positions),
            Self::SetMeshKeyform { id, keyform } => edit.set_mesh_keyform(&id, keyform),
            Self::SetSceneKeyform { id, keyform } => edit.set_scene_keyform(&id, keyform),
            Self::ReplaceTopology {
                source,
                replacement,
            } => edit.replace_topology(&source, replacement),
        }
    }
}
impl crate::EditSession<'_> {
    pub fn apply_modeling(
        &mut self,
        commands: impl IntoIterator<Item = ModelingCommand>,
    ) -> Result<(), crate::SdkError> {
        self.ensure_active("apply_modeling")?;
        for command in commands {
            command.apply(self)?;
        }
        Ok(())
    }
}

/// Select explicit record IDs; empty lists return collection identities only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
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
#[derive(Debug, Serialize)]
pub struct ModelingRecords<'a> {
    pub document_id: &'a str,
    pub setup: zipzi_core::ModelSetup,
    pub hierarchy_order: &'a zipzi_core::document::HierarchyOrder,
    pub editor_state: &'a zipzi_core::document::EditorState,
    pub display_info: &'a zipzi_core::document::DisplayInfo,
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
    ModelingRecords {
        document_id: document.id(),
        setup: document.model_setup(),
        hierarchy_order: document.hierarchy_order(),
        editor_state: document.editor_state(),
        display_info: document.display_info(),
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
#[derive(Debug, Serialize)]
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
        let diagnostics = self.diagnose_geometry(checks)?;
        let frames = parameters
            .iter()
            .map(|p| self.evaluate(p))
            .collect::<Result<_, _>>()?;
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
#[derive(Debug, Clone, Serialize)]
pub struct ModelingPlanReport {
    pub version: crate::Version,
    pub affected_ids: Vec<String>,
    pub copied_ids: std::collections::BTreeMap<String, String>,
    pub cost: Option<crate::ConversionCost>,
    pub losses: Vec<String>,
    pub no_op: Option<bool>,
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
