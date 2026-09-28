//! Reproducible 120 Hz budget probe. CPU submission and GPU completion are
//! measured separately; this is not a window/input-to-photon measurement.
use editor_core::{
    CanvasMetrics, CanvasPoint, Editor, EditorCommand, LoadRequest, ViewState, ViewportTransform,
};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use kasane_render_metal::metal;
use std::{path::PathBuf, sync::Arc, time::Instant};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("model3 path")).canonicalize()?;
    let samples: usize = args.next().unwrap_or_else(|| "100".into()).parse()?;
    let width: u32 = args.next().unwrap_or_else(|| "1280".into()).parse()?;
    let height: u32 = args.next().unwrap_or_else(|| "800".into()).parse()?;
    let device = metal::Device::system_default().expect("Metal device");
    let queue = device.new_command_queue();
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
    let mut editor = Editor::empty();
    editor.set_canvas_metrics(metrics);
    let task = editor.prepare_load(LoadRequest::Model3(path))?.unwrap();
    editor.finish_load(task.run())?;
    let scene = editor.preview()?.scene;
    let mut renderer = MetalSceneRenderer::new(device.clone(), config)?;
    eprintln!(
        "device={}, debug_assertions={}, physical={}x{}, samples={}",
        device.name(),
        cfg!(debug_assertions),
        width,
        height,
        samples
    );
    println!(
        "mode,sample,edit_ms,scene_ms,submit_ms,complete_ms,gpu_ms,draw_calls,render_passes,mask_redraws,mask_hits,buffer_uploads"
    );
    for mode in ["pan", "zoom", "mesh_edit", "deform_all", "opacity_edit"] {
        let mut edited = scene.clone();
        for sample in 0..samples + 5 {
            let view = ViewState {
                zoom: if mode == "zoom" {
                    0.25 + (sample % 60) as f32 / 59. * 2.75
                } else {
                    1.
                },
                pan: CanvasPoint {
                    x: (sample as f32 * 0.13).sin() * 80.,
                    y: (sample as f32 * 0.07).cos() * 40.,
                },
            };
            let transform = ViewportTransform::new(scene.frame.canvas, metrics, view).unwrap();
            let start = Instant::now();
            let (mut edit_ms, mut scene_ms) = (0., 0.);
            if mode == "opacity_edit" {
                editor.execute(EditorCommand::PreviewOpacity(
                    0.5 + (sample % 50) as f32 * 0.01,
                ))?;
                edit_ms = start.elapsed().as_secs_f64() * 1000.;
                edited = editor.preview()?.scene;
                scene_ms = start.elapsed().as_secs_f64() * 1000. - edit_ms;
            }
            if mode == "mesh_edit" {
                // An immutable scene update with one changed mesh. Include the
                // snapshot copy; SDK evaluation is covered by the core benchmark.
                let frame = Arc::make_mut(&mut edited.frame);
                if let Some(mesh) = frame
                    .drawables
                    .iter_mut()
                    .find(|d| d.visible && !d.positions.is_empty())
                {
                    mesh.positions[0].x += if sample % 2 == 0 { 0.001 } else { -0.001 };
                }
            }
            if mode == "deform_all" {
                let frame = Arc::make_mut(&mut edited.frame);
                let delta = if sample % 2 == 0 { 0.001 } else { -0.001 };
                for mesh in &mut frame.drawables {
                    for position in &mut mesh.positions {
                        position.x += delta;
                    }
                }
            }
            let (submit, complete, gpu_ms) =
                objc2::rc::autoreleasepool(|_| -> anyhow::Result<_> {
                    let submitted =
                        renderer.submit_transformed(&queue, config, &edited, transform)?;
                    let submit = start.elapsed().as_secs_f64() * 1000.;
                    submitted.command.wait_until_completed();
                    let complete = start.elapsed().as_secs_f64() * 1000.;
                    anyhow::ensure!(
                        submitted.command.status() == metal::MTLCommandBufferStatus::Completed,
                        "GPU command failed"
                    );
                    // Metal exposes these completed-command timestamps on macOS
                    // 10.15+. They exclude CPU wake-up and pre-execution queue time.
                    // metal-rs owns this Objective-C object for the entire
                    // borrow; objc2 supplies the missing timestamp selectors.
                    let command = unsafe {
                        &*(submitted.command.as_ref() as *const metal::CommandBufferRef
                            as *const objc2::runtime::AnyObject)
                    };
                    let gpu_start: f64 = unsafe { objc2::msg_send![command, GPUStartTime] };
                    let gpu_end: f64 = unsafe { objc2::msg_send![command, GPUEndTime] };
                    Ok((submit, complete, (gpu_end - gpu_start) * 1000.))
                })?;
            if sample >= 5 {
                let stats = renderer.last_frame_stats();
                println!(
                    "{mode},{},{edit_ms:.6},{scene_ms:.6},{submit:.6},{complete:.6},{gpu_ms:.6},{},{},{},{},{}",
                    sample - 5,
                    stats.draw_calls,
                    stats.render_passes,
                    stats.masks,
                    stats.mask_cache_hits,
                    stats.buffer_uploads
                );
            }
        }
    }
    Ok(())
}
