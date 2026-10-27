//! Load a real Editor model and compare wgpu/native Metal at several views.
use anyhow::{Result, ensure};
use std::{path::PathBuf, time::Instant};
use zipzi_editor_core::{
    Editor, LoadRequest, ViewState, ViewportId, ViewportKind, ViewportMetrics, ViewportTransform,
};
use zipzi_editor_render::{MetalSceneRenderer, MetalTargetConfig, WgpuSceneRenderer};
use zipzi_render_metal::{metal, read_rgba8};
use zipzi_render_wgpu::WgpuTargetConfig;
fn png(path: &std::path::Path, rgba: &[u8]) -> Result<()> {
    let mut encoder = png::Encoder::new(std::fs::File::create(path)?, 512, 512);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let model = PathBuf::from(args.next().expect("model3 path")).canonicalize()?;
    let output = PathBuf::from(args.next().unwrap_or_else(|| "target/wgpu-preview".into()));
    std::fs::create_dir_all(&output)?;
    let metrics = ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    };
    let mut editor = Editor::empty();
    editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
    editor.set_canvas_metrics(metrics);
    let task = editor.prepare_load(LoadRequest::Model3(model))?.unwrap();
    editor.finish_load(task.run())?;
    let preview = editor.preview()?;
    let wt = WgpuTargetConfig {
        width: 512,
        height: 512,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mt = MetalTargetConfig {
        width: 512,
        height: 512,
        format: metal::MTLPixelFormat::RGBA8Unorm,
    };
    let device = metal::Device::system_default().unwrap();
    let queue = device.new_command_queue();
    let mut w = WgpuSceneRenderer::new(wt)?;
    let mut m = MetalSceneRenderer::new(device, mt)?;
    println!(
        "adapter={} backend={} meshes={} masks={}",
        w.context().adapter_name(),
        w.context().adapter_backend(),
        preview.scene.viewport_frame.drawables.len(),
        preview
            .scene
            .viewport_frame
            .drawables
            .iter()
            .filter(|d| !d.masks.is_empty())
            .count()
    );
    for (i, zoom) in [1., 1.1, 0.6].into_iter().enumerate() {
        let transform = ViewportTransform::new(
            metrics,
            ViewState {
                zoom,
                ..Default::default()
            },
        )
        .unwrap();
        let start = Instant::now();
        let submitted = w.submit_transformed(wt, &preview.scene, transform)?;
        let actual = w
            .context()
            .read_rgba8(&submitted.image)
            .map_err(|s| anyhow::anyhow!("{}", s.message))?;
        let ms = start.elapsed().as_secs_f64() * 1000.;
        let expected = objc2::rc::autoreleasepool(|_| -> Result<_> {
            let submitted = m.submit_transformed(&queue, mt, &preview.scene, transform)?;
            submitted.command.wait_until_completed();
            read_rgba8(&submitted.image).map_err(|s| anyhow::anyhow!("{}", s.message))
        })?;
        let diff: Vec<u8> = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        let max = *diff.iter().max().unwrap();
        let mean = diff.iter().map(|&b| u64::from(b)).sum::<u64>() as f64 / diff.len() as f64;
        let bad = diff
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p.iter().any(|&v| v > 8))
            .count();
        png(&output.join(format!("{i}-wgpu.png")), &actual)?;
        png(&output.join(format!("{i}-metal.png")), &expected)?;
        png(&output.join(format!("{i}-diff.png")), &diff)?;
        let stats = w.last_frame_stats();
        println!(
            "view={i} zoom={zoom} wgpu_ms={ms:.3} max_error={max} mean_error={mean:.6} bad_pixels={bad} passes={} draws={} mask_redraws={} mask_hits={} attachment_bytes={} vertex_upload_bytes={}",
            stats.render_passes,
            stats.draw_calls,
            stats.mask_redraws,
            stats.mask_cache_hits,
            stats.attachment_bytes,
            stats.vertex_upload_bytes
        );
        ensure!(
            actual.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
            "Empty model image"
        );
        ensure!(
            mean < 0.5 && bad < 512 * 512 / 1000,
            "wgpu/Metal pixel mismatch"
        );
        if i > 0 {
            ensure!(stats.vertex_upload_bytes == 0, "Pan/zoom uploaded geometry");
        }
    }
    Ok(())
}
