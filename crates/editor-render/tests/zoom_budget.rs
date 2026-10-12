#![cfg(target_os = "macos")]

use editor_core::{CanvasMetrics, CanvasPoint, Document, ViewState, ViewportTransform};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use kasane_render_metal::{metal, read_rgba8};

#[test]
#[ignore = "requires models/local/mao_pro.kasane/project.kasane.json"]
fn mao_high_zoom_crops_masks_with_stable_pixels_after_panning() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../models/local/mao_pro.kasane/project.kasane.json")
            .canonicalize()?;
        let mut document = Document::open(&path)?;
        let scene = document.snapshot()?;
        let frame = &scene.viewport_frame;
        // Inspect both the canvas center and authored masked content, so a
        // transparent viewport cannot make this regression pass accidentally.
        let anchors: Vec<_> = frame
            .drawables
            .iter()
            .filter(|drawable| {
                drawable.visible
                    && drawable.opacity > 0.
                    && !drawable.masks.is_empty()
                    && !drawable.positions.is_empty()
            })
            .take(2)
            .map(|drawable| {
                let count = drawable.positions.len() as f32;
                let x = drawable.positions.iter().map(|p| p.x).sum::<f32>() / count;
                let y = drawable.positions.iter().map(|p| p.y).sum::<f32>() / count;
                CanvasPoint {
                    x: x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                    y: frame.canvas.origin.y - y * frame.canvas.pixels_per_unit,
                }
            })
            .collect();
        assert_eq!(anchors.len(), 2);
        let device = metal::Device::system_default().expect("Metal device");
        let queue = device.new_command_queue();
        let mut renderer = MetalSceneRenderer::new(device, MetalTargetConfig::default())?;
        let mut content_frames = 0;
        for (width, height) in [(1280, 800), (2560, 1600)] {
            let config = MetalTargetConfig {
                width,
                height,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let metrics = CanvasMetrics {
                width: width as f32 / 2.,
                height: height as f32 / 2.,
                scale_factor: 2.,
            };
            for zoom in [1., 16., 20., 24., 32.] {
                let capture = |renderer: &mut MetalSceneRenderer,
                               pan: CanvasPoint|
                 -> anyhow::Result<Vec<u8>> {
                    let transform =
                        ViewportTransform::new(frame.canvas, metrics, ViewState { zoom, pan })
                            .unwrap();
                    let submitted =
                        renderer.submit_transformed(&queue, config, &scene, transform)?;
                    submitted.command.wait_until_completed();
                    assert_eq!(
                        submitted.command.status(),
                        metal::MTLCommandBufferStatus::Completed
                    );
                    let stats = renderer.last_frame_stats();
                    assert!(
                        stats.attachment_budget_bytes
                            <= kasane_render::OFFSCREEN_BUDGET_BYTES as u64
                    );
                    assert_eq!(
                        stats.mask_resolution_limit, 4096,
                        "Mao should retain full visible density"
                    );
                    read_rgba8(&submitted.image)
                        .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))
                };
                let first = capture(&mut renderer, CanvasPoint::default())?;
                assert_eq!(first, capture(&mut renderer, CanvasPoint::default())?);
                assert_eq!(
                    renderer.last_frame_stats().masks,
                    0,
                    "Warm frames reuse cropped masks"
                );
                let mut peak = renderer.last_frame_stats().attachment_budget_bytes;
                let mut covered = first.chunks_exact(4).filter(|pixel| pixel[3] != 0).count();
                for anchor in &anchors {
                    let pixels = capture(
                        &mut renderer,
                        CanvasPoint {
                            x: (frame.canvas.width / 2. - anchor.x) * zoom / metrics.scale_factor,
                            y: (frame.canvas.height / 2. - anchor.y) * zoom / metrics.scale_factor,
                        },
                    )?;
                    covered += pixels.chunks_exact(4).filter(|pixel| pixel[3] != 0).count();
                    peak = peak.max(renderer.last_frame_stats().attachment_budget_bytes);
                }
                assert_eq!(
                    first,
                    capture(&mut renderer, CanvasPoint::default())?,
                    "Returning from a pan must restore identical pixels"
                );
                if covered > 0 {
                    content_frames += 1;
                }
                println!(
                    "Mao {width}x{height}, {:.0}%: peak reservation={:.2} MiB, covered pixels={covered}",
                    zoom * 100.,
                    peak as f64 / 1024. / 1024.
                );
            }
        }
        assert_eq!(content_frames, 10, "Every zoom must render actual artwork");
        assert_eq!(
            renderer.stats().1,
            scene.textures.len() as u64,
            "Zoom and pan must not re-upload textures"
        );
        Ok(())
    })
}
