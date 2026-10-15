//! Read-only export preflight for parameter namespace edits.
use crate::{Document, EditorCommand};
use kasane_core::document::{CdiParameterEntry, CdiParameterGroup};

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
        let status = match command {
            EditorCommand::CreateParameter(draft)
            | EditorCommand::CreateParameterChecked { draft, .. }
            | EditorCommand::UpdateParameter { draft, .. } => {
                let id = match command {
                    EditorCommand::UpdateParameter { id, .. } => id.clone(),
                    _ => uuid::Uuid::new_v4().to_string(),
                };
                let p = draft.parameter(&id);
                if matches!(command, EditorCommand::UpdateParameter { .. }) {
                    candidate.replace_parameter(p).status
                } else {
                    candidate.create_parameter(p).status
                }
            }
            EditorCommand::DeleteParameter { id, .. } => {
                let mut info = candidate.display_info().clone();
                if let Some(entries) = &mut info.parameters {
                    entries.retain(|e| !matches!(e, CdiParameterEntry::Resolved { parameter_id, .. } if parameter_id == id));
                }
                if !candidate.replace_display_info(info).status.is_ok() {
                    return Vec::new();
                }
                candidate.erase_object(id).status
            }
            EditorCommand::CreateParameterGroup { draft, .. }
            | EditorCommand::UpdateParameterGroup { draft, .. } => {
                let mut info = candidate.display_info().clone();
                let groups = info.parameter_groups.get_or_insert_with(Vec::new);
                if let EditorCommand::UpdateParameterGroup { id, .. } = command {
                    let Some(g) = groups.iter_mut().find(|g| &g.id == id) else {
                        return Vec::new();
                    };
                    g.name = draft.name.trim().into();
                    g.runtime_id = draft.runtime_id.trim().into();
                    g.parent_id = draft.parent_id.clone();
                } else {
                    groups.push(CdiParameterGroup {
                        id: uuid::Uuid::new_v4().to_string(),
                        runtime_id: draft.runtime_id.trim().into(),
                        name: draft.name.trim().into(),
                        parent_id: draft.parent_id.clone(),
                        extensions: Default::default(),
                    });
                }
                candidate.replace_display_info(info).status
            }
            EditorCommand::DeleteParameterGroup { id, .. } => {
                let mut info = candidate.display_info().clone();
                let Some(groups) = &mut info.parameter_groups else {
                    return Vec::new();
                };
                let Some(parent) = groups
                    .iter()
                    .find(|g| &g.id == id)
                    .map(|g| g.parent_id.clone())
                else {
                    return Vec::new();
                };
                groups.retain(|g| &g.id != id);
                for group in groups {
                    if group.parent_id.as_ref() == Some(id) {
                        group.parent_id = parent.clone();
                    }
                }
                for entry in info.parameters.iter_mut().flatten() {
                    match entry {
                        CdiParameterEntry::Resolved { group_id, .. }
                        | CdiParameterEntry::Unresolved { group_id, .. } => {
                            if group_id.as_ref() == Some(id) {
                                *group_id = parent.clone();
                            }
                        }
                    }
                }
                candidate.replace_display_info(info).status
            }
            _ => return Vec::new(),
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

fn export_diagnostics(doc: &kasane_core::Document) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    macro_rules! inspect {
        ($resource:expr, $export:expr) => {
            if let Err(e) = $export {
                result.push(($resource.to_string(), e.code, e.message));
            }
        };
    }
    inspect!("CDI", kasane_project::export_cdi3(doc));
    inspect!("Physics", kasane_project::export_physics3(doc));
    inspect!("Model3", kasane_project::model3::export_settings(doc));
    for id in doc.expression_order() {
        inspect!(
            format!("Expression: {}", doc.get_expression(id).unwrap().name),
            kasane_project::export_expression3(doc, id)
        );
    }
    for id in doc.motion_order() {
        inspect!(
            format!("Motion: {}", doc.get_motion(id).unwrap().name),
            kasane_project::export_motion3(doc, id)
        );
    }
    result
}
