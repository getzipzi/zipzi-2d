//! Frozen snapshot capture and explicit-view GPU rendering for Runtime jobs.
//!
//! Evaluation uses detached authoring snapshots. Texture decoding and GPU work
//! never borrow the live editing session.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use zipzi_animation::{MotionPreview, MotionSnapshot};
use zipzi_core::draw_order::DrawOrderGroup;
use zipzi_core::evaluation::evaluate_frame_including_hidden;
use zipzi_core::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, DrawableFrame, Glue, ImageAsset,
    Mesh, MeshBinding, Offscreen, Parameter, Part, PreviewValues, SceneBinding, Transform,
};
use zipzi_project::{read_project_asset, AssetData};
use zipzi_sdk::{AuthoringSnapshot, Version};

mod digest;
mod gpu;
mod quality;
mod topology;
mod view;
pub use gpu::{DrawableBounds, ObservedFrame, ObservedTexture, Observer, ObserverConfig};
pub use view::{ModelRoi, RenderRequest, ViewMapping};

#[derive(Clone, Debug)]
pub struct ObservationInput {
    version: Version,
    evaluation_revision: u64,
    document_id: String,
    requested: PreviewValues,
    frame: DrawableFrame,
    assets: Vec<ImageAsset>,
    root: PathBuf,
    memory_images: BTreeMap<String, zipzi_project::PreparedImageContent>,
    source: ObservationSource,
    authoring: CapturedAuthoring,
    reference_frame: Option<DrawableFrame>,
}

/// Authoring content retained only for the scene digest and batch validation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct CapturedAuthoring {
    meshes: Vec<Mesh>,
    parts: Vec<Part>,
    transforms: Vec<Transform>,
    bindings: Vec<MeshBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parameters: Vec<Parameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    assets: Vec<ImageAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    draw_order_groups: Vec<DrawOrderGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    scene_bindings: Vec<SceneBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    offscreens: Vec<Offscreen>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    glues: Vec<Glue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blend_key_tables: Vec<BlendShapeKeyTable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blend_constraints: Vec<BlendShapeConstraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blend_bindings: Vec<BlendShapeBinding>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
enum ObservationSource {
    Parameters,
    Animation { snapshot: Box<MotionSnapshot> },
}

#[derive(Clone, Debug)]
struct ResolvedTexture {
    asset: ImageAsset,
    data: AssetData,
}

/// A captured evaluated frame and its decoded textures. Rendering this value
/// never revisits the source project or its assets.
#[derive(Clone, Debug)]
pub struct ResolvedObservation {
    input: ObservationInput,
    textures: Arc<[ResolvedTexture]>,
    capture_id: String,
    scene_digest: String,
}

pub(crate) const MAX_CAPTURE_TEXTURE_BYTES: u64 = 256 * 1024 * 1024;

fn check_capture(control: &zipzi_sdk::ConversionControl) -> Result<(), ObservationError> {
    if control.is_cancelled() {
        Err(ObservationError {
            code: "CANCELLED".into(),
            message: "Observation capture cancelled".into(),
            asset_id: None,
        })
    } else {
        Ok(())
    }
}

