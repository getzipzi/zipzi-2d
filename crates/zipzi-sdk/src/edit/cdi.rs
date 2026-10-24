//! CDI authoring commands. Each command validates the complete candidate
//! before publication, so a failed edit cannot leave partial memberships.
use super::EditSession;
use crate::types::SdkError;
use zipzi_core::ChangeKind;
use zipzi_core::document::{CdiCombinedSet, CdiParameterGroup, DisplayInfo};
use zipzi_project::{CdiProjectDiagnostic, CdiProjectError, import_cdi3};

fn project_error(error: CdiProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn replace_display_info(&mut self, info: DisplayInfo) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_display_info", &id, false, |document| {
            document.replace_display_info(info)
        })
    }

    pub fn set_parameter_display_name(
        &mut self,
        parameter_id: &str,
        name: String,
    ) -> Result<(), SdkError> {
        self.apply_authoring_change(
            "set_parameter_display_name",
            parameter_id,
            true,
            |document| document.set_parameter_display_name(parameter_id, name),
        )
    }

    pub fn set_part_display_name(&mut self, part_id: &str, name: String) -> Result<(), SdkError> {
        self.apply_authoring_change("set_part_display_name", part_id, true, |document| {
            document.set_part_display_name(part_id, name)
        })
    }

    pub fn create_parameter_group(&mut self, group: CdiParameterGroup) -> Result<(), SdkError> {
        self.ensure_active("create_parameter_group")?;
        let mut info = self.candidate_document().display_info().clone();
        if info
            .parameter_groups
            .as_ref()
            .is_some_and(|groups| groups.iter().any(|item| item.id == group.id))
        {
            return Err(self.abort_with(SdkError::new(
                "DUPLICATE_ID",
                "CDI parameter group already exists",
                "create_parameter_group",
            )));
        }
        info.parameter_groups
            .get_or_insert_with(Vec::new)
            .push(group);
        self.replace_display_info(info)
    }

    pub fn replace_parameter_group(&mut self, group: CdiParameterGroup) -> Result<(), SdkError> {
        self.ensure_active("replace_parameter_group")?;
        let mut info = self.candidate_document().display_info().clone();
        let Some(existing) = info
            .parameter_groups
            .as_mut()
            .and_then(|groups| groups.iter_mut().find(|item| item.id == group.id))
        else {
            return Err(self.abort_with(SdkError::new(
                "MISSING_CDI_GROUP",
                "CDI parameter group does not exist",
                "replace_parameter_group",
            )));
        };
        *existing = group;
        self.replace_display_info(info)
    }

    /// Materializes generated entries before changing one membership, so
    /// other parameters do not disappear from the exported CDI.
    pub fn set_parameter_group(
        &mut self,
        parameter_id: &str,
        group_id: Option<&str>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_parameter_group")?;
        if self
            .candidate_document()
            .get_parameter(parameter_id)
            .is_none()
        {
            return Err(self.abort_with(SdkError::new(
                "MISSING_PARAMETER",
                parameter_id,
                "set_parameter_group",
            )));
        }
        self.edit_organization(zipzi_core::document::OrganizationEdit::SetParameterGroup {
            parameter_id: parameter_id.into(),
            group_id: group_id.map(str::to_owned),
        })
    }

    pub fn set_combined_parameters(&mut self, set: CdiCombinedSet) -> Result<(), SdkError> {
        self.ensure_active("set_combined_parameters")?;
        let mut info = self.candidate_document().display_info().clone();
        let sets = info.combined_parameters.get_or_insert_with(Vec::new);
        if let Some(existing) = sets.iter_mut().find(|item| item.id == set.id) {
            *existing = set;
        } else {
            sets.push(set);
        }
        self.replace_display_info(info)
    }

    /// Import CDI into the current edit workspace. The original session is
    /// untouched until the containing edit commits.
    pub fn import_cdi3(&mut self, text: &str) -> Result<Vec<CdiProjectDiagnostic>, SdkError> {
        self.ensure_active("import_cdi3")?;
        let imported = import_cdi3(self.candidate_document(), text)
            .map_err(|error| self.abort_with(project_error(error, "import_cdi3")))?;
        let id = imported.candidate.id().to_owned();
        let before = crate::session::keys_in(self.candidate_document());
        self.candidate_content_bytes.take();
        self.candidate = Some(imported.candidate);
        self.track_removed_objects(before);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.contains(&id) {
            self.object_ids.push(id);
        }
        Ok(imported.diagnostics)
    }
}
