//! Preserve runtime expression operations alongside Animator's absolute poses.
//! optionParam is a native string map, so these entries survive Editor saves
//! without adding unknown XML fields or claiming native Multiply playback.
use crate::{Cmo3Error, Document};
use kasane_core::document::{ExpressionBlend, ExpressionEntry, ExpressionTarget};
use kasane_live2d::exp3::{encode_exp3, Expression3, Expression3Parameter};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const KEY: &str = "KASANE_EXPRESSION_ENTRY_V1";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    // Provenance only: copying a scene in Cubism assigns a new scene GUID.
    pub scene: String,
    pub parameter: String,
    pub blend: Option<ExpressionBlend>,
    pub value: f32,
    pub pose: f32,
    pub default: f32,
}

impl Entry {
    pub fn restore(
        &self,
        parameter: &str,
        default: f32,
        pose: f32,
    ) -> Result<ExpressionEntry, Cmo3Error> {
        if self.parameter != parameter
            || ![self.value, self.pose, self.default]
                .into_iter()
                .all(f32::is_finite)
        {
            return Err(Cmo3Error::InvalidDocument(
                "Invalid CAN3 expression operation identity or value".into(),
            ));
        }
        let unchanged = (pose - self.pose).abs() <= 0.00001 && default == self.default;
        let value = if unchanged {
            self.value
        } else {
            match self.blend.unwrap_or(ExpressionBlend::Add) {
                ExpressionBlend::Add => pose - default,
                ExpressionBlend::Overwrite => pose,
                ExpressionBlend::Multiply if default != 0. => pose / default,
                ExpressionBlend::Multiply if pose == 0. => self.value,
                ExpressionBlend::Multiply => return Err(Cmo3Error::InvalidDocument(
                    "An edited Multiply expression pose cannot be restored against a zero parameter default; explicitly change its blend operation".into()
                )),
            }
        };
        if !value.is_finite() {
            return Err(Cmo3Error::InvalidDocument(
                "Edited CAN3 expression operation exceeds finite values".into(),
            ));
        }
        Ok(ExpressionEntry {
            id: uuid::Uuid::new_v4().to_string(),
            target: ExpressionTarget::Resolved {
                parameter_id: parameter.into(),
            },
            value,
            blend: self.blend,
            extensions: Default::default(),
        })
    }
}

/// Native runtime sidecars, for Viewer/SDK playback with arbitrary motion bases.
pub(crate) fn files(doc: &Document) -> Result<BTreeMap<String, Vec<u8>>, Cmo3Error> {
    let mut files = BTreeMap::new();
    let mut manifest = Vec::new();
    for id in doc.expression_order() {
        let asset = doc.get_expression(id).unwrap();
        let projection = doc
            .expression_projection(id)
            .map_err(|s| Cmo3Error::InvalidDocument(format!("{}: {}", s.code, s.message)))?;
        let parameters = projection
            .entries
            .iter()
            .map(|entry| {
                let runtime_id = match &entry.target {
                    ExpressionTarget::Resolved { parameter_id } => {
                        &doc.get_parameter(parameter_id).unwrap().runtime_id
                    }
                    ExpressionTarget::Unresolved { runtime_id } => runtime_id,
                    ExpressionTarget::Unspecified => unreachable!(
                        "CAN3 diagnostics reject unconfigured entries before sidecar emission"
                    ),
                };
                Expression3Parameter {
                    id: runtime_id.clone(),
                    value: entry.value,
                    blend: entry.blend.map(|blend| match blend {
                        ExpressionBlend::Add => kasane_live2d::exp3::ExpressionBlend::Add,
                        ExpressionBlend::Multiply => kasane_live2d::exp3::ExpressionBlend::Multiply,
                        ExpressionBlend::Overwrite => {
                            kasane_live2d::exp3::ExpressionBlend::Overwrite
                        }
                    }),
                    extensions: entry.extensions.clone(),
                }
            })
            .collect();
        let wire = Expression3 {
            file_type: asset.file_type.clone(),
            fade_in: asset.fade_in,
            fade_out: asset.fade_out,
            parameters: Some(parameters),
            extensions: asset.runtime_extensions(),
        };
        let name = format!("expression-{id}.exp3.json");
        files.insert(
            name.clone(),
            encode_exp3(&wire)
                .map_err(|e| Cmo3Error::InvalidDocument(e.to_string()))?
                .into_bytes(),
        );
        manifest.push(serde_json::json!({"Id":id,"Name":asset.name,"File":name}));
    }
    if !manifest.is_empty() {
        files.insert(
            "expressions.json".into(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "Version":1,"Expressions":manifest,
            }))
            .map_err(|e| Cmo3Error::InvalidDocument(e.to_string()))?,
        );
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_operations_survive_clamped_poses_and_edits_recalculate_the_same_blend() {
        let mut entry = Entry {
            scene: "scene".into(),
            parameter: "parameter".into(),
            blend: Some(ExpressionBlend::Multiply),
            value: -5.,
            pose: -1.,
            default: 0.5,
        };
        assert_eq!(entry.restore("parameter", 0.5, -1.).unwrap().value, -5.);
        let edited = entry.restore("parameter", 0.5, 0.4).unwrap();
        assert_eq!(edited.blend, Some(ExpressionBlend::Multiply));
        assert_eq!(edited.value, 0.8);
        entry.default = 0.;
        entry.pose = 0.;
        assert_eq!(entry.restore("parameter", 0., 0.).unwrap().value, -5.);
        assert!(entry.restore("parameter", 0., 0.5).is_err());
        assert!(entry.restore("other-parameter", 0., 0.).is_err());
        entry.blend = Some(ExpressionBlend::Overwrite);
        assert_eq!(entry.restore("parameter", 0., 0.5).unwrap().value, 0.5);
        entry.blend = Some(ExpressionBlend::Add);
        assert_eq!(entry.restore("parameter", 0.2, 0.5).unwrap().value, 0.3);
    }
}