impl ResolvedObservation {
    /// Resolve 1..64 inputs from one snapshot, sharing decoded texture content.
    /// Cooperatively cancel between samples, texture decodes and scene digests.
    /// An already entered image decode completes before cancellation is observed.
    pub fn capture_many_with_control(
        inputs: Vec<ObservationInput>,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Vec<Self>, ObservationError> {
        check_capture(control)?;
        if inputs.is_empty() || inputs.len() > 64 {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Sample count must be 1..64".into(),
                asset_id: None,
            });
        }
        let first = &inputs[0];
        let mut assets = BTreeMap::<String, ImageAsset>::new();
        for input in &inputs {
            check_capture(control)?;
            if input.version != first.version
                || input.document_id != first.document_id
                || input.evaluation_revision != first.evaluation_revision
                || input.root != first.root
                || input.authoring != first.authoring
            {
                return Err(ObservationError {
                    code: "MIXED_DOCUMENT_SNAPSHOTS".into(),
                    message: "Samples must use one authoring snapshot".into(),
                    asset_id: None,
                });
            }
            for asset in &input.assets {
                check_capture(control)?;
                asset.image_content().map_err(|e| ObservationError {
                    code: e.code,
                    message: e.message,
                    asset_id: Some(asset.id.clone()),
                })?;
                if let Some(existing) = assets.insert(asset.id.clone(), asset.clone()) {
                    if existing != *asset {
                        return Err(ObservationError {
                            code: "MIXED_DOCUMENT_SNAPSHOTS".into(),
                            message: "Asset descriptor changed between samples".into(),
                            asset_id: Some(asset.id.clone()),
                        });
                    }
                }
            }
        }
        let expected_rgba = assets.values().try_fold(0u64, |total, asset| {
            total.checked_add(
                u64::from(asset.content.as_ref().unwrap().width)
                    .checked_mul(u64::from(asset.content.as_ref().unwrap().height))?
                    .checked_mul(4)?,
            )
        });
        if expected_rgba.is_none_or(|bytes| bytes > MAX_CAPTURE_TEXTURE_BYTES) {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Captured texture RGBA exceeds 256 MiB".into(),
                asset_id: None,
            });
        }
        let textures: Vec<ResolvedTexture> = assets
            .into_values()
            .map(|asset| {
                check_capture(control)?;
                let data = match first.memory_images.get(&asset.id) {
                    Some(image) => image.read_asset(),
                    None => read_project_asset(&first.root, &asset),
                }
                .map_err(|status| ObservationError {
                    code: status.code,
                    message: status.message,
                    asset_id: Some(asset.id.clone()),
                })?;
                check_capture(control)?;
                Ok(ResolvedTexture { asset, data })
            })
            .collect::<Result<_, _>>()?;
        let retained = textures.iter().try_fold(0u64, |total, texture| {
            total
                .checked_add(texture.data.bytes.len() as u64)?
                .checked_add(texture.data.rgba.len() as u64)
        });
        if retained.is_none_or(|bytes| bytes > MAX_CAPTURE_TEXTURE_BYTES) {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Captured texture bytes exceed 256 MiB".into(),
                asset_id: None,
            });
        }
        let shared: Arc<[ResolvedTexture]> = textures.into();
        let capture_id = uuid::Uuid::new_v4().to_string();
        inputs
            .into_iter()
            .map(|input| {
                check_capture(control)?;
                let mut capture = Self {
                    input,
                    textures: Arc::clone(&shared),
                    capture_id: capture_id.clone(),
                    scene_digest: String::new(),
                };
                capture.scene_digest = capture.compute_scene_digest()?;
                check_capture(control)?;
                Ok(capture)
            })
            .collect()
    }

    pub fn capture_id(&self) -> &str {
        &self.capture_id
    }

    pub fn scene_digest(&self) -> &str {
        &self.scene_digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationError {
    pub code: String,
    pub message: String,
    pub asset_id: Option<String>,
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ObservationError {}

impl ObservationInput {
    /// Evaluate a detached authoring snapshot; no live session is consulted.
    pub fn capture_from_snapshot(
        snapshot: &AuthoringSnapshot,
        requested: &PreviewValues,
    ) -> Result<Self, ObservationError> {
        Self::capture_from_snapshot_with_quality(snapshot, requested, false)
    }

    /// Capture an optional default-pose reference from this exact immutable snapshot.
    pub fn capture_from_snapshot_with_quality(
        snapshot: &AuthoringSnapshot,
        requested: &PreviewValues,
        quality: bool,
    ) -> Result<Self, ObservationError> {
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame_including_hidden(snapshot.document(), requested, &mut frame);
        if !status.is_ok() {
            return Err(ObservationError {
                code: status.code,
                message: status.message,
                asset_id: None,
            });
        }
        Self::capture_frame(
            snapshot,
            requested.clone(),
            frame,
            ObservationSource::Parameters,
            quality,
        )
    }

    /// Capture animation against the exact document snapshot checked for identity.
    /// Apply Model opacity to drawable draws before mask / Offscreen composition.
    pub fn capture_motion_from_snapshot(
        snapshot: &AuthoringSnapshot,
        preview: &MotionPreview,
    ) -> Result<Self, ObservationError> {
        Self::capture_motion_from_snapshot_with_quality(snapshot, preview, false)
    }

    /// Capture animation and, optionally, its authoring source's default-pose reference.
    pub fn capture_motion_from_snapshot_with_quality(
        snapshot: &AuthoringSnapshot,
        preview: &MotionPreview,
        quality: bool,
    ) -> Result<Self, ObservationError> {
        let version = snapshot.version();
        if preview.source_identity() != Some((version.session_id, version.generation))
            || !preview.matches_document(snapshot.document())
        {
            return Err(ObservationError {
                code: "STALE_ANIMATION_PREVIEW".into(),
                message: "Animation preview was captured from another document revision".into(),
                asset_id: None,
            });
        }
        let requested: PreviewValues = preview
            .snapshot()
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect();
        let mut frame = preview
            .evaluate_drawables_including_hidden()
            .map_err(|error| ObservationError {
                code: "ANIMATION_EVALUATION".into(),
                message: error.to_string(),
                asset_id: None,
            })?;
        zipzi_animation::apply_model_opacity(&mut frame, preview.snapshot().model_opacity)
            .map_err(|error| ObservationError {
                code: "INVALID_MODEL_OPACITY".into(),
                message: error.to_string(),
                asset_id: None,
            })?;
        Self::capture_frame(
            snapshot,
            requested,
            frame,
            ObservationSource::Animation {
                snapshot: Box::new(preview.snapshot().clone()),
            },
            quality,
        )
    }

    fn capture_frame(
        snapshot: &AuthoringSnapshot,
        requested: PreviewValues,
        frame: DrawableFrame,
        source: ObservationSource,
        quality: bool,
    ) -> Result<Self, ObservationError> {
        let version = snapshot.version();
        let document = snapshot.document();
        let evaluation_revision = document.evaluation_revision();
        let required: BTreeSet<_> = frame
            .drawables
            .iter()
            .map(|drawable| drawable.texture_asset_id.as_str())
            .collect();
        let mut assets = Vec::with_capacity(required.len());
        for id in required {
            let asset = document
                .get_asset(id)
                .cloned()
                .ok_or_else(|| ObservationError {
                    code: "MISSING_ASSET".into(),
                    message: "Drawable references an absent asset".into(),
                    asset_id: Some(id.to_owned()),
                })?;
            assets.push(asset);
        }
        let root = snapshot
            .project_path()
            .and_then(|path| path.parent())
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
        let authoring = CapturedAuthoring {
            meshes: document
                .mesh_order()
                .iter()
                .map(|id| {
                    document
                        .get_mesh(id)
                        .cloned()
                        .expect("mesh ID exists in captured document")
                })
                .collect(),
            parts: document
                .part_order()
                .iter()
                .map(|id| {
                    document
                        .get_part(id)
                        .cloned()
                        .expect("part ID exists in captured document")
                })
                .collect(),
            transforms: document
                .transform_order()
                .iter()
                .map(|id| {
                    document
                        .get_transform(id)
                        .cloned()
                        .expect("transform ID exists in captured document")
                })
                .collect(),
            bindings: document
                .binding_order()
                .iter()
                .map(|id| {
                    document
                        .get_binding(id)
                        .cloned()
                        .expect("binding ID exists in captured document")
                })
                .collect(),
            parameters: document
                .parameter_order()
                .iter()
                .map(|id| {
                    document
                        .get_parameter(id)
                        .cloned()
                        .expect("parameter exists")
                })
                .collect(),
            assets: document
                .asset_order()
                .iter()
                .map(|id| document.get_asset(id).cloned().expect("asset exists"))
                .collect(),
            draw_order_groups: document.draw_order_groups().unwrap_or_default().to_vec(),
            scene_bindings: document
                .scene_binding_order()
                .iter()
                .map(|id| {
                    document
                        .get_scene_binding(id)
                        .cloned()
                        .expect("scene binding exists")
                })
                .collect(),
            offscreens: document
                .offscreen_order()
                .iter()
                .map(|id| {
                    document
                        .get_offscreen(id)
                        .cloned()
                        .expect("offscreen exists")
                })
                .collect(),
            glues: document
                .glue_order()
                .iter()
                .map(|id| document.get_glue(id).cloned().expect("glue exists"))
                .collect(),
            blend_key_tables: document
                .blend_key_table_order()
                .iter()
                .map(|id| {
                    document
                        .get_blend_key_table(id)
                        .cloned()
                        .expect("key table exists")
                })
                .collect(),
            blend_constraints: document
                .blend_constraint_order()
                .iter()
                .map(|id| {
                    document
                        .get_blend_constraint(id)
                        .cloned()
                        .expect("constraint exists")
                })
                .collect(),
            blend_bindings: document
                .blend_binding_order()
                .iter()
                .map(|id| {
                    document
                        .get_blend_binding(id)
                        .cloned()
                        .expect("blend binding exists")
                })
                .collect(),
        };
        let memory_images = assets
            .iter()
            .filter_map(|a| {
                snapshot
                    .memory_image(&a.id)
                    .map(|image| (a.id.clone(), image.clone()))
            })
            .collect();
        Ok(Self {
            reference_frame: if quality {
                Some(snapshot.quality_reference().map_err(|e| ObservationError {
                    code: e.code.into(),
                    message: e.message.into(),
                    asset_id: None,
                })?)
            } else {
                None
            },
            memory_images,
            version,
            evaluation_revision,
            document_id: document.id().to_owned(),
            requested,
            frame,
            assets,
            root,
            source,
            authoring,
        })
    }
}
