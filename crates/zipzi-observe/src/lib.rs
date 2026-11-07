//! Frozen snapshot capture and explicit-view GPU rendering for Runtime jobs.
//!
//! Evaluation uses detached authoring snapshots. Texture decoding and GPU work
//! never borrow the live editing session.

use std::collections::BTreeMap;
use std::sync::Arc;

use zipzi_animation::{MotionPreview, MotionSnapshot};
use zipzi_core::draw_order::DrawOrderGroup;
use zipzi_core::evaluation::evaluate_frame_including_hidden;
use zipzi_core::state::{AssetContent, AssetRecord, ContentId};
use zipzi_core::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, DrawableFrame, Glue, Mesh,
    MeshBinding, Offscreen, Parameter, Part, PreviewValues, SceneBinding, Transform,
};
use zipzi_project::{ResourceView, TextureData};
use zipzi_sdk::{QuerySource, SourcedSnapshot};

mod alpha;
mod digest;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "gpu")]
mod project_preview;
#[cfg(feature = "gpu")]
pub use project_preview::{render_project_preview, PROJECT_PREVIEW_WORKSPACE};
pub mod planning;
mod preparation;
#[cfg(feature = "gpu")]
mod quality;
#[cfg(feature = "gpu")]
mod topology;
mod view;
#[cfg(feature = "gpu")]
pub use gpu::{DrawableBounds, ObservedFrame, ObservedTexture, Observer, ObserverConfig};
pub use view::{ModelRoi, RenderRequest, ViewMapping};
pub use zipzi_contract::observation::{ObservationTextureReport, ObservationTextureReportEntry};

