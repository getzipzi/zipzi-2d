//! Compare cropped groups with transparent geometry that forces full-view surfaces.
use super::helpers::quad;
use kasane_core::{
    evaluation::{DrawableFrame, OffscreenFrame, RenderCommand},
    BlendMode, Vec2,
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer, MetalTargetConfig,
    MetalTexture, MetalTextureCatalog,
};

#[test]
fn cropped_nested_groups_match_full_surfaces_for_every_blend_and_mask() {
    metal::objc::rc::autoreleasepool(|| {
        let context = MetalContext::new().unwrap();
        let art = context
            .upload_rgba8(
                2,
                2,
                &[
                    230, 40, 110, 200, 30, 220, 70, 120, 60, 100, 240, 0, 220, 200, 20, 240,
                ],
                &[],
            )
            .unwrap();
        let mask = context
            .upload_rgba8(
                2,
                2,
                &[
                    255, 255, 255, 255, 255, 255, 255, 0, 255, 255, 255, 100, 255, 255, 255, 210,
                ],
                &[],
            )
            .unwrap();
        let clear = context.upload_rgba8(1, 1, &[0; 4], &[]).unwrap();
        let mut catalog = MetalTextureCatalog::new(std::collections::HashMap::from([
            (
                "art".into(),
                MetalTexture {
                    view: &art,
                    width: 2,
                    height: 2,
                },
            ),
            (
                "mask".into(),
                MetalTexture {
                    view: &mask,
                    width: 2,
                    height: 2,
                },
            ),
            (
                "clear".into(),
                MetalTexture {
                    view: &clear,
                    width: 1,
                    height: 1,
                },
            ),
        ]));
        for name in ["art", "mask", "clear"] {
            catalog.set_revision(name, 1);
        }
        let config = MetalTargetConfig {
            width: 128,
            height: 96,
            format: metal::MTLPixelFormat::RGBA8Unorm,
        };
        let mut cropped = MetalRenderer::new(&context, config).unwrap();
        let mut reference = MetalRenderer::new(&context, config).unwrap();
        let mut compared = 0;
        for alpha in 0..5 {
            for color in 0..18 {
                for (case, transform) in [
                    Affine2::IDENTITY,
                    Affine2 {
                        a: Vec2::new(1.7, 0.),
                        b: Vec2::new(0., 1.7),
                        origin: Vec2::new(-37.25, -21.75),
                    },
                    Affine2 {
                        a: Vec2::new(0.83, 0.27),
                        b: Vec2::new(-0.19, 1.13),
                        origin: Vec2::new(23.375, -11.125),
                    },
                    Affine2 {
                        a: Vec2::new(-1.1, 0.),
                        b: Vec2::new(0., 0.9),
                        origin: Vec2::new(139.25, 13.5),
                    },
                ]
                .into_iter()
                .enumerate()
                {
                    let mode = color | (alpha << 8);
                    let mut background = quad("background", "art");
                    for p in &mut background.positions {
                        p.x *= 2.;
                        p.y *= 1.5;
                    }
                    background.opacity = 0.7;
                    let mut source = quad("mask-source", "mask");
                    source.visible = false;
                    for p in &mut source.positions {
                        p.x += 26.5;
                        p.y -= 12.25;
                    }
                    let mut first = quad("first", "art");
                    for p in &mut first.positions {
                        p.x = p.x * 0.35 + 17.375;
                        p.y = p.y * 0.4 - 20.125;
                    }
                    first.raw_blend_mode = Some(mode);
                    first.masks = vec!["mask-source".into()];
                    first.inverted_mask = case % 2 == 0;
                    let mut second = quad("second", "art");
                    for p in &mut second.positions {
                        p.x = p.x * 0.6 + 29.625;
                        p.y = p.y * 0.55 - 26.375;
                    }
                    second.opacity = 0.73;
                    second.multiply_color = [0.8, 0.9, 0.7, 1.];
                    second.screen_color = [0.1, 0.03, 0.15, 1.];
                    second.blend_mode = [
                        BlendMode::Normal,
                        BlendMode::Additive,
                        BlendMode::Multiplicative,
                    ][case % 3];
                    let mut last = quad("last", "art");
                    for p in &mut last.positions {
                        p.x = p.x * 0.27 + 62.875;
                        p.y = p.y * 0.2 - 21.625;
                    }
                    last.opacity = 0.45;
                    let group = |id: &str, parent: Option<&str>, blend_mode| OffscreenFrame {
                        id: id.into(),
                        parent_offscreen_id: parent.map(str::to_owned),
                        enabled: true,
                        opacity: 0.67,
                        multiply_color: [0.9, 0.7, 0.8, 1.],
                        screen_color: [0.05, 0.1, 0.02, 1.],
                        blend_mode,
                        masks: vec!["mask-source".into()],
                        flags: if case % 2 == 0 { 8 } else { 0 },
                        ..Default::default()
                    };
                    let mut frame = DrawableFrame {
                        drawables: vec![background, source, first, second, last],
                        // Child deliberately precedes parent in storage.
                        offscreens: vec![
                            group("inner", Some("outer"), mode),
                            group("outer", None, mode),
                        ],
                        render_plan: vec![
                            RenderCommand::DrawMesh {
                                mesh_id: "background".into(),
                            },
                            RenderCommand::DrawMesh {
                                mesh_id: "mask-source".into(),
                            },
                            RenderCommand::BeginOffscreen {
                                offscreen_id: "outer".into(),
                            },
                            RenderCommand::DrawMesh {
                                mesh_id: "first".into(),
                            },
                            RenderCommand::BeginOffscreen {
                                offscreen_id: "inner".into(),
                            },
                            RenderCommand::DrawMesh {
                                mesh_id: "second".into(),
                            },
                            RenderCommand::EndOffscreen {
                                offscreen_id: "inner".into(),
                            },
                            RenderCommand::DrawMesh {
                                mesh_id: "last".into(),
                            },
                            RenderCommand::EndOffscreen {
                                offscreen_id: "outer".into(),
                            },
                        ],
                        ..Default::default()
                    };
                    cropped.sync_model(&frame, &catalog).unwrap();
                    // Normal transparent padding changes neither color nor alpha,
                    // but expands both groups to cover the entire output.
                    for id in ["outer", "inner"] {
                        let mut pad = quad(&format!("pad-{id}"), "clear");
                        for p in &mut pad.positions {
                            p.x = p.x * 32. - 1024.;
                            p.y = p.y * 32. + 1024.;
                        }
                        let start = frame.render_plan.iter().position(|command| matches!(command, RenderCommand::BeginOffscreen { offscreen_id } if offscreen_id == id)).unwrap();
                        frame.render_plan.insert(
                            start + 1,
                            RenderCommand::DrawMesh {
                                mesh_id: pad.id.clone(),
                            },
                        );
                        frame.drawables.push(pad);
                    }
                    reference.sync_model(&frame, &catalog).unwrap();
                    let view = ViewportConfig {
                        transform,
                        target_extent: Vec2::new(128., 96.),
                        mask_scale: 1.7,
                    };
                    cropped.update_view(view).unwrap();
                    reference.update_view(view).unwrap();
                    let mut images = Vec::new();
                    for renderer in [&mut cropped, &mut reference] {
                        let output = context.output_texture(config).unwrap();
                        let command = context.queue().new_command_buffer();
                        let stats = renderer
                            .encode(command, &output, MetalOutputMode::Replace, &catalog)
                            .unwrap();
                        assert!(
                            stats.color_attachment_bytes + 128 * 96 * 4
                                <= stats.attachment_budget_bytes
                        );
                        command.commit();
                        command.wait_until_completed();
                        assert_eq!(command.status(), metal::MTLCommandBufferStatus::Completed);
                        images.push(read_rgba8(&output).unwrap());
                    }
                    let maximum = images[0]
                        .iter()
                        .zip(&images[1])
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    assert!(
                        maximum <= 1,
                        "mode={mode}, transform={case}, max channel difference={maximum}"
                    );
                    compared += 1;
                }
            }
        }
        assert_eq!(compared, 360);
    });
}

