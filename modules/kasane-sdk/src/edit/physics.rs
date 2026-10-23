use super::EditSession;
use crate::types::SdkError;
use kasane_core::document::{PhysicsAsset, PhysicsPatch};
use kasane_core::ChangeKind;
use kasane_project::{import_physics3, PhysicsDiagnostic, PhysicsProjectError};

fn project_error(error: PhysicsProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn discard_missing_attachment(&mut self, path: &str) -> Result<(), SdkError> {
        self.ensure_active("discard_missing_attachment")?;
        let mut paths = self.candidate_document().missing_attachments().to_vec();
        if !paths.iter().any(|item| item == path) {
            return Err(self.abort_with(SdkError::new(
                "MISSING_ATTACHMENT",
                path,
                "discard_missing_attachment",
            )));
        }
        paths.retain(|item| item != path);
        let result = self.document().set_missing_attachments(paths);
        self.record(result, "discard_missing_attachment", path)
    }

    pub fn set_physics(&mut self, asset: PhysicsAsset) -> Result<(), SdkError> {
        let id = asset.id.clone();
        self.apply_authoring_change("set_physics", &id, false, |document| {
            document.set_physics(asset)
        })
    }

    /// Apply a typed field patch while sharing untouched rigs with history/snapshots.
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> Result<(), SdkError> {
        self.apply_authoring_change("patch_physics", &patch.asset_id, true, |document| {
            document.patch_physics(patch)
        })
    }

    pub fn import_physics3(
        &mut self,
        id: &str,
        text: &str,
    ) -> Result<Vec<PhysicsDiagnostic>, SdkError> {
        self.ensure_active("import_physics3")?;
        let imported = import_physics3(self.candidate_document(), id, text)
            .map_err(|error| self.abort_with(project_error(error, "import_physics3")))?;
        let before = crate::session::keys_in(self.candidate_document());
        self.candidate = Some(imported.candidate);
        self.track_removed_objects(before);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.iter().any(|item| item == id) {
            self.object_ids.push(id.into());
        }
        Ok(imported.diagnostics)
    }
}
