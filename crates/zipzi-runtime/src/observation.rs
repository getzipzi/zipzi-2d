//! Finite transport projections; all evaluation, view mapping and rendering are SDK-owned.
use serde::Serialize;
#[cfg(feature = "gpu-observation")]
use zipzi_observe::{Observer, ObserverConfig, ResolvedObservation};
use zipzi_sdk::TopologyOverlayValidation;

pub use zipzi_contract::runtime::ObservationView;
pub(crate) trait ObservationViewRuntime {
    fn request(&self) -> zipzi_observe::RenderRequest;
    fn validate(&self) -> crate::Result<()>;
}
impl ObservationViewRuntime for ObservationView {
    fn request(&self) -> zipzi_observe::RenderRequest {
        let [x0, y0, x1, y1] = self.roi;
        zipzi_observe::RenderRequest {
            width: self.width,
            height: self.height,
            roi: zipzi_observe::ModelRoi { x0, y0, x1, y1 },
            padding_model: self.padding_model,
            topology: self.topology.clone(),
            quality: self.quality,
        }
    }
    fn validate(&self) -> crate::Result<()> {
        if let Some(overlay) = &self.topology {
            overlay.validate()?;
        }
        self.request().mapping().map_err(error)?;
        Ok(())
    }
}

pub use zipzi_contract::runtime::{ObservationImage, ObservedBounds};

pub use zipzi_contract::runtime::ObservationBatch;
pub use zipzi_contract::runtime::{
    ObservationEstimate, ObservationEstimateIssue, ObservationTexturePlan,
    ObservationTexturePlanning, ObservationTexturePolicy, ObservationTextureReport,
};

#[derive(Debug, Clone, Serialize)]
pub struct ObservationSampleBatch {
    #[serde(skip)]
    pub(crate) lease:
        std::sync::Arc<std::sync::OnceLock<std::sync::Arc<zipzi_project::TransferReservation>>>,
    pub source: crate::SourceRef,
    pub samples: ObservationBatch,
}

pub(crate) fn require_gpu_observation() -> crate::Result<()> {
    if cfg!(feature = "gpu-observation") {
        Ok(())
    } else {
        Err(gpu_unavailable())
    }
}
pub(crate) fn gpu_unavailable() -> crate::RuntimeError {
    crate::RuntimeError::Sdk(zipzi_sdk::SdkError {
        code: "GPU_UNAVAILABLE".into(),
        message: "This host was built without GPU observation support".into(),
        operation: "observation",
        object_ids: vec![],
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    })
}
pub(crate) fn error(e: zipzi_observe::ObservationError) -> crate::RuntimeError {
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

pub(crate) fn capture_limits(
    limits: &crate::RuntimeLimits,
) -> zipzi_observe::ObservationCaptureLimits {
    zipzi_observe::ObservationCaptureLimits {
        max_frames: limits.observation_frames,
        max_rgba_bytes: limits.observation_texture_rgba_bytes,
        png: zipzi_core::image::PngDecodeLimits {
            max_encoded_bytes: limits.observation_encoded_bytes,
            max_workspace_bytes: limits.observation_workspace_bytes,
        },
    }
}

#[cfg_attr(not(feature = "gpu-observation"), allow(dead_code))]
pub(crate) struct ObservationWork {
    pub plan: ObservationTexturePlan,
    pub limits: zipzi_observe::ObservationCaptureLimits,
}
pub(crate) fn check(cancel: &std::sync::atomic::AtomicBool) -> crate::Result<()> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Err(crate::RuntimeError::Cancelled)
    } else {
        Ok(())
    }
}
#[cfg(feature = "gpu-observation")]
pub(crate) fn observer(view: &ObservationView) -> crate::Result<Observer> {
    view.validate()?;
    Observer::new(ObserverConfig {
        width: view.width,
        height: view.height,
    })
    .map_err(error)
}
#[cfg(feature = "gpu-observation")]
pub(crate) fn render(
    observer: &mut Observer,
    capture: &ResolvedObservation,
    view: &ObservationView,
) -> crate::Result<ObservationImage> {
    let frame = observer.render(capture, view.request()).map_err(error)?;
    let mapping = frame.view;
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
        parameters: frame.parameters,
        visible_roi: [roi.x0, roi.y0, roi.x1, roi.y1],
        scale: mapping.scale,
        offset: [mapping.offset.0, mapping.offset.1],
        drawable_bounds: frame
            .drawable_bounds
            .into_iter()
            .map(|b| ObservedBounds {
                id: b.id,
                visible: b.visible,
                bounds: b.bounds.map(|(a, b, c, d)| [a, b, c, d]),
            })
            .collect(),
        quality: frame.quality,
    })
}