#[test]
fn color_storage_shrinks_for_small_groups_and_falls_back_for_fragmented_sizes() {
    metal::objc::rc::autoreleasepool(|| {
        let context = MetalContext::new().unwrap();
        let texture = context
            .upload_rgba8(1, 1, &[200, 100, 50, 255], &[])
            .unwrap();
        let catalog = MetalTextureCatalog::new(std::collections::HashMap::from([(
            "art".into(),
            MetalTexture {
                view: &texture,
                width: 1,
                height: 1,
            },
        )]));
        let config = MetalTargetConfig {
            width: 64,
            height: 64,
            format: metal::MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalRenderer::new(&context, config).unwrap();
        for fragmented in [false, true] {
            let mut frame = DrawableFrame {
                drawables: vec![quad("background", "art")],
                render_plan: vec![RenderCommand::DrawMesh {
                    mesh_id: "background".into(),
                }],
                ..Default::default()
            };
            for i in 0..10 {
                let id = format!("g{i}");
                let mut d = quad(&format!("m{i}"), "art");
                let size = if fragmented { 35. + i as f32 * 3. } else { 8. };
                for p in &mut d.positions {
                    p.x *= size / 64.;
                    p.y *= size / 64.;
                }
                frame.offscreens.push(OffscreenFrame {
                    id: id.clone(),
                    enabled: true,
                    opacity: 0.5,
                    multiply_color: [1.; 4],
                    ..Default::default()
                });
                frame.render_plan.extend([
                    RenderCommand::BeginOffscreen {
                        offscreen_id: id.clone(),
                    },
                    RenderCommand::DrawMesh {
                        mesh_id: d.id.clone(),
                    },
                    RenderCommand::EndOffscreen { offscreen_id: id },
                ]);
                frame.drawables.push(d);
            }
            renderer.sync_model(&frame, &catalog).unwrap();
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2::IDENTITY,
                    target_extent: Vec2::new(64., 64.),
                    mask_scale: 1.,
                })
                .unwrap();
            for mode in [MetalOutputMode::Replace, MetalOutputMode::Composite] {
                let output = context.output_texture(config).unwrap();
                let command = context.queue().new_command_buffer();
                let stats = renderer.encode(command, &output, mode, &catalog).unwrap();
                let expected_surface = if fragmented || cfg!(feature = "benchmark-global-surfaces")
                {
                    64 * 64 * 4
                } else {
                    8 * 8 * 4
                };
                let intermediate = if mode == MetalOutputMode::Composite {
                    64 * 64 * 4
                } else {
                    0
                };
                assert_eq!(
                    stats.color_attachment_bytes,
                    expected_surface + intermediate
                );
                assert_eq!(
                    stats.attachment_budget_bytes,
                    stats.color_attachment_bytes + 64 * 64 * 4
                );
                command.commit();
                command.wait_until_completed();
                assert_eq!(command.status(), metal::MTLCommandBufferStatus::Completed);
            }
        }
    });
}
