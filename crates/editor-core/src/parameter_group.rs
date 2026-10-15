use crate::{Document, ObjectKind, ObjectRef, ParameterManagementError as E};
use anyhow::{Result, ensure};
use kasane_core::document::{CdiParameterEntry, CdiParameterGroup};

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
            .display_info()
            .parameter_groups
            .iter()
            .flatten()
            .find(|g| g.id == id)
            .ok_or_else(|| anyhow::Error::from(E::MissingGroup))?;
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

    pub fn validate_parameter_group(
        &self,
        id: Option<&str>,
        draft: &ParameterGroupDraft,
    ) -> Result<()> {
        if let Some(id) = id {
            self.parameter_group_draft(id)?;
            self.ensure_editable(&[ObjectRef::new(ObjectKind::ParameterGroup, id)])?;
        }
        ensure!(!draft.name.trim().is_empty(), E::GroupNameRequired);
        ensure!(!draft.runtime_id.trim().is_empty(), E::GroupIdRequired);
        let groups = self
            .model()
            .display_info()
            .parameter_groups
            .as_deref()
            .unwrap_or_default();
        ensure!(
            !groups
                .iter()
                .any(|g| Some(g.id.as_str()) != id && g.runtime_id == draft.runtime_id.trim()),
            E::DuplicateGroupId
        );
        let mut current = draft.parent_id.as_deref();
        let mut seen = std::collections::HashSet::new();
        while let Some(parent) = current {
            ensure!(Some(parent) != id && seen.insert(parent), E::GroupCycle);
            current = groups
                .iter()
                .find(|g| g.id == parent)
                .ok_or_else(|| anyhow::Error::from(E::MissingParent))?
                .parent_id
                .as_deref();
        }
        Ok(())
    }

    pub fn create_parameter_group(
        &mut self,
        draft: ParameterGroupDraft,
        expected: u64,
    ) -> Result<String> {
        self.check_parameter_version(expected)?;
        self.validate_parameter_group(None, &draft)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.session.edit("New parameter group", None, |edit| {
            edit.create_parameter_group(CdiParameterGroup {
                id: id.clone(),
                runtime_id: draft.runtime_id.trim().into(),
                name: draft.name.trim().into(),
                parent_id: draft.parent_id,
                extensions: Default::default(),
            })
        })?;
        self.parameter_management_changed()?;
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
        let mut info = self.model().display_info().clone();
        let groups = info.parameter_groups.as_mut().unwrap();
        let index = groups.iter().position(|g| g.id == id).unwrap();
        let mut group = groups[index].clone();
        let moved = group.parent_id != draft.parent_id;
        group.name = draft.name.trim().into();
        group.runtime_id = draft.runtime_id.trim().into();
        group.parent_id = draft.parent_id;
        if group == groups[index] {
            return Ok(());
        }
        if moved {
            groups.remove(index);
            groups.push(group);
        } else {
            groups[index] = group;
        }
        self.session.edit("Edit parameter group", None, |edit| {
            edit.replace_display_info(info)
        })?;
        self.parameter_management_changed()?;
        self.navigate(ObjectRef::new(ObjectKind::ParameterGroup, id))
    }

    /// Promote an existing group without revalidating unrelated legacy display fields.
    pub fn move_parameter_group_to_root(&mut self, id: &str) -> Result<()> {
        self.ensure_editable(&[ObjectRef::new(ObjectKind::ParameterGroup, id)])?;
        let mut info = self.model().display_info().clone();
        let groups = info.parameter_groups.as_mut().ok_or(E::MissingGroup)?;
        let position = groups
            .iter()
            .position(|g| g.id == id)
            .ok_or(E::MissingGroup)?;
        if groups[position].parent_id.is_none()
            && groups
                .iter()
                .rfind(|g| g.parent_id.is_none())
                .is_some_and(|g| g.id == id)
        {
            return Ok(());
        }
        let mut group = groups.remove(position);
        group.parent_id = None;
        groups.push(group);
        self.finish_edit()?;
        self.session
            .edit("Move parameter group to root", None, |edit| {
                edit.replace_display_info(info)
            })?;
        self.parameter_management_changed()
    }

    /// Direct resolved entries, unresolved entries and child groups that will be promoted.
    pub fn parameter_group_delete_counts(&self, id: &str) -> Result<(usize, usize, usize)> {
        self.parameter_group_draft(id)?;
        let mut counts = (0, 0, 0);
        for entry in self
            .model()
            .display_info()
            .parameters
            .iter()
            .flatten()
            .filter(|e| e.group_id() == Some(id))
        {
            match entry {
                CdiParameterEntry::Resolved { .. } => counts.0 += 1,
                CdiParameterEntry::Unresolved { .. } => counts.1 += 1,
            }
        }
        counts.2 = self
            .model()
            .display_info()
            .parameter_groups
            .iter()
            .flatten()
            .filter(|g| g.parent_id.as_deref() == Some(id))
            .count();
        Ok(counts)
    }

    pub fn delete_parameter_group(&mut self, id: &str, expected: u64) -> Result<()> {
        self.check_parameter_version(expected)?;
        self.ensure_editable(&[ObjectRef::new(ObjectKind::ParameterGroup, id)])?;
        let parent = self.parameter_group_draft(id)?.parent_id;
        let mut info = self.model().display_info().clone();
        if let Some(entries) = &mut info.parameters {
            let mut promoted = Vec::new();
            entries.retain(|e| {
                if e.group_id() == Some(id) {
                    promoted.push(e.clone());
                    false
                } else {
                    true
                }
            });
            for entry in &mut promoted {
                match entry {
                    CdiParameterEntry::Resolved { group_id, .. }
                    | CdiParameterEntry::Unresolved { group_id, .. } => *group_id = parent.clone(),
                }
            }
            let insertion = entries
                .iter()
                .rposition(|e| e.group_id() == parent.as_deref())
                .map_or(entries.len(), |i| i + 1);
            entries.splice(insertion..insertion, promoted);
        }
        if let Some(groups) = &mut info.parameter_groups {
            let mut promoted = Vec::new();
            groups.retain(|g| {
                if g.parent_id.as_deref() == Some(id) {
                    promoted.push(g.clone());
                    false
                } else {
                    true
                }
            });
            for g in &mut promoted {
                g.parent_id = parent.clone();
            }
            groups.extend(promoted);
        }
        // Keep the group until erase_object, so SDK object handles are invalidated correctly.
        self.session.edit("Delete parameter group", None, |edit| {
            edit.replace_display_info(info)?;
            edit.erase_object(id)
        })?;
        self.parameter_management_changed()
    }
}