#[derive(Clone, Debug)]
pub struct ObservationInput {
    provenance: QuerySource,
    content_id: ContentId,
    document_id: String,
    requested: PreviewValues,
    frame: DrawableFrame,
    assets: Vec<AssetRecord>,
    resources: ResourceView,
    source: ObservationSource,
    authoring: CapturedAuthoring,
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
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
    assets: Vec<CapturedAsset>,
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
struct CapturedAsset {
    id: String,
    name: String,
    content: CapturedAssetContent,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CapturedAssetContent {
    Empty,
    Unresolved { source: String },
    Image { width: u32, height: u32 },
}
impl From<&AssetRecord> for CapturedAsset {
    fn from(asset: &AssetRecord) -> Self {
        let content = match &asset.content {
            AssetContent::Empty => CapturedAssetContent::Empty,
            AssetContent::Unresolved { source } => CapturedAssetContent::Unresolved {
                source: source.clone(),
            },
            AssetContent::Image { width, height, .. } => CapturedAssetContent::Image {
                width: *width,
                height: *height,
            },
        };
        Self {
            id: asset.id.clone(),
            name: asset.name.clone(),
            content,
        }
    }
}
fn image_descriptor(
    asset: &AssetRecord,
) -> Result<(zipzi_core::state::ResourceKey, u32, u32), ObservationError> {
    match asset.content {
        AssetContent::Image {
            resource,
            width,
            height,
        } => Ok((resource, width, height)),
        _ => Err(ObservationError {
            code: "MISSING_IMAGE_CONTENT".into(),
            message: "Observation requires captured image content".into(),
            asset_id: Some(asset.id.clone()),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
enum ObservationSource {
    Parameters,
    Animation { snapshot: Box<MotionSnapshot> },
}

#[derive(Clone, Debug)]
struct ResolvedTexture {
    asset: AssetRecord,
    data: TextureData,
    source_content_identity: planning::ObservationContentIdentity,
    render_rgba_hash: String,
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    source_alpha: Option<alpha::SourceAlphaSamples>,
}

/// Host limits applied before texture reads and allocations. The workspace cap
/// covers both single-image decode and direct downsampling/conversion phases.
#[derive(Clone, Copy, Debug)]
pub struct ObservationCaptureLimits {
    pub max_frames: usize,
    pub max_rgba_bytes: u64,
    pub png: zipzi_core::image::PngDecodeLimits,
}
impl Default for ObservationCaptureLimits {
    fn default() -> Self {
        Self {
            max_frames: 64,
            max_rgba_bytes: zipzi_contract::observation::DEFAULT_OBSERVATION_TEXTURE_RGBA_BYTES,
            png: Default::default(),
        }
    }
}

/// A captured evaluated frame and its decoded textures. Rendering this value
/// never revisits the source project or its assets.
#[derive(Clone, Debug)]
pub struct ResolvedObservation {
    input: ObservationInput,
    textures: Arc<[ResolvedTexture]>,
    capture_id: String,
    scene_digest: String,
    texture_report: Arc<ObservationTextureReport>,
}

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
    pub fn source(&self) -> QuerySource {
        self.input.provenance
    }

    pub fn content_id(&self) -> ContentId {
        self.input.content_id
    }

    /// Resolve 1..64 inputs from one snapshot, sharing decoded texture content.
    /// Cooperatively cancel between samples, texture decodes and scene digests.
    /// An already entered image decode completes before cancellation is observed.
    pub fn capture_many_with_control(
        inputs: Vec<ObservationInput>,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Vec<Self>, ObservationError> {
        let source_alpha = inputs.iter().any(|i| i.reference_frame.is_some());
        Self::capture_many(
            inputs,
            None,
            ObservationCaptureLimits::default(),
            source_alpha,
            control,
        )
    }

    /// Execute a host-created frozen plan, rejecting a mismatch rather than
    /// adding unplanned textures or silently adapting to changed resources.
    pub fn capture_many_with_plan(
        inputs: Vec<ObservationInput>,
        plan: &planning::ObservationTexturePlan,
        limits: ObservationCaptureLimits,
        source_alpha: bool,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Vec<Self>, ObservationError> {
        Self::capture_many(inputs, Some(plan), limits, source_alpha, control)
    }

    fn capture_many(
        inputs: Vec<ObservationInput>,
        supplied_plan: Option<&planning::ObservationTexturePlan>,
        limits: ObservationCaptureLimits,
        source_alpha: bool,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Vec<Self>, ObservationError> {
        check_capture(control)?;
        if inputs.is_empty() || inputs.len() > limits.max_frames {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: format!("Sample count must be 1..={}", limits.max_frames),
                asset_id: None,
            });
        }
        let first = &inputs[0];
        let mut assets = BTreeMap::<String, AssetRecord>::new();
        for input in &inputs {
            check_capture(control)?;
            if input.provenance != first.provenance
                || input.document_id != first.document_id
                || input.content_id != first.content_id
            {
                return Err(ObservationError {
                    code: "MIXED_DOCUMENT_SNAPSHOTS".into(),
                    message: "Samples must use one authoring snapshot".into(),
                    asset_id: None,
                });
            }
            for asset in &input.assets {
                check_capture(control)?;
                image_descriptor(asset)?;
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
        let assets: Vec<_> = assets.into_values().collect();
        let planned = planning::plan_textures(
            planning::sources_from_assets(&assets)?,
            supplied_plan.map_or_else(Default::default, |p| p.policy.clone()),
            limits.max_rgba_bytes,
        )?;
        let plan = match planned {
            planning::ObservationTexturePlanning::Ready { plan } => plan,
            planning::ObservationTexturePlanning::InsufficientBudget { minimum_required_rgba_bytes, .. } => return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: format!("Captured texture RGBA requires at least {minimum_required_rgba_bytes} bytes, host limit {}", limits.max_rgba_bytes),
                asset_id: None,
            }),
        };
        if supplied_plan.is_some_and(|supplied| *supplied != plan) {
            return Err(ObservationError {
                code: "TEXTURE_PLAN_MISMATCH".into(),
                message: "Frozen texture plan differs from captured source descriptors".into(),
                asset_id: None,
            });
        }
        // The equality check above validates the plan. No resource-driven
        // replanning takes place; actual dimensions/hashes are checked below.
        let plan = supplied_plan.unwrap_or(&plan);
        let textures: Vec<ResolvedTexture> = assets
            .into_iter()
            .zip(&plan.textures)
            .map(|(asset, target)| {
                check_capture(control)?;
                Self::check_preparation_workspace(target, limits, source_alpha, true)?;
                let (key, _, _) = image_descriptor(&asset)?;
                let image = first.resources.image(key).ok_or_else(|| ObservationError {
                    code: "MISSING_RESOURCE".into(),
                    message: "Captured resource view is incomplete".into(),
                    asset_id: Some(asset.id.clone()),
                })?;
                let verified = image
                    .verified_texture_with_control(|| !control.is_cancelled())
                    .map_err(|status| ObservationError {
                        code: status.code,
                        message: status.message,
                        asset_id: Some(asset.id.clone()),
                    })?;
                check_capture(control)?;
                Self::prepare_texture(asset, target, verified, source_alpha, control)
            })
            .collect::<Result<_, _>>()?;
        let texture_report = Arc::new(Self::report(plan, &textures));
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
                    texture_report: texture_report.clone(),
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

    pub fn texture_report(&self) -> &ObservationTextureReport {
        &self.texture_report
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
        snapshot: &SourcedSnapshot,
        requested: &PreviewValues,
    ) -> Result<Self, ObservationError> {
        Self::capture_from_snapshot_with_quality(snapshot, requested, false)
    }

    /// Capture an optional default-pose reference from this exact immutable snapshot.
    pub fn capture_from_snapshot_with_quality(
        snapshot: &SourcedSnapshot,
        requested: &PreviewValues,
        quality: bool,
    ) -> Result<Self, ObservationError> {
        let mut frame = DrawableFrame::default();
        let status =
            evaluate_frame_including_hidden(snapshot.snapshot.document(), requested, &mut frame);
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
        snapshot: &SourcedSnapshot,
        preview: &MotionPreview,
    ) -> Result<Self, ObservationError> {
        Self::capture_motion_from_snapshot_with_quality(snapshot, preview, false)
    }

    /// Capture animation and, optionally, its authoring source's default-pose reference.
    pub fn capture_motion_from_snapshot_with_quality(
        snapshot: &SourcedSnapshot,
        preview: &MotionPreview,
        quality: bool,
    ) -> Result<Self, ObservationError> {
        if preview.source_identity() != Some(snapshot.source)
            || !preview.matches_document(snapshot.snapshot.document())
        {
            return Err(ObservationError {
                code: "STALE_ANIMATION_PREVIEW".into(),
                message: "Animation preview belongs to another query source or content node".into(),
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
        snapshot: &SourcedSnapshot,
        requested: PreviewValues,
        frame: DrawableFrame,
        source: ObservationSource,
        quality: bool,
    ) -> Result<Self, ObservationError> {
        let provenance = snapshot.source;
        let document = snapshot.snapshot.document();
        let content_id = document.content_id();
        let assets = planning::referenced_assets(document)?;
        planning::validate_frame_references(&frame, &assets)?;
        let authoring = CapturedAuthoring {
            meshes: document
                .mesh_order()
                .iter()
                .map(|id| {
                    document
                        .mesh(id)
                        .cloned()
                        .expect("mesh ID exists in captured document")
                })
                .collect(),
            parts: document
                .part_order()
                .iter()
                .map(|id| {
                    document
                        .part(id)
                        .cloned()
                        .expect("part ID exists in captured document")
                })
                .collect(),
            transforms: document
                .transform_order()
                .iter()
                .map(|id| {
                    document
                        .transform(id)
                        .cloned()
                        .expect("transform ID exists in captured document")
                })
                .collect(),
            bindings: document
                .binding_order()
                .iter()
                .map(|id| {
                    document
                        .binding(id)
                        .cloned()
                        .expect("binding ID exists in captured document")
                })
                .collect(),
            parameters: document
                .parameter_order()
                .iter()
                .map(|id| document.parameter(id).cloned().expect("parameter exists"))
                .collect(),
            assets: document
                .asset_order()
                .iter()
                .map(|id| CapturedAsset::from(document.asset(id).expect("asset exists")))
                .collect(),
            draw_order_groups: document.draw_order_groups().clone().unwrap_or_default(),
            scene_bindings: document
                .scene_binding_order()
                .iter()
                .map(|id| {
                    document
                        .scene_binding(id)
                        .cloned()
                        .expect("scene binding exists")
                })
                .collect(),
            offscreens: document
                .offscreen_order()
                .iter()
                .map(|id| document.offscreen(id).cloned().expect("offscreen exists"))
                .collect(),
            glues: document
                .glue_order()
                .iter()
                .map(|id| document.glue(id).cloned().expect("glue exists"))
                .collect(),
            blend_key_tables: document
                .blend_key_table_order()
                .iter()
                .map(|id| {
                    document
                        .blend_key_table(id)
                        .cloned()
                        .expect("key table exists")
                })
                .collect(),
            blend_constraints: document
                .blend_constraint_order()
                .iter()
                .map(|id| {
                    document
                        .blend_constraint(id)
                        .cloned()
                        .expect("constraint exists")
                })
                .collect(),
            blend_bindings: document
                .blend_binding_order()
                .iter()
                .map(|id| {
                    document
                        .blend_binding(id)
                        .cloned()
                        .expect("blend binding exists")
                })
                .collect(),
        };
        Ok(Self {
            reference_frame: if quality {
                Some(
                    snapshot
                        .snapshot
                        .quality_reference()
                        .map_err(|e| ObservationError {
                            code: e.code.into(),
                            message: e.message.into(),
                            asset_id: None,
                        })?,
                )
            } else {
                None
            },
            resources: snapshot.snapshot.resources().clone(),
            provenance,
            content_id,
            document_id: document.id().to_owned(),
            requested,
            frame,
            assets,
            source,
            authoring,
        })
    }
}

#[cfg(test)]
mod source_tests;
