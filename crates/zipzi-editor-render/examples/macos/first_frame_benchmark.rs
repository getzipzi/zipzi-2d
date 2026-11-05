use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, HashSet},
    hint::black_box,
    path::PathBuf,
    time::Instant,
};
use zipzi_editor_core::{Editor, LoadRequest, ViewportId, ViewportKind, ViewportMetrics};
use zipzi_editor_render::{MetalSceneRenderer, MetalTargetConfig};
use zipzi_render::texture::{area_average_rgba8, straight_rgba_mipmaps};
use zipzi_render_metal::{metal, read_rgba8};

fn reference_mips(mut width: u32, mut height: u32, rgba: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    while width > 1 || height > 1 {
        let source = levels.last().map_or(rgba, |(_, _, p)| p.as_slice());
        let (w, h) = ((width / 2).max(1), (height / 2).max(1));
        levels.push((w, h, area_average_rgba8(width, height, source, w, h)));
        (width, height) = (w, h);
    }
    levels
}

pub fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        !args.is_empty(),
        "Usage: first_frame_benchmark PROJECT [ROUNDS=3] [RGBA_OUTPUT]"
    );
    let rounds: usize = args.get(1).map_or(Ok(3), |s| s.parse())?;
    ensure!(rounds > 0, "rounds must be positive");
    let mut editor = Editor::empty();
    editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 470.5,
        height: 656.,
        scale_factor: 2.,
    });
    let task = editor
        .prepare_load(LoadRequest::Open(PathBuf::from(&args[0]).canonicalize()?))?
        .unwrap();
    editor.finish_load(task.run())?;
    let preview = editor.preview()?;
    let scene = preview.scene;
    let transform = preview.canvas.unwrap().transform;
    ensure!(!scene.runtime_frame.drawables.is_empty(), "empty scene");
    let mut keys = HashSet::new();
    let sources: Vec<_> = scene
        .textures
        .iter()
        .filter(|s| keys.insert((s.cache_key.clone(), s.width, s.height)))
        .collect();
    // Validate real artwork against the independent general area filter first.
    for source in &sources {
        ensure!(
            straight_rgba_mipmaps(source.width, source.height, &source.rgba)
                == reference_mips(source.width, source.height, &source.rgba),
            "mipmap bytes changed for {}",
            source.asset_id
        );
    }
    println!(
        "{{\"type\":\"fixture\",\"textures\":{},\"unique_textures\":{},\"rgba_bytes\":{},\"mipmap_bytes_equal\":true}}",
        scene.textures.len(),
        sources.len(),
        sources.iter().map(|s| s.rgba.len()).sum::<usize>()
    );
    for round in 0..rounds {
        for reference in if round % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let start = Instant::now();
            for source in &sources {
                let levels = if reference {
                    reference_mips(source.width, source.height, &source.rgba)
                } else {
                    straight_rgba_mipmaps(source.width, source.height, &source.rgba)
                };
                black_box(levels);
            }
            println!(
                "{{\"type\":\"mipmaps\",\"round\":{round},\"reference\":{reference},\"elapsed_ms\":{}}}",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
    }
    let device = metal::Device::system_default().expect("Metal device");
    let queue = device.new_command_queue();
    let config = MetalTargetConfig {
        width: 941,
        height: 1312,
        format: metal::MTLPixelFormat::RGBA8Unorm,
    };
    let mut expected_pixels = None;
    for round in 0..rounds {
        objc2::rc::autoreleasepool(|_| -> Result<()> {
            let mut renderer = MetalSceneRenderer::new(device.clone(), config)?;
            let mut stages = BTreeMap::new();
            let start = Instant::now();
            let frame =
                renderer.submit_transformed_traced(&queue, config, &scene, transform, |stage| {
                    stages.insert(stage, start.elapsed().as_secs_f64() * 1000.);
                })?;
            frame.command.wait_until_completed();
            let complete_ms = start.elapsed().as_secs_f64() * 1000.;
            ensure!(
                frame.command.status() == metal::MTLCommandBufferStatus::Completed,
                "GPU failed"
            );
            let pixels = read_rgba8(&frame.image)
                .map_err(|e| anyhow::anyhow!("{}: {}", e.code, e.message))?;
            if let Some(expected) = &expected_pixels {
                ensure!(*expected == pixels, "cold frames differ");
            } else {
                ensure!(pixels.chunks_exact(4).any(|p| p[3] > 0), "empty render");
                if let Some(path) = args.get(2) {
                    std::fs::write(path, &pixels)?;
                }
                expected_pixels = Some(pixels);
            }
            println!(
                "{{\"type\":\"cold_frame\",\"round\":{round},\"texture_ms\":{},\"submit_ms\":{},\"complete_ms\":{complete_ms},\"uploads\":{},\"texture_bytes\":{},\"draw_calls\":{},\"pixels_equal\":true}}",
                stages["textures_ready"],
                stages["preview_submitted"],
                renderer.stats().1,
                renderer.stats().2,
                renderer.last_frame_stats().draw_calls
            );
            Ok(())
        })?;
    }
    Ok(())
}
