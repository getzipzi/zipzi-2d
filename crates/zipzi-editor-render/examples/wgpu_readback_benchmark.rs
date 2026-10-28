//! Paired worker-stage benchmark: rendering + old/new GPUI readback paths.
//! No GPUI upload/presentation; not an FPS or input-latency measurement.
use anyhow::Result;
use std::{sync::Arc, time::Instant};
use zipzi_core::{Drawable, DrawableFrame, Vec2};
use zipzi_editor_core::{
    SceneSnapshot, SceneTexture, ViewportMetrics, ViewportPoint, ViewportTransform,
};
use zipzi_editor_render::{WgpuSceneRenderer, WgpuTargetConfig};

fn legacy_bgra(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let [red, green, blue, alpha] = *pixel;
        *pixel = match alpha {
            0 => [0; 4],
            255 => [blue, green, red, alpha],
            _ => {
                let alpha32 = u32::from(alpha);
                let straight =
                    |value: u8| ((u32::from(value) * 255 + alpha32 / 2) / alpha32).min(255) as u8;
                [straight(blue), straight(green), straight(red), alpha]
            }
        };
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let width = args
        .first()
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(1061u32);
    let height = args
        .get(1)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(1478u32);
    let samples = args
        .get(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(60usize);
    anyhow::ensure!(
        samples >= 2 && width > 0 && height > 0,
        "width height samples>=2"
    );
    // Transparent background plus a textured, translucent triangle. Every frame
    // moves the view, and both paths consume the identical rendered texture.
    let frame = Arc::new(DrawableFrame {
        drawables: vec![Drawable {
            id: "triangle".into(),
            texture_asset_id: "color".into(),
            positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(width as f32, 0.),
                Vec2::new(0., -(height as f32)),
            ],
            uvs: Arc::from([Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }],
        ..Default::default()
    });
    let scene = SceneSnapshot {
        model_opacity: 1.,
        runtime_frame: frame.clone(),
        viewport_frame: frame,
        textures: Arc::from([SceneTexture {
            asset_id: "color".into(),
            cache_key: "mixed-alpha".into(),
            width: 2,
            height: 2,
            rgba: Arc::from([
                240, 70, 40, 255, 30, 160, 240, 160, 80, 30, 200, 32, 0, 0, 0, 0,
            ]),
        }]),
    };
    let target = WgpuTargetConfig {
        width,
        height,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut renderer = WgpuSceneRenderer::new(target)?;
    println!(
        "adapter={} backend={} debug={} width={width} height={height} bytes={} samples={samples} warmup=20",
        renderer.context().adapter_name(),
        renderer.context().adapter_backend(),
        cfg!(debug_assertions),
        u64::from(width) * u64::from(height) * 4
    );
    println!("sample,path,submit_ms,readback_ms,cpu_conversion_ms,total_ms");
    for sample in 0..samples + 20 {
        let begin = Instant::now();
        let frame = renderer.submit_transformed(
            target,
            &scene,
            ViewportTransform {
                scale: 1.,
                offset: ViewportPoint {
                    x: (sample % 7) as f32,
                    y: 0.,
                },
                metrics: ViewportMetrics {
                    width: width as f32,
                    height: height as f32,
                    scale_factor: 1.,
                },
            },
        )?;
        let submit_ms = begin.elapsed().as_secs_f64() * 1000.;
        let mut results = [Vec::new(), Vec::new()];
        // Alternate order to avoid systematically assigning GPU render completion
        // to one path. readback_ms includes that completion when this path runs first.
        for path in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            let mut bytes = if path == 0 {
                renderer
                    .context()
                    .read_rgba8(&frame.image)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?
            } else {
                renderer.read_straight_bgra8(&frame.image)?
            };
            let read_ms = start.elapsed().as_secs_f64() * 1000.;
            let conversion = Instant::now();
            if path == 0 {
                legacy_bgra(&mut bytes);
            }
            let conversion_ms = conversion.elapsed().as_secs_f64() * 1000.;
            if sample >= 20 {
                println!(
                    "{},{},{submit_ms:.6},{read_ms:.6},{conversion_ms:.6},{:.6}",
                    sample - 20,
                    if path == 0 { "legacy" } else { "gpu" },
                    submit_ms + read_ms + conversion_ms
                );
            }
            results[path] = bytes;
        }
        anyhow::ensure!(results[0] == results[1], "BGRA mismatch at sample {sample}");
        std::hint::black_box(results);
    }
    Ok(())
}
