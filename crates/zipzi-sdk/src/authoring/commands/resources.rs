use super::*;
use zipzi_core::{document::EditorState, SceneReference};
impl Commands<'_> {
    pub fn edit_attachment(&mut self, edit: AttachmentEdit) -> Result<(), Status> {
        self.run(|this| this.builder.edit_attachment(edit))
    }
    pub fn replace_editor_state(&mut self, state: EditorState) -> Result<(), Status> {
        self.run(|this| this.builder.replace_editor_state(state))
    }
    pub fn duplicate_asset(
        &mut self,
        source: &str,
        id: String,
        name: String,
    ) -> Result<(), Status> {
        self.run(|this| this.builder.duplicate_asset(source, id, name))
    }
    pub fn delete_asset(
        &mut self,
        id: &str,
        replacement: Option<SceneReference>,
    ) -> Result<(), Status> {
        self.run(|this| this.builder.delete_asset(id, replacement))
    }
    pub fn reorder_assets(&mut self, order: Vec<String>) -> Result<(), Status> {
        self.run(|this| this.builder.reorder_assets(order))
    }
    pub fn clear_image(&mut self, id: &str) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(id)?;
            let mut asset = this
                .builder
                .asset(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_ASSET", id))?;
            asset.content = AssetContent::Empty;
            this.builder.replace_asset(asset)
        })
    }
}

impl Commands<'_> {
    pub fn edit_document(
        &mut self,
        edit: zipzi_core::document::DocumentAuthoringEdit,
    ) -> Result<zipzi_core::state::DocumentOutcome, Status> {
        self.run(|this| this.builder.edit_document(edit))
    }
}

impl Commands<'_> {
    pub fn patch_property(
        &mut self,
        patch: zipzi_core::state::PropertyPatch,
    ) -> Result<(), Status> {
        self.run(|this| this.builder.patch_property(patch))
    }
}
