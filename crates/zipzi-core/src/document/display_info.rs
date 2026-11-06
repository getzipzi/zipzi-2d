//! Persistent CDI display metadata. Model objects own their display names.

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

pub use zipzi_contract::document::{
    CdiCombinedSet, CdiNamespaceIds, CdiParameterEntry, CdiParameterGroup, CdiParameterRef,
    CdiPartEntry, DisplayInfo, DisplayInfoOrigin,
};

impl Document {
    pub fn display_info(&self) -> &DisplayInfo {
        &self.content.display_info
    }

    pub fn replace_display_info(&mut self, display_info: DisplayInfo) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.content.display_info == display_info {
            return self.failed(Status::ok());
        }
        if let Err(status) = self.validate_organization_metadata_unlocked(&display_info) {
            return self.failed(status);
        }
        if let Err(status) =
            self.validate_reference_replacement_unlocked(self.model3_settings(), &display_info)
        {
            return self.failed(status);
        }
        let previous_references = self
            .reference_identities()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        self.reference_index.take();
        self.reference_projection.take();
        let previous = std::mem::replace(&mut self.content.display_info, display_info);
        let old_editor_state = self.content.editor_state.clone();
        for id in previous.parameter_groups.iter().flatten().map(|g| &g.id) {
            if self
                .content
                .display_info
                .parameter_groups
                .iter()
                .flatten()
                .all(|g| g.id != *id)
            {
                self.content.editor_state.objects.remove(id);
            }
        }
        self.clean_retired_reference_flags(previous_references);
        if let Some(issue) = self.validate_structure().into_iter().next() {
            self.content.display_info = previous;
            self.reference_index.take();
            self.reference_projection.take();
            self.content.editor_state = old_editor_state;
            return self.failed(issue.status);
        }
        if !super::organization::same_metadata_shape(&previous, &self.content.display_info) {
            self.organization_index.take();
        }
        self.changed(
            ChangeKind::Metadata,
            Vec::new(),
            vec![self.content.id.clone()],
        )
    }

    pub fn set_parameter_display_name(&mut self, id: &str, name: String) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if name.contains('\0') {
            return self.failed(Status::error(
                "INVALID_NAME",
                format!("$.Parameters[{id}].Name: embedded NUL"),
            ));
        }
        let Some(parameter) = self.content.parameters.get_mut(id) else {
            return self.failed(Status::error("MISSING_PARAMETER", id));
        };
        if parameter.name == name {
            return self.failed(Status::ok());
        }
        parameter.name = name;
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id.into()])
    }

    pub fn set_part_display_name(&mut self, id: &str, name: String) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if name.contains('\0') {
            return self.failed(Status::error(
                "INVALID_NAME",
                format!("$.Parts[{id}].Name: embedded NUL"),
            ));
        }
        let Some(part) = self.content.parts.get_mut(id) else {
            return self.failed(Status::error("MISSING_PART", id));
        };
        if part.name == name {
            return self.failed(Status::ok());
        }
        part.name = name;
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id.into()])
    }

    pub(super) fn validate_display_info(&self) -> Vec<StructureIssue> {
        self.validation_model().validate_display_info()
    }
    pub(super) fn validate_display_info_value(
        &self,
        info: &DisplayInfo,
        parameters: &[crate::Parameter],
        parts: &[crate::Part],
    ) -> Vec<StructureIssue> {
        self.validation_model()
            .validate_display_info_value(info, parameters, parts)
    }
}
