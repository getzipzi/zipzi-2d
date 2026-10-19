//! Parameter creation intent and validation, independent of UI input text.
use crate::{Document, ObjectKind, ObjectRef};
use anyhow::{Result, ensure};
use kasane_core::{Parameter, ParameterKind, document::CdiParameterEntry};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct NewParameter {
    pub name: String,
    pub runtime_id: String,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub decimal_places: i32,
    pub kind: ParameterKind,
    pub repeat: bool,
    pub group_id: Option<String>,
}

impl Default for NewParameter {
    fn default() -> Self {
        Self {
            name: "New Parameter".into(),
            runtime_id: "ParamNew".into(),
            minimum: -1.,
            maximum: 1.,
            default_value: 0.,
            decimal_places: 6,
            kind: ParameterKind::Normal,
            repeat: false,
            group_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterCreateError {
    NameRequired,
    IdRequired,
    DuplicateId,
    MinimumInvalid,
    MaximumInvalid,
    DefaultInvalid,
    RangeInvalid,
    RangeOverflow,
    DefaultOutOfRange,
    PrecisionInvalid,
    MissingGroup,
}

impl std::fmt::Display for ParameterCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NameRequired => "Enter a parameter name",
            Self::IdRequired => "Enter a parameter ID",
            Self::DuplicateId => "Parameter ID is already in use",
            Self::MinimumInvalid | Self::MaximumInvalid | Self::DefaultInvalid => {
                "Enter a finite number"
            }
            Self::RangeInvalid => "Minimum must be less than maximum",
            Self::RangeOverflow => "Parameter range is too large",
            Self::DefaultOutOfRange => "Default value must be within the parameter range",
            Self::PrecisionInvalid => "Precision must be an integer from 0 to 9",
            Self::MissingGroup => "The selected group no longer exists",
        })
    }
}
impl std::error::Error for ParameterCreateError {}

