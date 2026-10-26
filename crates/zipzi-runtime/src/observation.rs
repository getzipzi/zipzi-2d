//! Finite transport projections; all evaluation, view mapping and rendering are SDK-owned.
use serde::{Deserialize, Serialize};
use zipzi_core::PreviewValues;
use zipzi_sdk_observe::{ObservationInput, Observer, ObserverConfig, ResolvedObservation};

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationView {
    /// Output width in pixels, 1..4096, subject to device and Runtime budgets.
    pub width: u32,
    /// Output height in pixels, 1..4096, subject to device and Runtime budgets.
    pub height: u32,
    /// Nonempty finite [min_x,min_y,max_x,max_y] in Y-up model coordinates.
    pub roi: [f32; 4],
    /// Nonnegative padding around the ROI in model units; aspect ratio is preserved.
    #[serde(default)]
    pub padding_model: f32,
}
impl ObservationView {
    pub fn request(self) -> zipzi_sdk_observe::RenderRequest {
        let [x0, y0, x1, y1] = self.roi;
        zipzi_sdk_observe::RenderRequest {
            width: self.width,
            height: self.height,
            roi: zipzi_sdk_observe::ModelRoi { x0, y0, x1, y1 },
            padding_model: self.padding_model,
        }
    }
    pub fn validate(self) -> crate::Result<()> {
        if self.width > 4096 || self.height > 4096 {
            return Err(crate::RuntimeError::Budget("observation dimensions"));
        }
        self.request().mapping().map_err(error)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservedTexture {
    /// Stable asset UUID resolved from the frozen authoring source.
    pub asset_id: String,
    /// SHA256 of the exact immutable texture content uploaded by this capture.
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservedBounds {
    /// Stable authoring drawable UUID.
    pub id: String,
    /// Evaluated visibility; geometric bounds do not prove unoccluded painted pixels.
    pub visible: bool,
    /// Clipped image bounds [min_x,min_y,max_x,max_y], exclusive maximum; None if empty.
    pub bounds: Option<[u32; 4]>,
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservationImage {
    /// Output image width in pixels.
    pub width: u32,
    /// Output image height in pixels.
    pub height: u32,
    /// Raw renderer RGBA8, row-major, top-left image origin. Renderer alpha convention is preserved.
    pub rgba: Vec<u8>,
    /// PNG encoding of exactly the returned RGBA8; encoded by Rust, never by Python.
    pub png: Vec<u8>,
    /// Captured authoring document UUID.
    pub document_id: String,
    /// Captured geometry evaluation revision, distinct from the outer formal/candidate Source.
    pub evaluation_revision: u64,
    /// Shared acquisition UUID for this batch's once-resolved immutable texture set.
    pub capture_id: String,
    /// Digest of captured evaluated geometry, authoring records and resolved texture content.
    pub scene_digest: String,
    /// Digest of the exact source scene and explicit rendering settings.
    pub render_digest: String,
    /// Renderer hash of evaluated input, texture content and output view.
    pub input_sha256: String,
    /// Actual GPU adapter name, obtained only when a render executes.
    pub adapter_name: String,
    /// Actual renderer backend, Metal on macOS or the selected WGPU backend elsewhere.
    pub adapter_backend: String,
    /// Effective evaluated parameters keyed by stable document UUID.
    pub parameters: PreviewValues,
    /// Actual viewport model extent [min_x,min_y,max_x,max_y], including aspect-ratio expansion.
    pub visible_roi: [f32; 4],
    /// Model-to-image scale. Image X=x*scale+offset[0], Y=-y*scale+offset[1].
    pub scale: f32,
    /// Translation in top-left image pixels, parallel to the documented scale formula.
    pub offset: [f32; 2],
    /// Source texture identities and exact content hashes shared across this batch.
    pub textures: Vec<ObservedTexture>,
    /// Drawable geometric image bounds in evaluated order; masks/occlusion are not pixel-tested here.
    pub drawable_bounds: Vec<ObservedBounds>,
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservationBatch {
    /// Owned explicit view shared by these ordered frames.
    pub view: ObservationView,
    /// Owned requested parameter maps in input order, including duplicates.
    pub requested: Vec<PreviewValues>,
    /// Rendered frames in requested pose order, sharing one resolved texture acquisition.
    pub frames: Vec<ObservationImage>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ObservationSampleBatch {
    pub source: crate::SourceRef,
    pub samples: ObservationBatch,
}

pub(crate) fn error(e: zipzi_sdk_observe::ObservationError) -> crate::RuntimeError {
    crate::RuntimeError::Sdk(zipzi_sdk::SdkError {
        code: e.code.into(),
        message: e.message.into(),
        operation: "observation",
        object_ids: e.asset_id.into_iter().collect(),
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    })
}
pub(crate) fn check(cancel: &std::sync::atomic::AtomicBool) -> crate::Result<()> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Err(crate::RuntimeError::Cancelled)
    } else {
        Ok(())
    }
}
pub(crate) fn observer(view: ObservationView) -> crate::Result<Observer> {
    view.validate()?;
    Observer::new(ObserverConfig {
        width: view.width,
        height: view.height,
        fit_long_side: 1.,
    })
    .map_err(error)
}
pub(crate) fn render(
    observer: &mut Observer,
    capture: &ResolvedObservation,
    view: ObservationView,
) -> crate::Result<ObservationImage> {
    let frame = observer.render(capture, view.request()).map_err(error)?;
    let mapping = frame.explicit_view.expect("explicit observation view");
    let roi = mapping.visible_roi;
    let png = frame.png_bytes().map_err(error)?;
    Ok(ObservationImage {
        width: frame.width,
        height: frame.height,
        rgba: frame.rgba,
        png,
        document_id: frame.document_id,
        evaluation_revision: frame.evaluation_revision,
        capture_id: capture.capture_id().into(),
        scene_digest: capture.scene_digest().into(),
        render_digest: capture.render_digest(view.request()).map_err(error)?,
        input_sha256: frame.input_sha256,
        adapter_name: frame.adapter_name,
        adapter_backend: frame.adapter_backend,
        parameters: frame
            .parameters
            .into_iter()
            .map(|(id, _, value, _)| (id, value))
            .collect(),
        visible_roi: [roi.x0, roi.y0, roi.x1, roi.y1],
        scale: mapping.scale,
        offset: [mapping.offset.0, mapping.offset.1],
        textures: frame
            .texture_revisions
            .into_iter()
            .map(|t| ObservedTexture {
                asset_id: t.asset_id,
                sha256: t.sha256,
            })
            .collect(),
        drawable_bounds: frame
            .drawable_bounds
            .into_iter()
            .map(|b| ObservedBounds {
                id: b.id,
                visible: b.visible,
                bounds: b.bounds.map(|(a, b, c, d)| [a, b, c, d]),
            })
            .collect(),
    })
}
pub(crate) fn sample(
    source: &crate::SourceSnapshot,
    requested: Vec<PreviewValues>,
    view: ObservationView,
    cancel: &std::sync::atomic::AtomicBool,
    control: &zipzi_sdk::ConversionControl,
) -> crate::Result<crate::JobValue> {
    let mut inputs = Vec::with_capacity(requested.len());
    for pose in &requested {
        check(cancel)?;
        inputs
            .push(ObservationInput::capture_from_snapshot(&source.snapshot, pose).map_err(error)?);
    }
    check(cancel)?;
    let captures =
        ResolvedObservation::capture_many_with_control(inputs, control).map_err(error)?;
    check(cancel)?;
    let mut observer = observer(view)?;
    let mut frames = Vec::with_capacity(captures.len());
    for capture in &captures {
        check(cancel)?;
        frames.push(render(&mut observer, capture, view)?);
    }
    check(cancel)?;
    Ok(crate::JobValue::Observed(std::sync::Arc::new(
        ObservationSampleBatch {
            source: source.source,
            samples: ObservationBatch {
                view,
                requested,
                frames,
            },
        },
    )))
}
