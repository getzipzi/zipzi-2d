//! Persistent CDI display metadata. Model objects own their display names.
use std::collections::BTreeMap;

use serde_json::Value;

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
        self.validate_display_info_value(&self.content.display_info, &[], &[])
    }
    pub(super) fn validate_display_info_value(
        &self,
        info: &DisplayInfo,
        parameters: &[crate::Parameter],
        parts: &[crate::Part],
    ) -> Vec<StructureIssue> {
        let parameter = |id: &str| {
            parameters
                .iter()
                .find(|p| p.id == id)
                .or_else(|| self.get_parameter(id))
        };
        let part = |id: &str| {
            parts
                .iter()
                .find(|p| p.id == id)
                .or_else(|| self.get_part(id))
        };
        let mut issues = Vec::new();
        let mut group_ids = std::collections::HashSet::new();
        let mut group_runtime_ids = std::collections::HashSet::new();
        let mut positions = std::collections::HashMap::new();
        if let Some(groups) = &info.parameter_groups {
            for (index, group) in groups.iter().enumerate() {
                let path = format!("$.ParameterGroups[{index}]");
                if !super::valid_uuid(&group.id) || !group_ids.insert(group.id.as_str()) {
                    issue(
                        &mut issues,
                        &group.id,
                        "INVALID_CDI_GROUP_ID",
                        format!("{path}.id is invalid or repeated"),
                    );
                }
                if group.runtime_id.is_empty()
                    || !group_runtime_ids.insert(group.runtime_id.as_str())
                {
                    issue(
                        &mut issues,
                        &group.id,
                        "DUPLICATE_CDI_RUNTIME_ID",
                        format!("{path}.runtime_id is empty or repeated"),
                    );
                }
                check_string(
                    &group.runtime_id,
                    &format!("{path}.runtime_id"),
                    &group.id,
                    &mut issues,
                );
                check_string(&group.name, &format!("{path}.name"), &group.id, &mut issues);
                check_extensions(
                    &group.extensions,
                    &format!("{path}.extensions"),
                    &group.id,
                    &mut issues,
                );
                positions.entry(group.id.as_str()).or_insert(index);
            }
            let mut completed = std::collections::HashSet::new();
            for index in 0..groups.len() {
                if completed.contains(&index) {
                    continue;
                }
                let mut path = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut current = Some(index);
                while let Some(i) = current {
                    if completed.contains(&i) {
                        break;
                    }
                    if !seen.insert(i) {
                        issue(
                            &mut issues,
                            &groups[i].id,
                            "CDI_GROUP_CYCLE",
                            format!("$.ParameterGroups[{i}].parent_id contains a cycle"),
                        );
                        break;
                    }
                    path.push(i);
                    current = match groups[i].parent_id.as_deref() {
                        Some(parent) => match positions.get(parent) {
                            Some(index) => Some(*index),
                            None => {
                                issue(
                                    &mut issues,
                                    &groups[i].id,
                                    "MISSING_CDI_GROUP",
                                    format!(
                                        "$.ParameterGroups[{i}].parent_id references absent group"
                                    ),
                                );
                                None
                            }
                        },
                        None => None,
                    };
                }
                completed.extend(path);
            }
        }
        let mut parameter_runtime_ids = std::collections::HashSet::new();
        if let Some(entries) = &info.parameters {
            for (index, entry) in entries.iter().enumerate() {
                let path = format!("$.Parameters[{index}]");
                if let Some(group_id) = entry.group_id() {
                    if !group_ids.contains(group_id) {
                        issue(
                            &mut issues,
                            group_id,
                            "MISSING_CDI_GROUP",
                            format!("{path}.group_id references absent group"),
                        );
                    }
                }
                let runtime_id = match entry {
                    CdiParameterEntry::Resolved { parameter_id, .. } => {
                        match parameter(parameter_id) {
                            Some(parameter) => parameter.runtime_id.as_str(),
                            None => {
                                issue(
                                    &mut issues,
                                    parameter_id,
                                    "MISSING_PARAMETER",
                                    format!("{path}.parameter_id references absent parameter"),
                                );
                                continue;
                            }
                        }
                    }
                    CdiParameterEntry::Unresolved {
                        runtime_id, name, ..
                    } => {
                        check_string(name, &format!("{path}.name"), runtime_id, &mut issues);
                        runtime_id
                    }
                };
                if runtime_id.is_empty() || !parameter_runtime_ids.insert(runtime_id) {
                    issue(
                        &mut issues,
                        runtime_id,
                        "DUPLICATE_CDI_RUNTIME_ID",
                        format!("{path} has empty or repeated runtime ID"),
                    );
                }
                check_string(
                    runtime_id,
                    &format!("{path}.runtime_id"),
                    runtime_id,
                    &mut issues,
                );
                check_extensions(
                    entry.extensions(),
                    &format!("{path}.extensions"),
                    runtime_id,
                    &mut issues,
                );
            }
        }
        let mut part_runtime_ids = std::collections::HashSet::new();
        if let Some(entries) = &info.parts {
            for (index, entry) in entries.iter().enumerate() {
                let path = format!("$.Parts[{index}]");
                let runtime_id = match entry {
                    CdiPartEntry::Resolved { part_id, .. } => match part(part_id) {
                        Some(part) => part.runtime_id.as_str(),
                        None => {
                            issue(
                                &mut issues,
                                part_id,
                                "MISSING_PART",
                                format!("{path}.part_id references absent part"),
                            );
                            continue;
                        }
                    },
                    CdiPartEntry::Unresolved {
                        runtime_id, name, ..
                    } => {
                        check_string(name, &format!("{path}.name"), runtime_id, &mut issues);
                        runtime_id
                    }
                };
                if runtime_id.is_empty() || !part_runtime_ids.insert(runtime_id) {
                    issue(
                        &mut issues,
                        runtime_id,
                        "DUPLICATE_CDI_RUNTIME_ID",
                        format!("{path} has empty or repeated runtime ID"),
                    );
                }
                check_string(
                    runtime_id,
                    &format!("{path}.runtime_id"),
                    runtime_id,
                    &mut issues,
                );
                check_extensions(
                    entry.extensions(),
                    &format!("{path}.extensions"),
                    runtime_id,
                    &mut issues,
                );
            }
        }
        let mut set_ids = std::collections::HashSet::new();
        if let Some(sets) = &info.combined_parameters {
            for (i, set) in sets.iter().enumerate() {
                if !super::valid_uuid(&set.id) || !set_ids.insert(set.id.as_str()) {
                    issue(
                        &mut issues,
                        &set.id,
                        "INVALID_CDI_SET_ID",
                        format!("$.CombinedParameters[{i}].id is invalid or repeated"),
                    );
                }
                let mut members = std::collections::HashSet::new();
                for (j, member) in set.members.iter().enumerate() {
                    let runtime_id = match &member.target {
                        CdiParameterRef::Unspecified => continue,
                        CdiParameterRef::Resolved { parameter_id } => {
                            match parameter(parameter_id) {
                                Some(parameter) => parameter.runtime_id.as_str(),
                                None => {
                                    issue(
                                        &mut issues,
                                        parameter_id,
                                        "MISSING_PARAMETER",
                                        format!(
                                            "$.CombinedParameters[{i}][{j}] references absent parameter"
                                        ),
                                    );
                                    continue;
                                }
                            }
                        }
                        CdiParameterRef::Unresolved { runtime_id } => runtime_id,
                    };
                    if runtime_id.is_empty() || !members.insert(runtime_id) {
                        issue(
                            &mut issues,
                            runtime_id,
                            "DUPLICATE_CDI_MEMBER",
                            format!("$.CombinedParameters[{i}][{j}] is empty or repeated"),
                        );
                    }
                    check_string(
                        runtime_id,
                        &format!("$.CombinedParameters[{i}][{j}]"),
                        runtime_id,
                        &mut issues,
                    );
                }
            }
        }
        check_extensions(&info.extensions, "$.extensions", self.id(), &mut issues);
        if let Some(snapshot) = &info.opaque_source_ids {
            for (domain, ids) in [
                ("parameters", &snapshot.parameters),
                ("parts", &snapshot.parts),
                ("groups", &snapshot.groups),
            ] {
                for (id, runtime_id) in ids {
                    check_string(
                        id,
                        &format!("$.opaque_source_ids.{domain}"),
                        id,
                        &mut issues,
                    );
                    check_string(
                        runtime_id,
                        &format!("$.opaque_source_ids.{domain}.{id}"),
                        id,
                        &mut issues,
                    );
                }
            }
        }
        issues
    }
}

