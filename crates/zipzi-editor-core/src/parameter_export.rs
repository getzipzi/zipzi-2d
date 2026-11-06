//! Read-only export preflight for parameter namespace edits.
use crate::{Document, EditorCommand};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterExportDiagnostic {
    pub resource: String,
    pub code: String,
    pub message: String,
    pub introduced: bool,
}

impl Document {
    pub fn parameter_export_diagnostics(
        &self,
        command: &EditorCommand,
    ) -> Vec<ParameterExportDiagnostic> {
        let mut candidate = self.model().builder();
        let intent = match command {
            EditorCommand::CreateParameter(draft)
            | EditorCommand::CreateParameterChecked { draft, .. } => Some(
                zipzi_contract::document::OrganizationEdit::CreateParameter {
                    parameter: draft.parameter(&uuid::Uuid::new_v4().to_string()),
                    group_id: draft.group_id.clone(),
                    before: None,
                },
            ),
            EditorCommand::DeleteParameter { id, .. } => {
                Some(zipzi_contract::document::OrganizationEdit::Delete {
                    ids: vec![id.clone()],
                    policy: zipzi_contract::document::OrganizationDeletePolicy::Contents,
                })
            }
            EditorCommand::CreateParameterGroup { draft, .. } => {
                self.group_intent(None, draft).ok()
            }
            EditorCommand::UpdateParameterGroup { id, draft, .. } => {
                self.group_intent(Some(id), draft).ok()
            }
            EditorCommand::DeleteParameterGroup { id, .. } => {
                Some(zipzi_contract::document::OrganizationEdit::Delete {
                    ids: vec![id.clone()],
                    policy: zipzi_contract::document::OrganizationDeletePolicy::Dissolve,
                })
            }
            EditorCommand::UpdateParameter { id, draft, .. } => {
                if !candidate.replace_parameter(draft.parameter(id)).is_ok() {
                    return Vec::new();
                }
                (candidate
                    .preview()
                    .ok()
                    .as_ref()
                    .and_then(|state| state.parameter_group_for(id).map(str::to_owned))
                    .as_deref()
                    != draft.group_id.as_deref())
                .then(|| zipzi_contract::document::OrganizationEdit::Move {
                    ids: vec![id.clone()],
                    destination: zipzi_contract::document::OrganizationCollection::Parameters {
                        group_id: draft.group_id.clone(),
                    },
                    before: None,
                })
            }
            _ => None,
        };
        if let Some(intent) = intent {
            if candidate.edit_organization(intent).is_err() {
                return Vec::new();
            }
        } else if !matches!(command, EditorCommand::UpdateParameter { .. }) {
            return Vec::new();
        }
        let Ok(candidate) = candidate.freeze() else {
            return Vec::new();
        };
        let before = export_diagnostics(&self.model());
        export_diagnostics(&candidate.state)
            .into_iter()
            .map(|(resource, code, message)| {
                let introduced =
                    !before.contains(&(resource.clone(), code.clone(), message.clone()));
                ParameterExportDiagnostic {
                    resource,
                    code,
                    message,
                    introduced,
                }
            })
            .collect()
    }
}

fn export_diagnostics(doc: &zipzi_core::state::DocumentState) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    macro_rules! inspect {
        ($resource:expr, $export:expr) => {
            if let Err(e) = $export {
                result.push(($resource.to_string(), e.code, e.message));
            }
        };
    }
    inspect!("CDI", zipzi_project::export_cdi3(doc));
    inspect!("Physics", zipzi_project::export_physics3(doc));
    inspect!("Model3", zipzi_project::model3::export_settings(doc));
    for id in doc.expression_order() {
        inspect!(
            format!("Expression: {}", doc.expression(id).unwrap().name),
            zipzi_project::export_expression3(doc, id)
        );
    }
    for id in doc.motion_order() {
        inspect!(
            format!("Motion: {}", doc.motion(id).unwrap().name),
            zipzi_project::export_motion3(doc, id)
        );
    }
    result
}
