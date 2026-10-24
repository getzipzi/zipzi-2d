use crate::{Document, ObjectKind, ObjectRef, ParameterManagementError as E};
use anyhow::{Result, ensure};
use zipzi_core::document::CdiParameterGroup;
use zipzi_sdk::OrganizationEdit as Edit;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterGroupDraft {
    pub name: String,
    pub runtime_id: String,
    pub parent_id: Option<String>,
}
impl Document {
    pub fn parameter_group_draft(&self, id: &str) -> Result<ParameterGroupDraft> {
        let g = self
            .model()
            .get_parameter_group(id)
            .ok_or(E::MissingGroup)?;
        Ok(ParameterGroupDraft {
            name: g.name.clone(),
            runtime_id: g.runtime_id.clone(),
            parent_id: g.parent_id.clone(),
        })
    }
    pub fn suggested_parameter_group_id(&self) -> String {
        (1_u64..)
            .map(|n| {
                if n == 1 {
                    "GroupNew".into()
                } else {
                    format!("GroupNew{n}")
                }
            })
            .find(|id| {
                !self
                    .model()
                    .display_info()
                    .parameter_groups
                    .iter()
                    .flatten()
                    .any(|g| &g.runtime_id == id)
            })
            .unwrap()
    }
    pub(super) fn group_intent(
        &self,
        id: Option<&str>,
        draft: &ParameterGroupDraft,
    ) -> Result<Edit> {
        ensure!(!draft.name.trim().is_empty(), E::GroupNameRequired);
        ensure!(!draft.runtime_id.trim().is_empty(), E::GroupIdRequired);
        let old = id
            .map(|id| {
                self.model()
                    .get_parameter_group(id)
                    .cloned()
                    .ok_or(E::MissingGroup)
            })
            .transpose()?;
        let group = CdiParameterGroup {
            id: id
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            runtime_id: draft.runtime_id.trim().into(),
            name: draft.name.trim().into(),
            parent_id: draft.parent_id.clone(),
            extensions: old
                .as_ref()
                .map(|g| g.extensions.clone())
                .unwrap_or_default(),
        };
        Ok(if let Some(expected) = old {
            Edit::PatchGroup { expected, group }
        } else {
            Edit::CreateGroup {
                group,
                before: None,
            }
        })
    }
    pub fn validate_parameter_group(
        &self,
        id: Option<&str>,
        draft: &ParameterGroupDraft,
    ) -> Result<()> {
        let intent = self.group_intent(id, draft)?;
        self.session
            .plan_organization_edit(intent)
            .map_err(|error| match error.code.as_ref() {
                "DUPLICATE_CDI_RUNTIME_ID" => anyhow::Error::from(E::DuplicateGroupId),
                "CDI_GROUP_CYCLE" => anyhow::Error::from(E::GroupCycle),
                "MISSING_CDI_GROUP" => anyhow::Error::from(E::MissingParent),
                _ => anyhow::Error::from(error),
            })?;
        Ok(())
    }
    pub fn create_parameter_group(
        &mut self,
        draft: ParameterGroupDraft,
        expected: u64,
    ) -> Result<String> {
        self.check_parameter_version(expected)?;
        self.validate_parameter_group(None, &draft)?;
        let intent = self.group_intent(None, &draft)?;
        let Edit::CreateGroup { ref group, .. } = intent else {
            unreachable!()
        };
        let id = group.id.clone();
        let plan = self.session.plan_organization_edit(intent)?;
        self.execute_organization_authoring(&plan)?;
        self.navigate(ObjectRef::new(ObjectKind::ParameterGroup, &id))?;
        Ok(id)
    }
    pub fn update_parameter_group(
        &mut self,
        id: &str,
        draft: ParameterGroupDraft,
        expected: u64,
    ) -> Result<()> {
        self.check_parameter_version(expected)?;
        self.validate_parameter_group(Some(id), &draft)?;
        let plan = self
            .session
            .plan_organization_edit(self.group_intent(Some(id), &draft)?)?;
        self.execute_organization_authoring(&plan)?;
        Ok(())
    }
    pub fn parameter_group_delete_counts(&self, id: &str) -> Result<(usize, usize, usize)> {
        use zipzi_core::document::CdiParameterEntry;
        self.parameter_group_draft(id)?;
        let mut count = (0, 0, 0);
        for entry in self
            .model()
            .display_info()
            .parameters
            .iter()
            .flatten()
            .filter(|e| e.group_id() == Some(id))
        {
            match entry {
                CdiParameterEntry::Resolved { .. } => count.0 += 1,
                CdiParameterEntry::Unresolved { .. } => count.1 += 1,
            }
        }
        count.2 = self
            .model()
            .display_info()
            .parameter_groups
            .iter()
            .flatten()
            .filter(|g| g.parent_id.as_deref() == Some(id))
            .count();
        Ok(count)
    }
    pub fn delete_parameter_group(&mut self, id: &str, expected: u64) -> Result<()> {
        let plan = self.plan_parameter_object_deletion(
            &ObjectRef::new(ObjectKind::ParameterGroup, id),
            expected,
        )?;
        self.execute_organization_authoring(&plan)?;
        Ok(())
    }
}
