//! Parameter creation intent and validation, independent of UI input text.
use crate::Document;
use kasane_core::{Parameter, ParameterKind, document::CdiParameterEntry};
use std::collections::HashSet;

#[derive(Clone, Debug)]
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

    fn parameter_runtime_ids(&self) -> HashSet<&str> {
        let model = self.model();
        let mut ids: HashSet<_> = model
            .parameter_order()
            .iter()
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
        let ids = self.parameter_runtime_ids();
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
        use ParameterCreateError as E;
        let error = if draft.name.trim().is_empty() {
            Some(E::NameRequired)
        } else if draft.runtime_id.trim().is_empty() {
            Some(E::IdRequired)
        } else if self
            .parameter_runtime_ids()
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
            edit.create_parameter(Parameter {
                id: id.clone(),
                runtime_id: draft.runtime_id.trim().into(),
                name: draft.name.trim().into(),
                minimum: draft.minimum,
                maximum: draft.maximum,
                default_value: draft.default_value,
                decimal_places: draft.decimal_places,
                kind: draft.kind,
                repeat: draft.repeat,
            })?;
            edit.set_parameter_group(&id, draft.group_id.as_deref())
        })?;
        self.session.drain_events();
        self.pristine_blank = false;
        self.projection_revision += 1;
        let index = self.object_index();
        self.context.reconcile(&index);
        self.focus_parameter(&id)?;
        Ok(id)
    }
}
