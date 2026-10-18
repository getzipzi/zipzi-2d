//! Persistent Expression assets; playback state remains outside the document.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

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
    Resolved { parameter_id: String },
    Unresolved { runtime_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionEntry {
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
            .get("KasaneSourceScene")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    pub fn preserve_source_scene(&mut self, scene: &super::MotionClip) {
        self.extensions.insert(
            "KasaneSourceScene".into(),
            serde_json::to_value(scene).expect("validated source scene serializes"),
        );
    }

    pub fn runtime_extensions(&self) -> BTreeMap<String, Value> {
        self.extensions
            .iter()
            .filter(|(key, _)| key.as_str() != "KasaneSourceScene")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    pub fn has_extensions(&self) -> bool {
        self.extensions.keys().any(|key| key != "KasaneSourceScene")
            || self
                .entries
                .iter()
                .any(|entry| !entry.extensions.is_empty())
    }
}

impl Document {
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
        if self.contains_id(&expression.id) {
            return self.failed(Status::error("DUPLICATE_ID", &expression.id));
        }
        let status = self.validate_expression(&expression);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = expression.id.clone();
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
        let status = self.validate_expression(&expression);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = expression.id.clone();
        if self.content.expressions.get(&id) == Some(&expression) {
            return self.failed(Status::ok());
        }
        self.content.expressions.insert(id.clone(), expression);
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    fn validate_expression(&self, expression: &ExpressionAsset) -> Status {
        if !super::valid_uuid(&expression.id) {
            return Status::error("INVALID_EXPRESSION_ID", &expression.id);
        }
        if expression.extensions.contains_key("KasaneSourceScene") {
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
        for (index, entry) in expression.entries.iter().enumerate() {
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