#[derive(Clone, Debug)]
pub struct ParameterGroupOption {
    pub id: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterGroupValue {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

impl Document {
    pub fn parameter_groups(&self) -> Vec<ParameterGroupValue> {
        self.model()
            .display_info()
            .parameter_groups
            .iter()
            .flatten()
            .map(|group| ParameterGroupValue {
                id: group.id.clone(),
                name: if group.name.is_empty() {
                    group.runtime_id.clone()
                } else {
                    group.name.clone()
                },
                parent_id: group.parent_id.clone(),
            })
            .collect()
    }

    fn parameter_runtime_ids(&self, except: Option<&str>) -> HashSet<&str> {
        let model = self.model();
        let mut ids: HashSet<_> = model
            .parameter_order()
            .iter()
            .filter(|id| Some(id.as_str()) != except)
            .filter_map(|id| model.get_parameter(id).map(|p| p.runtime_id.as_str()))
            .collect();
        if let Some(entries) = &model.display_info().parameters {
            ids.extend(entries.iter().filter_map(|entry| match entry {
                CdiParameterEntry::Unresolved { runtime_id, .. } => Some(runtime_id.as_str()),
                _ => None,
            }));
        }
        ids
    }

    pub fn suggested_parameter_id(&self) -> String {
        let ids = self.parameter_runtime_ids(None);
        if !ids.contains("ParamNew") {
            return "ParamNew".into();
        }
        (2_u64..)
            .map(|suffix| format!("ParamNew{suffix}"))
            .find(|id| !ids.contains(id.as_str()))
            .unwrap()
    }

    pub fn parameter_group_options(&self) -> Vec<ParameterGroupOption> {
        let Some(groups) = &self.model().display_info().parameter_groups else {
            return Vec::new();
        };
        let by_id: std::collections::HashMap<_, _> = groups
            .iter()
            .map(|group| (group.id.as_str(), group))
            .collect();
        groups
            .iter()
            .map(|group| {
                let mut names = Vec::new();
                let mut current = Some(group);
                let mut seen = HashSet::new();
                while let Some(group) = current {
                    if !seen.insert(&group.id) {
                        break;
                    }
                    names.push(if group.name.is_empty() {
                        group.runtime_id.as_str()
                    } else {
                        group.name.as_str()
                    });
                    current = group
                        .parent_id
                        .as_deref()
                        .and_then(|id| by_id.get(id).copied());
                }
                names.reverse();
                ParameterGroupOption {
                    id: group.id.clone(),
                    path: names.join(" / "),
                }
            })
            .collect()
    }

    pub fn validate_new_parameter(&self, draft: &NewParameter) -> Result<(), ParameterCreateError> {
        self.validate_parameter_fields(draft, None)
    }

    pub fn validate_parameter_fields(
        &self,
        draft: &NewParameter,
        except: Option<&str>,
    ) -> Result<(), ParameterCreateError> {
        use ParameterCreateError as E;
        let error = if draft.name.trim().is_empty() {
            Some(E::NameRequired)
        } else if draft.runtime_id.trim().is_empty() {
            Some(E::IdRequired)
        } else if self
            .parameter_runtime_ids(except)
            .contains(draft.runtime_id.trim())
        {
            Some(E::DuplicateId)
        } else if !draft.minimum.is_finite() {
            Some(E::MinimumInvalid)
        } else if !draft.maximum.is_finite() {
            Some(E::MaximumInvalid)
        } else if !draft.default_value.is_finite() {
            Some(E::DefaultInvalid)
        } else if draft.minimum >= draft.maximum {
            Some(E::RangeInvalid)
        } else if !(draft.maximum - draft.minimum).is_finite() {
            Some(E::RangeOverflow)
        } else if !(draft.minimum..=draft.maximum).contains(&draft.default_value) {
            Some(E::DefaultOutOfRange)
        } else if !(0..=9).contains(&draft.decimal_places) {
            Some(E::PrecisionInvalid)
        } else if draft.group_id.as_ref().is_some_and(|id| {
            !self
                .model()
                .display_info()
                .parameter_groups
                .as_ref()
                .is_some_and(|groups| groups.iter().any(|group| &group.id == id))
        }) {
            Some(E::MissingGroup)
        } else {
            None
        };
        error.map_or(Ok(()), Err)
    }

    pub fn create_parameter(&mut self, draft: NewParameter) -> anyhow::Result<String> {
        self.validate_new_parameter(&draft)?;
        self.finish_edit()?;
        let id = uuid::Uuid::new_v4().to_string();
        self.session.edit("New parameter", None, |edit| {
            edit.create_parameter(draft.parameter(&id))?;
            edit.set_parameter_group(&id, draft.group_id.as_deref())
        })?;
        self.session.drain_events();
        self.pristine_blank = false;
        self.invalidate_projection(false);
        let index = self.object_index();
        self.context.reconcile(&index);
        self.focus_parameter(&id)?;
        Ok(id)
    }
}

/// Shared typed draft for create and edit; the legacy name remains source compatible.
pub type ParameterDraft = NewParameter;
pub type ParameterValidationError = ParameterCreateError;

impl NewParameter {
    pub(crate) fn parameter(&self, id: &str) -> Parameter {
        Parameter {
            id: id.into(),
            name: self.name.trim().into(),
            runtime_id: self.runtime_id.trim().into(),
            minimum: self.minimum,
            maximum: self.maximum,
            default_value: self.default_value,
            decimal_places: self.decimal_places,
            kind: self.kind,
            repeat: self.repeat,
        }
    }
}

impl Document {
    pub fn parameter_draft(&self, id: &str) -> Result<ParameterDraft> {
        let p = self
            .model()
            .get_parameter(id)
            .ok_or_else(|| anyhow::Error::from(ParameterManagementError::MissingParameter))?;
        let group_id = self
            .model()
            .display_info()
            .parameters
            .iter()
            .flatten()
            .find_map(|e| match e {
                CdiParameterEntry::Resolved {
                    parameter_id,
                    group_id,
                    ..
                } if parameter_id == id => Some(group_id.clone()),
                _ => None,
            })
            .flatten();
        Ok(NewParameter {
            name: p.name.clone(),
            runtime_id: p.runtime_id.clone(),
            minimum: p.minimum,
            maximum: p.maximum,
            default_value: p.default_value,
            decimal_places: p.decimal_places,
            kind: p.kind,
            repeat: p.repeat,
            group_id,
        })
    }

    pub fn check_parameter_version(&self, expected: u64) -> Result<()> {
        ensure!(
            self.parameter_model_revision() == expected && !self.has_pending_edit(),
            ParameterManagementError::Stale
        );
        Ok(())
    }

    pub fn validate_parameter_update(&self, id: &str, draft: &ParameterDraft) -> Result<()> {
        self.parameter_draft(id)?;
        self.ensure_editable(&[ObjectRef::new(ObjectKind::Parameter, id)])?;
        self.validate_parameter_fields(draft, Some(id))?;
        let mut candidate = self.model().fork_candidate();
        let result = candidate.replace_parameter(draft.parameter(id));
        if !result.status.is_ok() {
            return Err(ParameterManagementError::Dependency {
                code: result.status.code,
                message: result.status.message,
                references: self.parameter_delete_references(id)?,
            }
            .into());
        }
        Ok(())
    }

    pub fn update_parameter(
        &mut self,
        id: &str,
        draft: ParameterDraft,
        expected: u64,
    ) -> Result<()> {
        self.check_parameter_version(expected)?;
        self.validate_parameter_update(id, &draft)?;
        let old = self.parameter_draft(id)?;
        let parameter = draft.parameter(id);
        if self.model().get_parameter(id) == Some(&parameter) && old.group_id == draft.group_id {
            return Ok(());
        }
        // Preview replacement evaluates the candidate. Check it before publication so
        // an evaluation error cannot report a failed edit after history was committed.
        let mut candidate = self.model().fork_candidate();
        let result = candidate.replace_parameter(parameter.clone());
        ensure!(result.status.is_ok(), "{}", result.status.message);
        let mut preview_values = self.session.preview_values().clone();
        if let Some(value) = preview_values.get_mut(id) {
            *value = value.clamp(parameter.minimum, parameter.maximum);
        }
        if &preview_values != self.session.preview_values() {
            let mut preview = kasane_core::preview::PreviewState::default();
            preview
                .replace(&candidate, 0, preview_values)
                .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        }
        let info = if old.group_id != draft.group_id {
            Some(self.parameter_move_info(
                id,
                crate::ParameterDestination::Group(draft.group_id.clone()),
            )?)
        } else {
            None
        };
        self.session.edit("Edit parameter", None, |edit| {
            edit.replace_parameter(parameter)?;
            if let Some(info) = info {
                edit.replace_display_info(info)?;
            }
            Ok(())
        })?;
        // Property submissions can settle an old input after navigation.
        self.parameter_management_changed()
    }

    /// Display membership is owned by the parameter. All other references block deletion.
    pub fn parameter_delete_references(&self, id: &str) -> Result<Vec<String>> {
        self.parameter_draft(id)?;
        let owned: HashSet<_> = self
            .model()
            .display_info()
            .parameters
            .iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, e)| {
                matches!(e, CdiParameterEntry::Resolved { parameter_id, .. } if parameter_id == id)
                    .then(|| format!("display_info.Parameters[{i}]"))
            })
            .collect();
        Ok(self
            .model()
            .references_to(id)
            .into_iter()
            .filter(|r| !owned.contains(r))
            .collect())
    }

