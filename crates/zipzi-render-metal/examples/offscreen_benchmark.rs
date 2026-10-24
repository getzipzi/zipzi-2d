//! Reproducible native GPU benchmark; run the same binary source before/after a change.
#[allow(unexpected_cfgs)]
#[cfg(target_os = "macos")]
fn main() {
    use metal::objc::{msg_send, sel, sel_impl};
    use std::{sync::Arc, time::Instant};
    use zipzi_core::{
        evaluation::{DrawableFrame, OffscreenFrame, RenderCommand},
        Drawable, Vec2,
    };
    use zipzi_render::{Affine2, ViewportConfig};
    use zipzi_render_metal::{
        metal, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer, MetalTargetConfig,
        MetalTexture, MetalTextureCatalog,
    };
    let directory = std::env::args().nth(1).expect("output directory");
    std::fs::create_dir_all(&directory).unwrap();
    metal::objc::rc::autoreleasepool(|| {
        let context = MetalContext::new().unwrap();
        println!(
            "device={} global_surfaces={}",
            context.device().name(),
            cfg!(feature = "benchmark-global-surfaces")
        );
        let texture = context
            .upload_rgba8(
                2,
                2,
                &[
                    220, 40, 80, 210, 20, 220, 100, 150, 60, 100, 240, 180, 230, 210, 40, 240,
                ],
                &[],
            )
            .unwrap();
        let mut catalog = MetalTextureCatalog::new(std::collections::HashMap::from([(
            "art".into(),
            MetalTexture {
                view: &texture,
                width: 2,
                height: 2,
            },
        )]));
        catalog.set_revision("art", 1);
        for (name, count, side, nested, extended, masked) in [
            ("small_siblings", 40, 96., false, false, false),
            ("mixed_sizes", 40, 64., false, false, false),
            ("nested_blend_mask", 24, 120., true, true, true),
            ("full_viewport", 8, 2048., true, true, false),
        ] {
            let mut frame = DrawableFrame::default();
            let mesh = |id: String, x: f32, y: f32, side: f32| Drawable {
                id,
                texture_asset_id: "art".into(),
                positions: vec![
                    Vec2::new(x, -y),
                    Vec2::new(x + side, -y),
                    Vec2::new(x + side, -y - side),
                    Vec2::new(x, -y - side),
                ],
                uvs: Arc::from([
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(1., 1.),
                    Vec2::new(0., 1.),
                ]),
                indices: Arc::from([0, 1, 2, 0, 2, 3]),
                ..Default::default()
            };
            let mut background = mesh("background".into(), 0., 0., 2048.);
            background.opacity = 0.6;
            frame.drawables.push(background);
            frame.render_plan.push(RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            });
            if masked {
                let mut mask = mesh("mask".into(), 350., 400., 900.);
                mask.visible = false;
                frame.drawables.push(mask);
                frame.render_plan.push(RenderCommand::DrawMesh {
                    mesh_id: "mask".into(),
                });
            }
            if nested {
                frame.offscreens.push(OffscreenFrame {
                    id: "outer".into(),
                    enabled: true,
                    opacity: 0.8,
                    multiply_color: [1.; 4],
                    ..Default::default()
                });
                frame.render_plan.push(RenderCommand::BeginOffscreen {
                    offscreen_id: "outer".into(),
                });
            }
            for i in 0..count {
                let id = format!("g{i}");
                let extent = if name == "mixed_sizes" {
                    side + (i % 9) as f32 * 13.
                } else {
                    side
                };
                let (x, y) = if side > 1000. {
                    (0., 0.)
                } else {
                    (30. + (i % 8) as f32 * 240., 50. + (i / 8) as f32 * 280.)
                };
                let mut draw = mesh(format!("m{i}"), x, y, extent);
                if extended {
                    draw.raw_blend_mode = Some(7);
                }
                if masked {
                    draw.masks = vec!["mask".into()];
                    draw.inverted_mask = i % 2 == 0;
                }
                frame.offscreens.push(OffscreenFrame {
                    id: id.clone(),
                    parent_offscreen_id: nested.then(|| "outer".into()),
                    enabled: true,
                    opacity: 0.7,
                    multiply_color: [0.9, 1., 0.8, 1.],
                    screen_color: [0.05, 0., 0.1, 1.],
                    blend_mode: if extended { 12 } else { 0 },
                    masks: if masked { vec!["mask".into()] } else { vec![] },
                    flags: if i % 2 == 0 { 8 } else { 0 },
                    ..Default::default()
                });
                frame.render_plan.extend([
                    RenderCommand::BeginOffscreen {
                        offscreen_id: id.clone(),
                    },
                    RenderCommand::DrawMesh {
                        mesh_id: draw.id.clone(),
                    },
                    RenderCommand::EndOffscreen { offscreen_id: id },
                ]);
                frame.drawables.push(draw);
            }
            if nested {
                frame.render_plan.push(RenderCommand::EndOffscreen {
                    offscreen_id: "outer".into(),
                });
            }
            let target = MetalTargetConfig {
                width: 2048,
                height: 2048,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let output = context.output_texture(target).unwrap();
            let mut renderer = MetalRenderer::new(&context, target).unwrap();
            renderer.sync_model(&frame, &catalog).unwrap();
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2::IDENTITY,
                    target_extent: Vec2::new(2048., 2048.),
                    mask_scale: 1.,
                })
                .unwrap();
            let mut cpu = Vec::new();
            let mut gpu = Vec::new();
            let mut last = Default::default();
            for iteration in 0..110 {
                metal::objc::rc::autoreleasepool(|| {
                    let command = context.queue().new_command_buffer();
                    let start = Instant::now();
                    last = renderer
                        .encode(command, &output, MetalOutputMode::Replace, &catalog)
                        .unwrap();
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    command.commit();
                    command.wait_until_completed();
                    assert_eq!(command.status(), metal::MTLCommandBufferStatus::Completed);
                    let begin: f64 = unsafe { msg_send![command, GPUStartTime] };
                    let end: f64 = unsafe { msg_send![command, GPUEndTime] };
                    if iteration >= 20 {
                        cpu.push(elapsed);
                        gpu.push((end - begin) * 1000.);
                    }
                });
            }
            cpu.sort_by(f64::total_cmp);
            gpu.sort_by(f64::total_cmp);
            std::fs::write(
                format!("{directory}/{name}.rgba"),
                read_rgba8(&output).unwrap(),
            )
            .unwrap();
            println!("{name}: cpu_ms={:.4} gpu_ms={:.4} gpu_p95_ms={:.4} color_bytes={} reservation_bytes={} draws={} passes={}",cpu[cpu.len()/2],gpu[gpu.len()/2],gpu[gpu.len()*95/100],last.color_attachment_bytes,last.attachment_budget_bytes,last.draw_calls,last.render_passes);
        }
    });
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This benchmark requires macOS Metal.");
}
