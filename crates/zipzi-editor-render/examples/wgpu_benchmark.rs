//! Backend A/B: CPU submission and completed-frame wall time, without readback.
use anyhow::{Result, ensure};
use std::{path::PathBuf, sync::Arc, time::Instant};
use zipzi_core::Vec2;
use zipzi_editor_core::{
    Editor, LoadRequest, SceneSnapshot, ViewState, ViewportId, ViewportKind, ViewportMetrics,
    ViewportPoint, ViewportTransform,
};
use zipzi_editor_render::{MetalSceneRenderer, MetalTargetConfig, WgpuSceneRenderer};
use zipzi_render_metal::metal;
use zipzi_render_wgpu::WgpuTargetConfig;
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 2,
        "usage: wgpu_benchmark MODEL3 OUTPUT.csv [samples=90] [rounds=3]"
    );
    let samples: usize = args.get(2).map_or(Ok(90), |s| s.parse())?;
    let rounds: usize = args.get(3).map_or(Ok(3), |s| s.parse())?;
    ensure!(
        samples >= 30 && rounds > 0,
        "At least 30 samples and one round required"
    );
    let metrics = ViewportMetrics {
        width: 640.,
        height: 400.,
        scale_factor: 2.,
    };
    let mut editor = Editor::empty();
    editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
    editor.set_canvas_metrics(metrics);
    let task = editor
        .prepare_load(LoadRequest::Model3(PathBuf::from(&args[0]).canonicalize()?))?
        .unwrap();
    editor.finish_load(task.run())?;
    let preview = editor.preview()?;
    let base_view = preview.view;
    let scene = preview.scene;
    let wt = WgpuTargetConfig {
        width: 1280,
        height: 800,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mt = MetalTargetConfig {
        width: 1280,
        height: 800,
        format: metal::MTLPixelFormat::RGBA8Unorm,
    };
    let mut w = WgpuSceneRenderer::new(wt)?;
    let device = metal::Device::system_default().unwrap();
    let queue = device.new_command_queue();
    let mut m = MetalSceneRenderer::new(device, mt)?;
    println!(
        "adapter={} backend={} release={} meshes={} samples={samples} rounds={rounds} size=1280x800 warmup=20 fit_zoom={} fit_pan={},{}",
        w.context().adapter_name(),
        w.context().adapter_backend(),
        !cfg!(debug_assertions),
        scene.viewport_frame.drawables.len(),
        base_view.zoom,
        base_view.pan.x,
        base_view.pan.y
    );
    let mut csv = String::from(
        "round,case,backend,sample,cpu_ms,complete_ms,draws,passes,mask_redraws,mask_hits,buffer_creations,bind_groups,color_bytes\n",
    );
    for round in 0..rounds {
        for case in ["idle", "pan", "deform"] {
            for backend in if round % 2 == 0 {
                ["wgpu", "metal"]
            } else {
                ["metal", "wgpu"]
            } {
                let mut cpu = Vec::new();
                let mut complete = Vec::new();
                for sample in 0..samples + 20 {
                    let phase = sample as f32 * 0.07;
                    let mut current: SceneSnapshot = scene.clone();
                    if case == "deform" {
                        for d in &mut Arc::make_mut(&mut current.viewport_frame).drawables {
                            for p in &mut d.positions {
                                *p = Vec2::new(p.x + phase.sin() * 0.4, p.y + phase.cos() * 0.3);
                            }
                        }
                    }
                    let view = ViewState {
                        pan: if case == "pan" {
                            ViewportPoint {
                                x: base_view.pan.x + phase.sin() * 12.,
                                y: base_view.pan.y + phase.cos() * 8.,
                            }
                        } else {
                            base_view.pan
                        },
                        ..base_view
                    };
                    let transform = ViewportTransform::new(metrics, view).unwrap();
                    let (submit, done, draws, passes, redraws, hits, buffers, binds, bytes) =
                        objc2::rc::autoreleasepool(|_| -> Result<_> {
                            let start = Instant::now();
                            if backend == "wgpu" {
                                let frame = w.submit_transformed(wt, &current, transform)?;
                                let cpu = start.elapsed().as_secs_f64() * 1000.;
                                w.context().device().poll(wgpu::PollType::Wait {
                                    submission_index: Some(frame.submission),
                                    timeout: Some(std::time::Duration::from_secs(30)),
                                })?;
                                let done = start.elapsed().as_secs_f64() * 1000.;
                                let s = w.last_frame_stats();
                                Ok((
                                    cpu,
                                    done,
                                    s.draw_calls,
                                    s.render_passes,
                                    s.mask_redraws,
                                    s.mask_cache_hits,
                                    s.geometry_buffer_creations,
                                    s.bind_group_creations,
                                    s.color_attachment_bytes,
                                ))
                            } else {
                                let frame =
                                    m.submit_transformed(&queue, mt, &current, transform)?;
                                let cpu = start.elapsed().as_secs_f64() * 1000.;
                                frame.command.wait_until_completed();
                                ensure!(
                                    frame.command.status()
                                        == metal::MTLCommandBufferStatus::Completed,
                                    "Metal command failed"
                                );
                                let done = start.elapsed().as_secs_f64() * 1000.;
                                let s = m.last_frame_stats();
                                Ok((
                                    cpu,
                                    done,
                                    s.draw_calls,
                                    s.render_passes,
                                    s.masks,
                                    s.mask_cache_hits,
                                    s.buffer_uploads as u64,
                                    0,
                                    s.color_attachment_bytes,
                                ))
                            }
                        })?;
                    if sample >= 20 {
                        cpu.push(submit);
                        complete.push(done);
                        use std::fmt::Write;
                        writeln!(
                            &mut csv,
                            "{round},{case},{backend},{},{submit:.6},{done:.6},{draws},{passes},{redraws},{hits},{buffers},{binds},{bytes}",
                            sample - 20
                        )?;
                    }
                }
                let p50 = percentile(&mut cpu, 0.5);
                let p95 = percentile(&mut cpu, 0.95);
                let c50 = percentile(&mut complete, 0.5);
                let c95 = percentile(&mut complete, 0.95);
                println!(
                    "round={round} case={case} backend={backend} cpu_p50={p50:.3} cpu_p95={p95:.3} complete_p50={c50:.3} complete_p95={c95:.3}"
                );
            }
        }
    }
    std::fs::write(&args[1], csv)?;
    Ok(())
}
