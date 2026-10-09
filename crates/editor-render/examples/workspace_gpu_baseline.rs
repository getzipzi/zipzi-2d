//! Native Metal workload benchmark. No window, UI automation, second device or readback.
use editor_core::{CanvasMetrics, Editor, LoadRequest, ViewState, ViewportTransform};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use kasane_render_metal::metal;
use std::{path::PathBuf, time::Instant};
fn rss() -> u64 {
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        * 1024
}
fn memory(stage: &str, renderer: &MetalSceneRenderer, device: &metal::DeviceRef) {
    let (draws, uploads, bytes) = renderer.stats();
    println!(
        "memory,{stage},0,0,{draws},{uploads},{bytes},{},{}",
        device.current_allocated_size(),
        rss()
    );
}
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let first = PathBuf::from(args.next().expect("Mao model3 path")).canonicalize()?;
    let second = PathBuf::from(args.next().expect("Shirousagi model3 path")).canonicalize()?;
    let samples: usize = args.next().unwrap_or_else(|| "30".into()).parse()?;
    let device = metal::Device::system_default().expect("Metal device");
    let queue = device.new_command_queue();
    let metrics = CanvasMetrics {
        width: 640.,
        height: 400.,
        scale_factor: 2.,
    };
    let config = MetalTargetConfig {
        width: 1280,
        height: 800,
        format: metal::MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalSceneRenderer::new(device.clone(), config)?;
    let mut editor = Editor::empty();
    editor.set_canvas_metrics(metrics);
    eprintln!(
        "device={}, profile=dev, logical=640x400, scale=2, physical=1280x800, HUD=off, samples={samples}",
        device.name()
    );
    println!(
        "kind,stage,sample,milliseconds,draws,uploads,texture_cache_bytes,gpu_allocated_bytes,rss_bytes"
    );
    for (stage, path) in [("mao", first), ("shirousagi", second)] {
        let start = Instant::now();
        let task = editor.prepare_load(LoadRequest::Model3(path))?.unwrap();
        editor.finish_load(task.run())?;
        println!(
            "cpu,{stage}_import,0,{:.6},0,0,0,0,0",
            start.elapsed().as_secs_f64() * 1000.
        );
        for index in 0..=samples {
            let start = Instant::now();
            objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
                let preview = editor.preview()?;
                let transform = ViewportTransform::new(
                    preview.scene.viewport_frame.canvas,
                    metrics,
                    ViewState {
                        zoom: 1. + (index as f32 % 4.) * 0.05,
                        ..Default::default()
                    },
                )
                .unwrap();
                let _image =
                    renderer.draw_transformed(&queue, config, &preview.scene, transform)?;
                let barrier = queue.new_command_buffer();
                barrier.commit();
                barrier.wait_until_completed();
                Ok(())
            })?;
            println!(
                "frame,{stage}_{},{index},{:.6},0,0,0,0,0",
                if index == 0 { "first" } else { "warm" },
                start.elapsed().as_secs_f64() * 1000.
            );
        }
        memory(stage, &renderer, &device);
    }
    for index in 0..samples {
        let start = Instant::now();
        objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
            editor.activate(editor.adjacent(true).unwrap())?;
            let preview = editor.preview()?;
            let _image = renderer.draw_transformed(
                &queue,
                config,
                &preview.scene,
                preview.canvas.unwrap().transform,
            )?;
            let barrier = queue.new_command_buffer();
            barrier.commit();
            barrier.wait_until_completed();
            Ok(())
        })?;
        println!(
            "frame,switch,{index},{:.6},0,0,0,0,0",
            start.elapsed().as_secs_f64() * 1000.
        );
    }
    memory("dual_after_switches", &renderer, &device);
    editor.close_document(editor.active_id().unwrap(), true)?;
    memory("one_document_closed", &renderer, &device);
    drop(editor);
    drop(renderer);
    println!(
        "memory,all_released,0,0,0,0,0,{},{}",
        device.current_allocated_size(),
        rss()
    );
    Ok(())
}