fn issue(issues: &mut Vec<StructureIssue>, object_id: &str, code: &'static str, path: String) {
    issues.push(StructureIssue {
        object_id: object_id.into(),
        status: Status::error(code, path),
    });
}

fn check_string(value: &str, path: &str, object_id: &str, issues: &mut Vec<StructureIssue>) {
    if value.contains('\0') {
        issue(issues, object_id, "CDI_NUL_UNPERSISTABLE", path.into());
    }
}

fn check_extensions(
    extensions: &BTreeMap<String, Value>,
    path: &str,
    object_id: &str,
    issues: &mut Vec<StructureIssue>,
) {
    for (key, value) in extensions {
        let item_path = format!("{path}.{key}");
        check_string(key, &item_path, object_id, issues);
        check_extension_value(value, &item_path, object_id, issues);
    }
}

fn check_extension_value(
    value: &Value,
    path: &str,
    object_id: &str,
    issues: &mut Vec<StructureIssue>,
) {
    match value {
        Value::String(text) => check_string(text, path, object_id, issues),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                check_extension_value(item, &format!("{path}[{i}]"), object_id, issues);
            }
        }
        Value::Object(items) => {
            for (key, item) in items {
                let item_path = format!("{path}.{key}");
                check_string(key, &item_path, object_id, issues);
                check_extension_value(item, &item_path, object_id, issues);
            }
        }
        _ => {}
    }
}
