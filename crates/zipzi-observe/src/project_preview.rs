//! Default-pose static project image; independent of the current Editor view.
use crate::*;

/// Conservative extra admission: source decode, fitted textures/mips, output,
/// PNG validation and a GPU render target. GPU driver allocations are not an RSS cap.
pub const PROJECT_PREVIEW_WORKSPACE: usize = 256 << 20;

pub fn render_project_preview(
    snapshot: &SourcedSnapshot,
    control: &zipzi_sdk::ConversionControl,
) -> Result<Option<zipzi_project::archive::ProjectPreview>, ObservationError> {
    check_capture(control)?;
    let input = ObservationInput::capture_from_snapshot(snapshot, &PreviewValues::new())?;
    let mut roi = ModelRoi {
        x0: f32::INFINITY,
        y0: f32::INFINITY,
        x1: f32::NEG_INFINITY,
        y1: f32::NEG_INFINITY,
    };
    for draw in &input.frame.drawables {
        if !draw.visible || !draw.enabled || !draw.renderable || draw.opacity <= 0. {
            continue;
        }
        for position in &draw.positions {
            roi.x0 = roi.x0.min(position.x);
            roi.x1 = roi.x1.max(position.x);
            roi.y0 = roi.y0.min(position.y);
            roi.y1 = roi.y1.max(position.y);
        }
    }
    if roi.x1 <= roi.x0 || roi.y1 <= roi.y0 {
        return Ok(None);
    }
    let planning::ObservationTexturePlanning::Ready { plan } = planning::plan_textures(
        planning::texture_sources(snapshot.snapshot.document())?,
        planning::ObservationTexturePolicy::FitBudget {
            max_rgba_bytes: 32 << 20,
        },
        32 << 20,
    )?
    else {
        return Err(gpu_error(
            "PROJECT_PREVIEW_BUDGET",
            "Textures exceed the static preview budget",
        ));
    };
    let mut resolved = ResolvedObservation::capture_many_with_plan(
        vec![input],
        &plan,
        ObservationCaptureLimits {
            max_frames: 1,
            max_rgba_bytes: 32 << 20,
            png: zipzi_core::image::PngDecodeLimits {
                max_encoded_bytes: 32 << 20,
                max_workspace_bytes: 128 << 20,
            },
        },
        false,
        control,
    )?;
    check_capture(control)?;
    let edge = 1024;
    let mut observer = Observer::new(ObserverConfig {
        width: edge,
        height: edge,
    })?;
    let frame = observer.render(
        &resolved.remove(0),
        RenderRequest {
            width: edge,
            height: edge,
            padding_model: (roi.x1 - roi.x0).max(roi.y1 - roi.y0) * 0.04,
            roi,
            topology: None,
            quality: false,
        },
    )?;
    check_capture(control)?;
    let png = frame.png_bytes()?;
    let preview = zipzi_project::archive::ProjectPreview::from_png(png)
        .map_err(|e| gpu_error("PROJECT_PREVIEW_PNG", e.to_string()))?;
    Ok(Some(preview))
}

fn gpu_error(code: &str, message: impl Into<String>) -> ObservationError {
    ObservationError {
        code: code.into(),
        message: message.into(),
        asset_id: None,
    }
}
