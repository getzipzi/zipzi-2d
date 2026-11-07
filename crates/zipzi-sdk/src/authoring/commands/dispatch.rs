use super::*;
use zipzi_contract::authoring::ObjectKind;
pub use zipzi_contract::kernel_commands::{Command, CommandOutcome, ImageInput};
impl Commands<'_> {
    /// Execute the shared typed command. Input/resource and domain failures are
    /// tracked by the same batch failure latch as direct Rust calls.
    pub fn execute(
        &mut self,
        command: Command,
        resources: &zipzi_project::ResourceStore,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<CommandOutcome, Status> {
        self.run(|this| {
            let created = match command {
                Command::CreateParameter { spec } => Some(this.create_parameter_spec(spec)?),
                Command::PutParameter { value } => {
                    this.replace_parameter(value)?;
                    None
                }
                Command::PatchParameter { id, patch } => {
                    this.patch_parameter(&id, patch)?;
                    None
                }
                Command::CreatePart { value } => {
                    this.create_part(value)?;
                    None
                }
                Command::CreateMesh { spec } => Some(this.create_mesh_spec(spec)?),
                Command::PutMesh { value } => {
                    this.replace_mesh(value)?;
                    None
                }
                Command::CreateAsset { id, name } => {
                    this.create_empty_asset(id, name)?;
                    None
                }
                Command::DuplicateAsset { source, id, name } => {
                    this.duplicate_asset(&source, id, name)?;
                    None
                }
                Command::DeleteAsset { id, replacement } => {
                    this.delete_asset(&id, replacement)?;
                    None
                }
                Command::ClearImage { id } => {
                    this.clear_image(&id)?;
                    None
                }
                Command::ReorderAssets { order } => {
                    this.reorder_assets(order)?;
                    None
                }
                Command::VertexPositions { mesh, updates } => {
                    this.set_vertex_positions(&mesh, &updates)?;
                    None
                }
                Command::CreateBinding { spec } => Some(this.create_mesh_binding_spec(spec)?),
                Command::PutBinding { value } => {
                    this.replace_binding(value)?;
                    None
                }
                Command::MeshKeyform { binding, value } => {
                    this.set_mesh_keyform_spec(&binding, value)?;
                    None
                }
                Command::CreateSceneBinding { value } => {
                    this.create_scene_binding(value)?;
                    None
                }
                Command::PutSceneBinding { value } => {
                    this.replace_scene_binding(value)?;
                    None
                }
                Command::SceneKeyform { binding, value } => {
                    this.set_scene_keyform(&binding, value)?;
                    None
                }
                Command::PutBlendBinding { value } => {
                    this.replace_blend_binding(value)?;
                    None
                }
                Command::ManualMesh { mesh, operation } => {
                    this.edit_manual_mesh(&mesh, operation, limits, control)?;
                    None
                }
                Command::Remesh { mesh, geometry } => {
                    this.remesh(&mesh, geometry, limits, control)?;
                    None
                }
                Command::RemeshTextureUvs {
                    mesh,
                    geometry,
                    texture_uvs,
                } => {
                    this.remesh_with_texture_uvs(&mesh, geometry, texture_uvs, limits, control)?;
                    None
                }
                Command::RectangleGrid {
                    mesh,
                    columns,
                    rows,
                } => {
                    this.remesh_rectangle_grid(&mesh, columns, rows, limits, control)?;
                    None
                }
                Command::DisplayInfo { value } => {
                    this.replace_display_info(value)?;
                    None
                }
                Command::GroupDeformer { create, sources } => {
                    this.group_deformer(create, sources, limits, control)?;
                    None
                }
                Command::PatchSceneContent {
                    owner,
                    target,
                    expected,
                    value,
                } => {
                    this.patch_scene_content(&owner, &target, expected, value, limits, control)?;
                    None
                }
                Command::Delete { request } => {
                    this.edit_deletion(&request, limits, control)?;
                    None
                }
                Command::ModelSettings { value } => {
                    this.replace_model3_settings(value)?;
                    None
                }
                Command::CreateMotion { value } => {
                    this.create_motion(value)?;
                    None
                }
                Command::PutMotion { value } => {
                    this.replace_motion(value)?;
                    None
                }
                Command::MotionCurve { clip, edit } => {
                    this.edit_motion_curve(&clip, edit)?;
                    None
                }
                Command::CreateExpression { value } => {
                    this.create_expression(value)?;
                    None
                }
                Command::PutExpression { value } => {
                    this.replace_expression(value)?;
                    None
                }
                Command::PutPose { value } => {
                    this.replace_pose(value)?;
                    None
                }
                Command::PutPhysics { value } => {
                    this.replace_physics(value)?;
                    None
                }
                Command::PatchPhysics { patch } => {
                    this.patch_physics(&patch)?;
                    None
                }
                Command::EditorState { value } => {
                    this.replace_editor_state(value)?;
                    None
                }
                Command::Property { patch } => {
                    this.patch_property(patch)?;
                    None
                }
                Command::BindingGrid { edit } => {
                    this.edit_binding_grid(edit, limits, control)?;
                    None
                }
                Command::BlendShape { edit } => {
                    this.edit_blend_shape(edit, limits, control)?;
                    None
                }
                Command::ParameterRange { edit } => {
                    this.edit_parameter_range(edit, limits, control)?;
                    None
                }
                Command::Scene { edit } => {
                    this.edit_scene(edit, limits, control)?;
                    None
                }
                Command::SceneContent { edit } => {
                    this.edit_scene_content(edit, limits, control)?;
                    None
                }
                Command::Organization { edit } => {
                    this.edit_organization(edit)?;
                    None
                }
                Command::Reference { edit } => {
                    this.edit_reference(edit)?;
                    None
                }
                Command::Motion { edit } => {
                    this.edit_motion(edit)?;
                    None
                }
                Command::Expression { edit } => {
                    this.edit_expression(edit)?;
                    None
                }
                Command::Pose { edit } => {
                    this.edit_pose(edit)?;
                    None
                }
                Command::Physics { edit } => {
                    this.edit_physics(edit)?;
                    None
                }
                Command::Attachment { edit } => {
                    this.edit_attachment(edit)?;
                    None
                }
                Command::Document { edit } => {
                    this.edit_document(edit)?;
                    None
                }
                Command::Rename { target, name } => {
                    let id = &target.id;
                    let target = match target.kind {
                        ObjectKind::Asset => NameTarget::Asset(id),
                        ObjectKind::Mesh => NameTarget::Mesh(id),
                        ObjectKind::Part => NameTarget::Part(id),
                        ObjectKind::Parameter => NameTarget::Parameter(id),
                        ObjectKind::Transform => NameTarget::Transform(id),
                        _ => {
                            return Err(error(
                                "INVALID_OBJECT_KIND",
                                "Use the domain command for this label",
                            ))
                        }
                    };
                    this.rename(target, name)?;
                    None
                }
                Command::ReplaceImage { id, input } => {
                    let image = match input {
                        ImageInput::Rgba {
                            width,
                            height,
                            bytes,
                        } => resources.prepare_rgba(width, height, bytes.into())?,
                        ImageInput::File { path } => resources.import_image(&path)?,
                    };
                    this.replace_image(&id, image)?;
                    None
                }
            };
            Ok(CommandOutcome { created })
        })
    }
}