    pub fn delete_parameter(&mut self, id: &str, expected: u64) -> Result<()> {
        self.check_parameter_version(expected)?;
        self.ensure_editable(&[ObjectRef::new(ObjectKind::Parameter, id)])?;
        let refs = self.parameter_delete_references(id)?;
        ensure!(refs.is_empty(), ParameterManagementError::Referenced(refs));
        let mut info = self.model().display_info().clone();
        if let Some(entries) = &mut info.parameters {
            entries.retain(|e| !matches!(e, CdiParameterEntry::Resolved { parameter_id, .. } if parameter_id == id));
        }
        self.session.edit("Delete parameter", None, |edit| {
            edit.replace_display_info(info)?;
            edit.erase_object(id)
        })?;
        self.parameter_management_changed()
    }

    pub(super) fn parameter_management_changed(&mut self) -> Result<()> {
        self.session.drain_events();
        self.pristine_blank = false;
        self.invalidate_projection(false);
        self.reconcile_parameter_preview()?;
        self.context.reconcile(&self.object_index());
        Ok(())
    }

    pub(super) fn reconcile_parameter_preview(&mut self) -> Result<()> {
        let mut values = self.session.preview_values().clone();
        values.retain(|id, value| {
            if let Some(p) = self.session.document().get_parameter(id) {
                *value = value.clamp(p.minimum, p.maximum);
                true
            } else {
                false
            }
        });
        if &values != self.session.preview_values() {
            self.session.set_preview_values(values)?;
            self.preview_revision += 1;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterManagementError {
    Stale,
    MissingParameter,
    MissingGroup,
    MissingParent,
    GroupNameRequired,
    GroupIdRequired,
    DuplicateGroupId,
    GroupCycle,
    Dependency {
        code: String,
        message: String,
        references: Vec<String>,
    },
    Referenced(Vec<String>),
}
impl std::fmt::Display for ParameterManagementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stale => f.write_str("Document changed; reload the form before saving"),
            Self::MissingParameter => f.write_str("Parameter no longer exists"),
            Self::MissingGroup => f.write_str("Group no longer exists"),
            Self::MissingParent => f.write_str("Parent group no longer exists"),
            Self::GroupNameRequired => f.write_str("Enter a group name"),
            Self::GroupIdRequired => f.write_str("Enter a group ID"),
            Self::DuplicateGroupId => f.write_str("Group ID is already in use"),
            Self::GroupCycle => {
                f.write_str("A group cannot be moved into itself or its descendants")
            }
            Self::Dependency { code, message, .. } => write!(f, "{code}: {message}"),
            Self::Referenced(refs) => write!(f, "Parameter is still used by: {}", refs.join(", ")),
        }
    }
}
impl std::error::Error for ParameterManagementError {}
