use super::EditSession;
use crate::types::SdkError;
use zipzi_core::document::PoseAsset;
use zipzi_core::ChangeKind;
use zipzi_project::{import_pose3, PoseDiagnostic, PoseProjectError};

fn project_error(error: PoseProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn set_pose(&mut self, pose: PoseAsset) -> Result<(), SdkError> {
        let id = pose.id.clone();
        self.apply_authoring_change("set_pose", &id, false, |document| document.set_pose(pose))
    }

    pub fn import_pose3(&mut self, id: &str, text: &str) -> Result<Vec<PoseDiagnostic>, SdkError> {
        self.ensure_active("import_pose3")?;
        let imported = import_pose3(self.candidate_document(), id, text)
            .map_err(|error| self.abort_with(project_error(error, "import_pose3")))?;
        let before = crate::session::keys_in(self.candidate_document());
        self.candidate_content_bytes.take();
        self.candidate = Some(imported.candidate);
        self.track_removed_objects(before);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.iter().any(|item| item == id) {
            self.object_ids.push(id.into());
        }
        Ok(imported.diagnostics)
    }
}
