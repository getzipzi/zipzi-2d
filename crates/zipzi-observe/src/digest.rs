//! Content digests for Runtime observation results; no scene persistence.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zipzi_core::{DrawableFrame, ImageAsset};

use crate::{
    CapturedAuthoring, ObservationError, ObservationSource, RenderRequest, ResolvedObservation,
};

mod coordinates;

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneContent {
    document_id: String,
    requested: BTreeMap<String, f32>,
    source: ObservationSource,
    authoring: CapturedAuthoring,
    frame: DrawableFrame,
    textures: Vec<TextureContent>,
}

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureContent {
    asset: ImageAsset,
}

fn failure(code: &'static str, message: impl Into<String>) -> ObservationError {
    ObservationError {
        code: code.into(),
        message: message.into(),
        asset_id: None,
    }
}

fn canonical_json(value: &Value, output: &mut Vec<u8>) -> Result<(), ObservationError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(number) => {
            if number.is_f64() && number.as_f64() == Some(0.0) {
                output.extend_from_slice(b"0.0");
            } else {
                output.extend_from_slice(number.to_string().as_bytes());
            }
        }
        Value::String(value) => output.extend_from_slice(
            serde_json::to_string(value)
                .map_err(|e| failure("SCENE_ENCODE", e.to_string()))?
                .as_bytes(),
        ),
        Value::Array(items) => {
            output.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                canonical_json(item, output)?;
            }
            output.push(b']');
        }
        Value::Object(fields) => {
            output.push(b'{');
            let sorted: BTreeMap<_, _> = fields.iter().collect();
            for (index, (key, item)) in sorted.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                canonical_json(&Value::String(key.clone()), output)?;
                output.push(b':');
                canonical_json(item, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn scene_hash(scene: &SceneContent) -> Result<String, ObservationError> {
    let value = serde_json::to_value(scene).map_err(|e| failure("SCENE_ENCODE", e.to_string()))?;
    let mut decoded: SceneContent = serde_json::from_value(value.clone())
        .map_err(|e| failure("NONFINITE_SCENE", e.to_string()))?;
    decoded.authoring.restore_binding_coordinates()?;
    if decoded != *scene {
        return Err(failure(
            "NONFINITE_SCENE",
            "Scene has a nonfinite or non-roundtrippable value",
        ));
    }
    let mut bytes = b"zipzi-observe-scene-digest-v2\0".to_vec();
    canonical_json(&value, &mut bytes)?;
    Ok(zipzi_project::content_sha256(&bytes))
}

impl ResolvedObservation {
    fn scene_content(&self) -> SceneContent {
        let input = &self.input;
        SceneContent {
            document_id: input.document_id.clone(),
            requested: input
                .requested
                .iter()
                .map(|(id, value)| (id.clone(), *value))
                .collect(),
            source: input.source.clone(),
            authoring: input.authoring.clone(),
            frame: input.frame.clone(),
            textures: self
                .textures
                .iter()
                .map(|texture| TextureContent {
                    asset: {
                        let mut asset = texture.asset.clone();
                        asset.content.as_mut().unwrap().sha256 = texture.data.sha256.clone();
                        asset
                    },
                })
                .collect(),
        }
    }

    pub(crate) fn compute_scene_digest(&self) -> Result<String, ObservationError> {
        scene_hash(&self.scene_content())
    }

    /// Digest of the captured scene and explicit raw context render settings.
    pub fn render_digest(&self, request: RenderRequest) -> Result<String, ObservationError> {
        request.mapping()?;
        let mut bytes = b"zipzi-observe-render-digest-v1\0".to_vec();
        let mut request = serde_json::json!({
            "scene_digest": self.scene_digest,
            "width": request.width,
            "height": request.height,
            "roi": [request.roi.x0, request.roi.y0, request.roi.x1, request.roi.y1],
            "padding_model": request.padding_model,
            "topology": request.topology,
            "quality": request.quality,
            "mode": "context",
            "output": "raw_rgba8_renderer_default",
            "sample_policy": "single_evaluated_frame",
        });
        request["texture_sampling"] = "linear_mipmap_linear_repeat".into();
        request["mipmap_generation"] = "area_box_v2".into();
        canonical_json(&request, &mut bytes)?;
        Ok(zipzi_project::content_sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_scene() -> SceneContent {
        SceneContent {
            document_id: "document".into(),
            requested: BTreeMap::new(),
            source: ObservationSource::Parameters,
            authoring: CapturedAuthoring {
                meshes: vec![],
                parts: vec![],
                transforms: vec![],
                bindings: vec![],
                parameters: vec![],
                assets: vec![],
                draw_order_groups: vec![],
                scene_bindings: vec![],
                offscreens: vec![],
                glues: vec![],
                blend_key_tables: vec![],
                blend_constraints: vec![],
                blend_bindings: vec![],
            },
            frame: DrawableFrame::default(),
            textures: vec![],
        }
    }

    #[test]
    fn scene_hash_normalizes_negative_zero() {
        let mut first = empty_scene();
        first.requested.insert("parameter".into(), -0.0);
        let mut second = empty_scene();
        second.requested.insert("parameter".into(), 0.0);
        assert_eq!(scene_hash(&first).unwrap(), scene_hash(&second).unwrap());
    }

    #[test]
    fn scene_hash_accepts_evaluated_deformers() {
        let mut doc = zipzi_core::Document::new();
        assert!(doc
            .initialize(
                "00000000-0000-4000-8000-000000000001",
                zipzi_core::Canvas::new(100.0, 100.0, zipzi_core::Vec2::new(50.0, 50.0), 10.0),
            )
            .is_ok());
        assert!(doc
            .create_transform(zipzi_core::Transform {
                id: "00000000-0000-4000-8000-000000000002".into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let mut scene = empty_scene();
        assert!(zipzi_core::evaluate_frame(&doc, &Default::default(), &mut scene.frame).is_ok());
        assert_eq!(scene.frame.deformers.len(), 1);
        let digest = scene_hash(&scene).unwrap();
        let restored: SceneContent =
            serde_json::from_str(&serde_json::to_string(&scene).unwrap()).unwrap();
        assert_eq!(restored.frame, scene.frame);
        assert_eq!(scene_hash(&restored).unwrap(), digest);
    }

    #[test]
    fn scene_hash_rejects_nonfinite_values() {
        let mut scene = empty_scene();
        scene.requested.insert("parameter".into(), f32::NAN);
        assert_eq!(scene_hash(&scene).unwrap_err().code, "NONFINITE_SCENE");
    }

    #[test]
    fn stable_binding_samples_restore_all_omitted_coordinate_caches() {
        use zipzi_core::*;
        let mut scene = empty_scene();
        let axis = BindingAxis::new("parameter", vec![0., 1.]);
        let samples = vec![axis.samples[1].id.clone()];
        scene.authoring.bindings.push(MeshBinding {
            id: "binding".into(),
            mesh_id: "mesh".into(),
            axes: vec![axis.clone()],
            keyforms: vec![MeshKeyform {
                keys: vec![1.],
                samples: samples.clone(),
                ..Default::default()
            }],
        });
        for track in [
            SceneTrack::Warp {
                target_id: "warp".into(),
                keyforms: vec![WarpKeyform {
                    keys: vec![1.],
                    samples: samples.clone(),
                    ..Default::default()
                }],
            },
            SceneTrack::Rotation {
                target_id: "rotation".into(),
                keyforms: vec![RotationKeyform {
                    keys: vec![1.],
                    samples: samples.clone(),
                    ..Default::default()
                }],
            },
            SceneTrack::Part {
                target_id: "part".into(),
                keyforms: vec![PartKeyform {
                    keys: vec![1.],
                    samples: samples.clone(),
                    ..Default::default()
                }],
            },
        ] {
            scene.authoring.scene_bindings.push(SceneBinding {
                id: track.target_id().into(),
                axes: vec![axis.clone()],
                track,
            });
        }
        scene.authoring.glues.push(Glue {
            binding: Some(GlueBinding {
                axes: vec![axis.clone()],
                keyforms: vec![GlueKeyform {
                    samples,
                    intensity: 0.5,
                }],
            }),
            ..Default::default()
        });
        scene.authoring.blend_key_tables.push(BlendShapeKeyTable {
            id: "table".into(),
            parameter_id: "parameter".into(),
            samples: axis.samples.clone(),
            base_sample_id: axis.samples[1].id.clone(),
            keys: vec![0., 1.],
            base_key_idx: 1,
        });
        scene
            .authoring
            .blend_constraints
            .push(BlendShapeConstraint {
                id: "constraint".into(),
                parameter_id: "parameter".into(),
                samples: axis
                    .samples
                    .iter()
                    .map(|sample| ConstraintSample {
                        id: sample.id.clone(),
                        value: sample.value,
                        weight: 0.5,
                    })
                    .collect(),
                keys: vec![0., 1.],
                weights: vec![0.5; 2],
            });
        let digest = scene_hash(&scene).unwrap();
        let mut restored: SceneContent =
            serde_json::from_str(&serde_json::to_string(&scene).unwrap()).unwrap();
        restored.authoring.restore_binding_coordinates().unwrap();
        assert_eq!(scene.authoring, restored.authoring);
        assert_eq!(scene_hash(&restored).unwrap(), digest);
        restored.authoring.bindings[0].keyforms[0].samples[0] = "missing".into();
        assert_eq!(scene_hash(&restored).unwrap_err().code, "SCENE_FORMAT");
        scene.authoring.bindings[0].axes[0].keys[0] = f32::NAN;
        assert_eq!(scene_hash(&scene).unwrap_err().code, "NONFINITE_SCENE");
    }
}
