use super::{HostedDraft, HostedSession};
use zipzi_contract::authoring::CapabilityState;
use zipzi_core::document::*;
use zipzi_core::*;
use zipzi_sdk::*;
impl HostedSession {
    pub fn resource_store(&self) -> zipzi_project::ResourceStore {
        self.host.read().resources().clone()
    }

    pub fn asset(&self, id: &str) -> Option<zipzi_core::state::AssetRecord> {
        self.document().asset(id).cloned()
    }

    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<MeshBinding> {
        self.document().binding_for_mesh(mesh_id).cloned()
    }
    pub fn blend_binding(&self, id: &str) -> Option<BlendShapeBinding> {
        self.document().blend_binding(id).cloned()
    }
    pub fn blend_binding_ids(&self) -> Vec<String> {
        self.document().blend_binding_order().to_vec()
    }

    pub fn document_field_capability(&self, field: DocumentField) -> CapabilityState {
        match field {
            DocumentField::Layout | DocumentField::ExportSettings => CapabilityState::Available,
            _ => CapabilityState::Unsupported {
                code: "SOURCE_METADATA_READ_ONLY",
            },
        }
    }

    pub fn history_lengths(&self) -> (usize, usize) {
        let history = self.read().history_state();
        (history.undo_steps, history.redo_steps)
    }

    pub fn mesh(&self, id: &str) -> Option<Mesh> {
        self.document().mesh(id).cloned()
    }
    pub fn mesh_ids(&self) -> Vec<String> {
        self.document().mesh_order().to_vec()
    }
    pub fn modified(&self) -> bool {
        self.read().is_dirty()
    }

    pub fn parameter(&self, id: &str) -> Option<Parameter> {
        self.document().parameter(id).cloned()
    }
    pub fn parameter_ids(&self) -> Vec<String> {
        self.document().parameter_order().to_vec()
    }

    pub fn prepare_rgba_image(
        &self,
        width: u32,
        height: u32,
        rgba: std::sync::Arc<[u8]>,
    ) -> Result<zipzi_project::PreparedImageContent, Status> {
        zipzi_project::PreparedImageContent::from_rgba(
            width,
            height,
            rgba,
            self.host.read().resources().budget(),
        )
    }
    pub fn read_snapshot(&self) -> Snapshot {
        self.read().snapshot().clone()
    }

    pub fn references_to(&self, id: &str) -> Vec<String> {
        self.document().references_to(id)
    }
    pub fn reparent_support(&self, edit: &ReparentEdit) -> Result<ReparentSupport, Status> {
        self.document().reparent_support(edit)
    }

    pub fn transfer_snapshot(&self) -> Result<zipzi_project::TransferSnapshot, Status> {
        {
            let source = QuerySource::Published(self.version());
            self.host.read().transfer_snapshot(self.client, source)
        }
    }
    pub fn version(&self) -> Version {
        self.read().version()
    }

    pub fn remesh_rectangle_grid(
        &mut self,
        mesh_id: &str,
        columns: usize,
        rows: usize,
        expected: Option<Version>,
    ) -> zipzi_runtime::Result<Publication> {
        self.edit("Generate rectangle mesh", expected, |commands| {
            commands
                .remesh_rectangle_grid(
                    mesh_id,
                    columns,
                    rows,
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
        })
        .map(|(_, publication)| publication)
    }
}
impl HostedDraft {
    pub fn edit_motion(&mut self, intent: MotionEdit) -> zipzi_runtime::Result<()> {
        self.batch(|edit| edit.edit_motion(intent))
    }
    pub fn edit_organization(&mut self, intent: OrganizationEdit) -> zipzi_runtime::Result<()> {
        self.batch(|edit| edit.edit_organization(intent).map(|_| ()))
    }
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> zipzi_runtime::Result<()> {
        self.batch(|edit| edit.patch_physics(patch))
    }

    pub fn preview_property(
        &mut self,
        patch: &zipzi_core::state::PropertyPatch,
        preview: &mut zipzi_core::preview::PreviewState,
        values: zipzi_core::PreviewValues,
    ) -> zipzi_runtime::Result<()> {
        self.preview(|edit| edit.patch_property(patch.clone()))?;
        preview
            .replace(&self.candidate_document(), 0, values)
            .map(|_| ())
    }
    pub fn rename_mesh(&mut self, id: &str, name: impl Into<String>) -> zipzi_runtime::Result<()> {
        self.batch(|edit| edit.rename(zipzi_sdk::NameTarget::Mesh(id), name.into()))
    }
    pub fn replace_export_settings(
        &mut self,
        settings: zipzi_core::ExportSettings,
    ) -> zipzi_runtime::Result<()> {
        let expected = DocumentValue::ExportSettings(self.candidate_document().setup().export);
        self.batch(|edit| {
            edit.edit_document(DocumentAuthoringEdit::Patch {
                expected,
                value: DocumentValue::ExportSettings(settings),
            })
            .map(|_| ())
        })
    }
    pub fn replace_parameter(&mut self, parameter: Parameter) -> zipzi_runtime::Result<()> {
        self.batch(|edit| edit.replace_parameter(parameter))
    }
    pub fn set_mesh_opacity(&mut self, id: &str, opacity: f32) -> zipzi_runtime::Result<()> {
        self.batch(|edit| {
            let mut mesh = edit
                .read_document()?
                .mesh(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_MESH", id))?;
            mesh.appearance.opacity = opacity;
            edit.replace_mesh(mesh)
        })
    }
    pub fn set_physics(&mut self, asset: PhysicsAsset) -> zipzi_runtime::Result<()> {
        self.batch(|edit| edit.replace_physics(Some(asset)))
    }
}
