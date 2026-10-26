use super::EditSession;
use crate::types::SdkError;
use zipzi_core::document::ExpressionAsset;
use zipzi_core::ChangeKind;
use zipzi_project::{import_expression3, ExpressionDiagnostic, ExpressionProjectError};

fn project_error(error: ExpressionProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn create_expression(&mut self, expression: ExpressionAsset) -> Result<(), SdkError> {
        let id = expression.id.clone();
        self.apply_authoring_change("create_expression", &id, true, |document| {
            document.create_expression(expression)
        })
    }

    pub fn replace_expression(&mut self, expression: ExpressionAsset) -> Result<(), SdkError> {
        self.ensure_active("replace_expression")?;
        let id = expression.id.clone();
        let removed: Vec<_> = self
            .candidate_document()
            .get_expression(&id)
            .into_iter()
            .flat_map(|e| &e.entries)
            .filter(|entry| !expression.entries.iter().any(|e| e.id == entry.id))
            .map(|e| e.id.clone())
            .collect();
        let result = self.document().replace_expression(expression);
        self.record(result, "replace_expression", &id)?;
        self.track_expression_deletions(&removed);
        Ok(())
    }

    pub fn import_expression3(
        &mut self,
        id: &str,
        name: &str,
        text: &str,
    ) -> Result<Vec<ExpressionDiagnostic>, SdkError> {
        self.ensure_active("import_expression3")?;
        let imported = import_expression3(self.candidate_document(), id, name, text)
            .map_err(|error| self.abort_with(project_error(error, "import_expression3")))?;
        let previous: Vec<_> = self
            .candidate_document()
            .get_expression(id)
            .into_iter()
            .flat_map(|e| &e.entries)
            .map(|e| e.id.clone())
            .collect();
        self.candidate_content_bytes.take();
        self.candidate = Some(imported.candidate);
        self.track_expression_deletions(&previous);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.iter().any(|item| item == id) {
            self.object_ids.push(id.into());
        }
        Ok(imported.diagnostics)
    }
}
