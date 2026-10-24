//! Parameter creation intent and validation, independent of UI input text.
use crate::{Document, ObjectKind, ObjectRef};
use anyhow::{Result, ensure};
use std::collections::HashSet;
use zipzi_core::{Parameter, ParameterKind, document::CdiParameterEntry};

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

    fn parameter_runtime_ids(&self, except: Option<&str>) -> HashSet<String> {
        let model = self.model();
        let mut ids: HashSet<_> = model
            .parameter_order()
            .iter()
            .filter(|id| Some(id.as_str()) != except)
            .filter_map(|id| model.get_parameter(id).map(|p| p.runtime_id.clone()))
            .collect();
        if let Some(entries) = &model.display_info().parameters {
            ids.extend(entries.iter().filter_map(|entry| match entry {
                CdiParameterEntry::Unresolved { runtime_id, .. } => Some(runtime_id.clone()),
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
        let model_read_guard_39 = self.model();
        let Some(groups) = &model_read_guard_39.display_info().parameter_groups else {
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
        let range = zipzi_core::document::validate_parameter_range(
            draft.minimum,
            draft.maximum,
            draft.default_value,
        )
        .err()
        .map(|e| match e {
            zipzi_core::document::ParameterRangeError::MinimumInvalid => E::MinimumInvalid,
            zipzi_core::document::ParameterRangeError::MaximumInvalid => E::MaximumInvalid,
            zipzi_core::document::ParameterRangeError::DefaultInvalid => E::DefaultInvalid,
            zipzi_core::document::ParameterRangeError::RangeInvalid => E::RangeInvalid,
            zipzi_core::document::ParameterRangeError::RangeOverflow => E::RangeOverflow,
            zipzi_core::document::ParameterRangeError::DefaultOutOfRange => E::DefaultOutOfRange,
        });
        let error = if draft.name.trim().is_empty() {
            Some(E::NameRequired)
        } else if draft.runtime_id.trim().is_empty() {
            Some(E::IdRequired)
        } else if self
            .parameter_runtime_ids(except)
            .contains(draft.runtime_id.trim())
        {
            Some(E::DuplicateId)
        } else if range.is_some() {
            range
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
        let plan =
            self.session
                .plan_organization_edit(zipzi_sdk::OrganizationEdit::CreateParameter {
                    parameter: draft.parameter(&id),
                    group_id: draft.group_id.clone(),
                    before: None,
                })?;
        self.execute_organization_authoring(&plan)?;
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
        let model_read_guard_40 = self.model();
        let p = model_read_guard_40
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
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("editor.validate_parameter_update");
        self.prepare_parameter_update(id, draft).map(|_| ())
    }

    pub(crate) fn prepare_parameter_update(
        &self,
        id: &str,
        draft: &ParameterDraft,
    ) -> Result<crate::runtime_host::HostedEdit> {
        let old = self.parameter_draft(id)?;
        self.ensure_editable(&[ObjectRef::new(ObjectKind::Parameter, id)])?;
        self.validate_parameter_fields(draft, Some(id))?;
        let parameter = draft.parameter(id);
        let mut edit = self.session.begin_owned_edit("Edit parameter", None)?;
        if let Err(error) = edit.replace_parameter(parameter.clone()) {
            let zipzi_runtime::RuntimeError::Sdk(error) = error else {
                return Err(error.into());
            };
            return Err(ParameterManagementError::Dependency {
                code: error.code.into(),
                message: error.message.into(),
                references: self.parameter_delete_references(id)?,
            }
            .into());
        }
        if old.group_id != draft.group_id {
            edit.edit_organization(zipzi_sdk::OrganizationEdit::Move {
                ids: vec![id.into()],
                destination: zipzi_sdk::OrganizationCollection::Parameters {
                    group_id: draft.group_id.clone(),
                },
                before: None,
            })?;
        }
        // The checked candidate is the one that will be published. Evaluate before
        // publication when the new range clamps a session preview input.
        let mut values = self.session.preview_values().clone();
        if let Some(value) = values.get_mut(id) {
            *value = value.clamp(parameter.minimum, parameter.maximum);
        }
        if &values != self.session.preview_values() {
            zipzi_core::preview::PreviewState::default()
                .replace(&edit.candidate_document(), 0, values)
                .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        }
        Ok(edit)
    }

    pub fn update_parameter(
        &mut self,
        id: &str,
        draft: ParameterDraft,
        expected: u64,
    ) -> Result<()> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("editor.update_parameter");
        self.check_parameter_version(expected)?;
        let edit = self.prepare_parameter_update(id, &draft)?;
        let old = self.parameter_draft(id)?;
        if self.model().get_parameter(id) == Some(&draft.parameter(id))
            && old.group_id == draft.group_id
        {
            return Ok(());
        }
        edit.commit_to(&mut self.session)?;
        self.parameter_management_changed()
    }

    /// Display membership is owned by the parameter. All other references block deletion.
    pub fn parameter_delete_references(&self, id: &str) -> Result<Vec<String>> {
        self.parameter_draft(id)?;
        Ok(self
            .model()
            .organization_delete_users(&[id.into()], zipzi_sdk::OrganizationDeletePolicy::Contents)
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?)
    }

    pub fn delete_parameter(&mut self, id: &str, expected: u64) -> Result<()> {
        let plan = self
            .plan_parameter_object_deletion(&ObjectRef::new(ObjectKind::Parameter, id), expected)?;
        self.execute_organization_authoring(&plan)?;
        Ok(())
    }

    pub(super) fn parameter_management_changed(&mut self) -> Result<()> {
        self.session.acknowledge_receipts();
        self.pristine_blank = None;
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
