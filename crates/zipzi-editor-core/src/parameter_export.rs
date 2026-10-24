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
        let mut candidate = self.model().fork_candidate();
        let intent = match command {
            EditorCommand::CreateParameter(draft)
            | EditorCommand::CreateParameterChecked { draft, .. } => {
                Some(zipzi_sdk::OrganizationEdit::CreateParameter {
                    parameter: draft.parameter(&uuid::Uuid::new_v4().to_string()),
                    group_id: draft.group_id.clone(),
                    before: None,
                })
            }
            EditorCommand::DeleteParameter { id, .. } => {
                Some(zipzi_sdk::OrganizationEdit::Delete {
                    ids: vec![id.clone()],
                    policy: zipzi_sdk::OrganizationDeletePolicy::Contents,
                })
            }
            EditorCommand::CreateParameterGroup { draft, .. } => {
                self.group_intent(None, draft).ok()
            }
            EditorCommand::UpdateParameterGroup { id, draft, .. } => {
                self.group_intent(Some(id), draft).ok()
            }
            EditorCommand::DeleteParameterGroup { id, .. } => {
                Some(zipzi_sdk::OrganizationEdit::Delete {
                    ids: vec![id.clone()],
                    policy: zipzi_sdk::OrganizationDeletePolicy::Dissolve,
                })
            }
            EditorCommand::UpdateParameter { id, draft, .. } => {
                if !candidate
                    .replace_parameter(draft.parameter(id))
                    .status
                    .is_ok()
                {
                    return Vec::new();
                }
                (candidate.parameter_group_for(id) != draft.group_id.as_deref()).then(|| {
                    zipzi_sdk::OrganizationEdit::Move {
                        ids: vec![id.clone()],
                        destination: zipzi_sdk::OrganizationCollection::Parameters {
                            group_id: draft.group_id.clone(),
                        },
                        before: None,
                    }
                })
            }
            _ => None,
        };
        let status = if let Some(intent) = intent {
            match candidate.plan_organization_edit(intent) {
                Ok(plan) => candidate.apply_organization_plan(&plan).status,
                Err(status) => status,
            }
        } else if matches!(command, EditorCommand::UpdateParameter { .. }) {
            zipzi_core::Status::ok()
        } else {
            return Vec::new();
        };
        if !status.is_ok() {
            return Vec::new();
        }
        let before = export_diagnostics(self.model());
        export_diagnostics(&candidate)
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

fn export_diagnostics(doc: &zipzi_core::Document) -> Vec<(String, String, String)> {
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
            format!("Expression: {}", doc.get_expression(id).unwrap().name),
            zipzi_project::export_expression3(doc, id)
        );
    }
    for id in doc.motion_order() {
        inspect!(
            format!("Motion: {}", doc.get_motion(id).unwrap().name),
            zipzi_project::export_motion3(doc, id)
        );
    }
    result
}
