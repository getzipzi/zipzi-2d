//! Persistent Expression assets; playback state remains outside the document.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

mod authoring;
mod projection;
pub use authoring::*;
pub use projection::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionBlend {
    Add,
    Multiply,
    Overwrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpressionTarget {
    Unspecified,
    Resolved { parameter_id: String },
    Unresolved { runtime_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionEntry {
    pub id: String,
    pub target: ExpressionTarget,
    pub value: f32,
    /// Missing means Framework's default Add blend.
    pub blend: Option<ExpressionBlend>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionAsset {
    pub id: String,
    /// Registration name in model3; independent of parameter display names.
    pub name: String,
    pub file_type: Option<String>,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub entries: Vec<ExpressionEntry>,
    pub extensions: BTreeMap<String, Value>,
    /// Full Parameter UUID-to-runtime-ID namespace when opaque fields exist.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    /// Hash of imported content when opaque fields exist; known-field edits
    /// cannot silently change the meaning of those fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_source_content_hash: Option<String>,
}

impl ExpressionAsset {
    /// CAN3 scene layout and initial non-parameter state are authoring data,
    /// separate from the expression's runtime parameter overrides.
    pub fn source_scene(&self) -> Option<super::MotionClip> {
        self.extensions
            .get("ZipziSourceScene")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    pub fn preserve_source_scene(&mut self, scene: &super::MotionClip) {
        self.extensions.insert(
            "ZipziSourceScene".into(),
            serde_json::to_value(scene).expect("validated source scene serializes"),
        );
    }

    pub fn runtime_extensions(&self) -> BTreeMap<String, Value> {
        self.extensions
            .iter()
            .filter(|(key, _)| key.as_str() != "ZipziSourceScene")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    pub fn has_extensions(&self) -> bool {
        self.extensions.keys().any(|key| key != "ZipziSourceScene")
            || self
                .entries
                .iter()
                .any(|entry| !entry.extensions.is_empty())
    }
}

impl Document {
    fn contains_expression_identity(&self, id: &str) -> bool {
        self.contains_id(id)
            || self.content.motions.values().any(|m| {
                m.tracks
                    .iter()
                    .any(|t| t.id == id || t.keys().any(|k| k.id == id))
                    || m.events.iter().any(|e| e.id == id)
            })
    }
    pub fn expression_order(&self) -> &[String] {
        &self.content.expression_order
    }

    pub fn get_expression(&self, id: &str) -> Option<&ExpressionAsset> {
        self.content.expressions.get(id)
    }

    pub fn create_expression(&mut self, expression: ExpressionAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.contains_expression_identity(&expression.id) {
            return self.failed(Status::error("DUPLICATE_ID", &expression.id));
        }
        if let Err(status) = self.validate_expression_unlocked(&expression) {
            return self.failed(status);
        }
        let status = self.validate_expression_replacement(&expression);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = expression.id.clone();
        self.expression_entries.take();
        self.content.expressions.insert(id.clone(), expression);
        self.content.expression_order.push(id.clone());
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    pub fn replace_expression(&mut self, expression: ExpressionAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.content.expressions.contains_key(&expression.id) {
            return self.failed(Status::error("MISSING_EXPRESSION", &expression.id));
        }
        if let Err(status) = self.validate_expression_unlocked(&expression) {
            return self.failed(status);
        }
        let status = self.validate_expression_replacement(&expression);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = expression.id.clone();
        if self.content.expressions.get(&id) == Some(&expression) {
            return self.failed(Status::ok());
        }
        let retained: HashSet<_> = expression.entries.iter().map(|e| e.id.clone()).collect();
        if let Some(old) = self.get_expression(&id) {
            let deleted: Vec<_> = old
                .entries
                .iter()
                .filter(|e| !retained.contains(&e.id))
                .map(|e| e.id.clone())
                .collect();
            for id in deleted {
                self.content.editor_state.objects.remove(&id);
            }
        }
        self.expression_entries.take();
        self.content.expressions.insert(id.clone(), expression);
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    pub(super) fn validate_expression_unlocked(
        &self,
        replacement: &ExpressionAsset,
    ) -> Result<(), Status> {
        if let Some(old) = self.get_expression(&replacement.id) {
            if old != replacement && self.editor_state().get(&old.id).locked {
                return Err(Status::error("OBJECT_LOCKED", &old.id));
            }
            for (index, entry) in old.entries.iter().enumerate() {
                if self.editor_state().get(&entry.id).locked
                    && replacement.entries.get(index) != Some(entry)
                {
                    return Err(Status::error("OBJECT_LOCKED", &entry.id));
                }
            }
        }
        Ok(())
    }

    fn validate_expression_replacement(&self, expression: &ExpressionAsset) -> Status {
        let status = self.validate_expression(expression);
        if !status.is_ok() {
            return status;
        }
        for entry in &expression.entries {
            let own = self
                .get_expression_entry(&entry.id)
                .is_some_and(|(owner, _)| owner.id == expression.id);
            if self.contains_expression_identity(&entry.id) && !own {
                return Status::error("DUPLICATE_ID", &entry.id);
            }
        }
        Status::ok()
    }

    fn expression_entry_index(&self) -> &std::collections::HashMap<String, (String, usize)> {
        self.expression_entries.get_or_init(|| {
            self.content
                .expressions
                .values()
                .flat_map(|owner| {
                    owner
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(slot, entry)| (entry.id.clone(), (owner.id.clone(), slot)))
                })
                .collect()
        })
    }

    pub fn get_expression_entry(&self, id: &str) -> Option<(&ExpressionAsset, &ExpressionEntry)> {
        let (owner, slot) = self.expression_entry_index().get(id)?;
        let owner = self.content.expressions.get(owner)?;
        Some((owner, owner.entries.get(*slot)?))
    }

    fn validate_expression(&self, expression: &ExpressionAsset) -> Status {
        if !super::valid_uuid(&expression.id) {
            return Status::error("INVALID_EXPRESSION_ID", &expression.id);
        }
        if expression.extensions.contains_key("ZipziSourceScene") {
            let Some(scene) = expression.source_scene() else {
                return Status::error("INVALID_EXPRESSION_SOURCE_SCENE", &expression.id);
            };
            let status = self.validate_motion(&scene);
            if !status.is_ok() {
                return status;
            }
            if scene.id != expression.id
                || !scene.events.is_empty()
                || scene
                    .tracks
                    .iter()
                    .any(|track| track.initial.time != 0. || !track.segments.is_empty())
            {
                return Status::error("INVALID_EXPRESSION_SOURCE_SCENE", &expression.id);
            }
        }
        if expression.name.is_empty() || expression.name.contains('\0') {
            return Status::error("INVALID_EXPRESSION_NAME", &expression.id);
        }
        if self
            .content
            .expressions
            .values()
            .any(|other| other.id != expression.id && other.name == expression.name)
        {
            return Status::error("DUPLICATE_EXPRESSION_NAME", &expression.name);
        }
        if expression
            .file_type
            .as_ref()
            .is_some_and(|value| value.contains('\0'))
        {
            return Status::error("INVALID_EXPRESSION_TYPE", &expression.id);
        }
        for (path, fade) in [
            ("fade_in", expression.fade_in),
            ("fade_out", expression.fade_out),
        ] {
            if fade.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Status::error(
                    "INVALID_EXPRESSION_FADE",
                    format!("{}.{}", expression.id, path),
                );
            }
        }
        let mut ids = HashSet::new();
        for (index, entry) in expression.entries.iter().enumerate() {
            if !super::valid_uuid(&entry.id) || entry.id == expression.id || !ids.insert(&entry.id)
            {
                return Status::error("INVALID_EXPRESSION_ENTRY_ID", &entry.id);
            }
            if !entry.value.is_finite() {
                return Status::error(
                    "INVALID_EXPRESSION_VALUE",
                    format!("{}.entries[{index}]", expression.id),
                );
            }
            match &entry.target {
                ExpressionTarget::Resolved { parameter_id }
                    if self.get_parameter(parameter_id).is_none() =>
                {
                    return Status::error(
                        "MISSING_PARAMETER",
                        format!("{}.entries[{index}]", expression.id),
                    );
                }
                ExpressionTarget::Unresolved { runtime_id }
                    if runtime_id.is_empty() || runtime_id.contains('\0') =>
                {
                    return Status::error(
                        "INVALID_EXPRESSION_TARGET",
                        format!("{}.entries[{index}]", expression.id),
                    );
                }
                _ => {}
            }
        }
        Status::ok()
    }

    pub(super) fn validate_expressions(&self) -> Vec<StructureIssue> {
        let mut issues = Vec::new();
        let mut names = HashSet::new();
        for id in &self.content.expression_order {
            if let Some(expression) = self.content.expressions.get(id) {
                let status = self.validate_expression(expression);
                if !status.is_ok() {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status,
                    });
                }
                if !names.insert(expression.name.as_str()) {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status: Status::error("DUPLICATE_EXPRESSION_NAME", &expression.name),
                    });
                }
            }
        }
        issues
    }
}
