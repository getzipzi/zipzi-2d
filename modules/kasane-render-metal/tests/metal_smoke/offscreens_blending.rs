use std::{collections::HashMap, sync::Arc};

use super::helpers::*;
use kasane_core::{
    evaluation::{DrawableFrame, OffscreenFrame, RenderCommand},
    types::{BlendMode, Vec2},
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal::MTLPixelFormat, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer,
    MetalTargetConfig, MetalTexture, MetalTextureCatalog,
};

#[test]
fn nested_offscreens_composite_into_main() {
    let frame = DrawableFrame {
        drawables: vec![quad("inner-mesh", "red")],
        offscreens: vec![
            OffscreenFrame {
                id: "outer".into(),
                enabled: true,
                opacity: 1.0,
                multiply_color: [1.0; 4],
                ..Default::default()
            },
            OffscreenFrame {
                id: "inner".into(),
                parent_offscreen_id: Some("outer".into()),
                enabled: true,
                opacity: 1.0,
                multiply_color: [1.0; 4],
                ..Default::default()
            },
        ],
        render_plan: vec![
            RenderCommand::BeginOffscreen {
                offscreen_id: "outer".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "inner".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "inner-mesh".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "inner".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "outer".into(),
            },
        ],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.active_surfaces, 2);
    assert_eq!(center(&pixels), [255, 0, 0, 255]);
}

#[test]
fn zoomed_sibling_offscreens_reuse_attachments_and_preserve_draw_order() {
    let mut frame = DrawableFrame {
        drawables: vec![quad("background", "red")],
        offscreens: vec![OffscreenFrame {
            id: "outer".into(),
            enabled: true,
            opacity: 0.5,
            multiply_color: [1.; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "outer".into(),
            },
        ],
        ..Default::default()
    };
    for i in 0..40 {
        let id = format!("group-{i}");
        let mut child = quad(&format!("child-{i}"), ["red", "green", "blue"][i % 3]);
        child.raw_blend_mode = Some(0);
        frame.offscreens.push(OffscreenFrame {
            id: id.clone(),
            parent_offscreen_id: Some("outer".into()),
            enabled: true,
            opacity: 0.5,
            blend_mode: 1,
            multiply_color: [1.; 4],
            ..Default::default()
        });
        frame.render_plan.extend([
            RenderCommand::BeginOffscreen {
                offscreen_id: id.clone(),
            },
            RenderCommand::DrawMesh {
                mesh_id: child.id.clone(),
            },
            RenderCommand::EndOffscreen { offscreen_id: id },
        ]);
        frame.drawables.push(child);
        // A parent draw between children catches lost Load actions and reordering.
        let mut parent = quad(&format!("parent-{i}"), "green");
        parent.opacity = 0.25;
        frame.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: parent.id.clone(),
        });
        frame.drawables.push(parent);
    }
    frame.render_plan.push(RenderCommand::EndOffscreen {
        offscreen_id: "outer".into(),
    });
    let expected = center(&fixture(&frame).0);
    let context = MetalContext::new().unwrap();
    let sources: Vec<_> = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .map(|rgba| context.upload_rgba8(1, 1, rgba, &[]).unwrap())
        .collect();
    let catalog = MetalTextureCatalog::new(
        ["red", "green", "blue"]
            .into_iter()
            .zip(&sources)
            .map(|(id, source)| {
                (
                    id.into(),
                    MetalTexture {
                        view: source,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect(),
    );
    let target = MetalTargetConfig {
        width: 2048,
        height: 2048,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(32., 0.),
                b: Vec2::new(0., -32.),
                origin: Vec2::default(),
            },
            target_extent: Vec2::new(2048., 2048.),
            mask_scale: 32.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    assert_eq!(stats.color_attachment_bytes, 3 * 2048 * 2048 * 4);
    command.commit();
    command.wait_until_completed();
    assert_eq!(stats.active_surfaces, 41);
    assert_eq!(stats.destination_copies, 80);
    let pixels = read_rgba8(&output).unwrap();
    for (x, y) in [(16, 16), (1024, 1024), (2032, 2032)] {
        assert_eq!(&pixels[(y * 2048 + x) * 4..][..4], &expected);
    }
}

#[test]
fn pooled_colors_preserve_parent_order_and_submitted_frames() {
    let context = MetalContext::new().unwrap();
    let sources: Vec<_> = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .map(|rgba| context.upload_rgba8(1, 1, rgba, &[]).unwrap())
        .collect();
    let catalog = MetalTextureCatalog::new(
        ["red", "green", "blue"]
            .into_iter()
            .zip(&sources)
            .map(|(id, source)| {
                (
                    id.into(),
                    MetalTexture {
                        view: source,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect(),
    );
    let mut draws = vec![
        quad("background", "red"),
        quad("first", "blue"),
        quad("middle", "green"),
        quad("second", "red"),
        quad("last", "blue"),
    ];
    for i in [0, 1, 3] {
        draws[i].raw_blend_mode = Some(0);
    }
    draws[2].opacity = 0.5;
    draws[4].opacity = 0.5;
    let mut frame = DrawableFrame {
        drawables: draws,
        offscreens: [
            ("outer", None, 0.5),
            ("first-group", Some("outer"), 1.),
            ("second-group", Some("outer"), 0.25),
        ]
        .into_iter()
        .map(|(id, parent, opacity)| OffscreenFrame {
            id: id.into(),
            parent_offscreen_id: parent.map(str::to_owned),
            enabled: true,
            opacity,
            multiply_color: [1.; 4],
            ..Default::default()
        })
        .collect(),
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "outer".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "first-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "first".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "first-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "middle".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "second-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "second".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "second-group".into(),
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
    let target = MetalTargetConfig {
        width: 128,
        height: 96,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let view = ViewportConfig {
        transform: Affine2 {
            origin: Vec2::new(16.25, 8.75),
            b: Vec2::new(0., -1.),
            ..Affine2::IDENTITY
        },
        target_extent: Vec2::new(128., 96.),
        mask_scale: 1.,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer.update_view(view).unwrap();
    let first_output = context.output_texture(target).unwrap();
    let first_command = context.queue().new_command_buffer();
    let first_stats = renderer
        .encode(
            first_command,
            &first_output,
            MetalOutputMode::Replace,
            &catalog,
        )
        .unwrap();
    assert_eq!(
        first_stats.color_attachment_bytes,
        (128 * 96 + 3 * 65 * 65) * 4
    );
    first_command.commit();
    // Submit a changed scene before waiting. Its pool must not overwrite the
    // previous command's attachments, even with a different presentation mode.
    for mesh in &mut frame.drawables {
        mesh.texture_asset_id = "red".into();
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer.update_view(view).unwrap();
    let second_output = context.output_texture(target).unwrap();
    let background = vec![255u8; 128 * 96 * 4];
    second_output.replace_region(
        kasane_render_metal::metal::MTLRegion::new_2d(0, 0, 128, 96),
        0,
        background.as_ptr().cast(),
        128 * 4,
    );
    let second_command = context.queue().new_command_buffer();
    let second_stats = renderer
        .encode(
            second_command,
            &second_output,
            MetalOutputMode::Composite,
            &catalog,
        )
        .unwrap();
    assert_eq!(
        second_stats.color_attachment_bytes,
        first_stats.color_attachment_bytes + 128 * 96 * 4
    );
    second_command.commit();
    second_command.wait_until_completed();
    let first = read_rgba8(&first_output).unwrap();
    let second = read_rgba8(&second_output).unwrap();
    let offset = (40 * 128 + 48) * 4;
    for (&actual, expected) in first[offset..][..4].iter().zip([144i16, 24, 88, 255]) {
        assert!(
            (i16::from(actual) - expected).abs() <= 2,
            "First frame pixel: {:?}",
            &first[offset..][..4]
        );
    }
    assert_eq!(&second[offset..][..4], &[255, 0, 0, 255]);
    assert_eq!(&first[..4], &[0; 4]);
    assert_eq!(
        &second[..4],
        &[255; 4],
        "Composite preserves the host background"
    );
}

#[test]
fn deeply_nested_offscreens_still_enforce_the_peak_attachment_budget() {
    let context = MetalContext::new().unwrap();
    let source = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &source,
            width: 1,
            height: 1,
        },
    )]));
    let mut frame = DrawableFrame {
        drawables: vec![quad("mesh", "red")],
        ..Default::default()
    };
    for i in 0..9 {
        let id = format!("nested-{i}");
        frame.offscreens.push(OffscreenFrame {
            id: id.clone(),
            parent_offscreen_id: (i > 0).then(|| format!("nested-{}", i - 1)),
            enabled: true,
            opacity: 1.,
            multiply_color: [1.; 4],
            ..Default::default()
        });
        frame
            .render_plan
            .push(RenderCommand::BeginOffscreen { offscreen_id: id });
    }
    frame.render_plan.push(RenderCommand::DrawMesh {
        mesh_id: "mesh".into(),
    });
    frame
        .render_plan
        .extend((0..9).rev().map(|i| RenderCommand::EndOffscreen {
            offscreen_id: format!("nested-{i}"),
        }));
    let target = MetalTargetConfig {
        width: 4096,
        height: 4096,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(64., 0.),
                b: Vec2::new(0., -64.),
                origin: Vec2::default(),
            },
            target_extent: Vec2::new(4096., 4096.),
            mask_scale: 64.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let rejected = context.queue().new_command_buffer();
    for mode in [MetalOutputMode::Replace, MetalOutputMode::Composite] {
        assert_eq!(
            renderer
                .encode(rejected, &output, mode, &catalog)
                .unwrap_err()
                .code,
            "OFFSCREEN_BUDGET_EXCEEDED"
        );
    }
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        ..target
    };
    renderer.resize(target).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2 {
                b: Vec2::new(0., -1.),
                ..Affine2::IDENTITY
            },
            target_extent: Vec2::new(64., 64.),
            mask_scale: 1.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    assert_eq!(
        stats.buffer_uploads, 2,
        "Budget rejection must preserve pending uploads"
    );
    command.commit();
    command.wait_until_completed();
    assert_eq!(center(&read_rgba8(&output).unwrap()), [255, 0, 0, 255]);
}

#[test]
fn fixed_and_destination_blends_see_prior_draws() {
    for (mode, expected) in [
        (BlendMode::Additive, [255, 255, 0, 255]),
        (BlendMode::Multiplicative, [0, 0, 0, 255]),
    ] {
        let mut green = quad("green", "green");
        green.blend_mode = mode;
        let frame = DrawableFrame {
            drawables: vec![quad("red", "red"), green],
            ..Default::default()
        };
        assert_eq!(center(&fixture(&frame).0), expected);
    }
    let mut green = quad("green", "green");
    green.raw_blend_mode = Some(1);
    let frame = DrawableFrame {
        drawables: vec![quad("red", "red"), green],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.destination_copies, 1);
    assert_eq!(center(&pixels), [255, 255, 0, 255]);
}

#[test]
fn offscreen_mask_and_destination_blend_use_parent_pixels() {
    let mut source = quad("source", "half");
    source.visible = false;
    source.opacity = 0.0;
    let frame = DrawableFrame {
        drawables: vec![source, quad("background", "red"), quad("child", "green")],
        offscreens: vec![OffscreenFrame {
            id: "group".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: 1,
            masks: vec!["source".into()],
            multiply_color: [1.0; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "source".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "child".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "group".into(),
            },
        ],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.active_surfaces, 1);
    assert_eq!(stats.masks, 1);
    assert_eq!(stats.destination_copies, 1);
    let pixel = center(&pixels);
    assert_eq!(pixel[0], 255);
    assert!((i16::from(pixel[1]) - 128).abs() <= 2, "{pixel:?}");
    assert_eq!(pixel[2], 0);
    assert_eq!(pixel[3], 255);
}

#[test]
fn view_changes_reuse_buffers_and_masks_but_source_changes_invalidate_them() {
    let context = MetalContext::new().unwrap();
    let source = context
        .upload_rgba8(1, 1, &[255, 255, 255, 128], &[])
        .unwrap();
    let color = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let mut catalog = MetalTextureCatalog::new(HashMap::from([
        (
            "half".into(),
            MetalTexture {
                view: &source,
                width: 1,
                height: 1,
            },
        ),
        (
            "red".into(),
            MetalTexture {
                view: &color,
                width: 1,
                height: 1,
            },
        ),
    ]));
    catalog.set_revision("half", 1);
    catalog.set_revision("red", 1);
    let mut mask = quad("mask", "half");
    mask.visible = false;
    let mut a = quad("a", "red");
    a.masks = vec!["mask".into()];
    let mut b = a.clone();
    b.id = "b".into();
    let mut frame = DrawableFrame {
        drawables: vec![mask, a, b],
        ..Default::default()
    };
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let draw =
        |renderer: &mut MetalRenderer, catalog: &MetalTextureCatalog<'_>, pan: f32, scale: f64| {
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2 {
                        origin: Vec2::new(pan, 0.),
                        b: Vec2::new(0., -1.),
                        ..Affine2::IDENTITY
                    },
                    target_extent: Vec2::new(64., 64.),
                    mask_scale: scale,
                })
                .unwrap();
            let output = context.output_texture(target).unwrap();
            let command = context.queue().new_command_buffer();
            let stats = renderer
                .encode(command, &output, MetalOutputMode::Replace, catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            assert_eq!(
                command.status(),
                kasane_render_metal::metal::MTLCommandBufferStatus::Completed
            );
            (read_rgba8(&output).unwrap(), stats)
        };
    renderer.sync_model(&frame, &catalog).unwrap();
    let (first, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!(stats.masks, 1, "consumers share the same raw mask");
    assert_eq!(stats.render_passes, 2, "one mask pass and one color pass");
    let (_, stats) = draw(&mut renderer, &catalog, 4., 1.);
    assert_eq!(
        (stats.masks, stats.buffer_uploads, stats.render_passes),
        (0, 0, 1)
    );
    assert_eq!(stats.mask_cache_hits, 2);
    // Re-submitting an unchanged model also preserves immutable geometry.
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 2.);
    assert_eq!(stats.masks, 1, "zoom changes mask density");
    assert_eq!(stats.buffer_uploads, 0);
    let (_, stats) = draw(&mut renderer, &catalog, 0., 1.1);
    assert_eq!(
        stats.masks, 0,
        "nearby zoom levels share rounded-up density"
    );
    // A source edit changes coverage and uploads only its vertex buffer.
    for p in &mut frame.drawables[0].positions {
        p.x += 64.;
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    let (moved, stats) = draw(&mut renderer, &catalog, 0., 2.);
    assert_eq!(stats.buffer_uploads, 1);
    assert_eq!(stats.masks, 1);
    assert_eq!(center(&moved), [0; 4]);
    for p in &mut frame.drawables[0].positions {
        p.x -= 64.;
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    let _ = draw(&mut renderer, &catalog, 0., 1.);
    // A new Arc with the same indices keeps the packed index buffer. A real
    // topology edit uploads just that buffer and invalidates dependent masks.
    frame.drawables[0].indices = Arc::from([0, 1, 2, 0, 2, 3]);
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!(stats.buffer_uploads, 0);
    frame.drawables[0].indices = Arc::from([0, 1, 2]);
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!((stats.buffer_uploads, stats.masks), (1, 1));
    frame.drawables[0].indices = Arc::from([0, 1, 2, 0, 2, 3]);
    renderer.sync_model(&frame, &catalog).unwrap();
    let _ = draw(&mut renderer, &catalog, 0., 1.);
    source.replace_region(
        kasane_render_metal::metal::MTLRegion::new_2d(0, 0, 1, 1),
        0,
        [255u8, 255, 255, 255].as_ptr().cast(),
        4,
    );
    catalog.set_revision("half", 2);
    let (opaque, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!(stats.masks, 1);
    assert_eq!(center(&opaque), [255, 0, 0, 255]);
    assert!(center(&first)[3] < 255);
    // Hosts without revision tracking must never get a stale cached mask.
    let unversioned = MetalTextureCatalog::new(HashMap::from([
        (
            "half".into(),
            MetalTexture {
                view: &source,
                width: 1,
                height: 1,
            },
        ),
        (
            "red".into(),
            MetalTexture {
                view: &color,
                width: 1,
                height: 1,
            },
        ),
    ]));
    for _ in 0..2 {
        assert_eq!(draw(&mut renderer, &unversioned, 0., 1.).1.masks, 1);
    }
    // Budget the shared allocation once, not once per consumer. The old
    // estimate exceeded 512 MiB here despite using a single ~4 MiB mask.
    for index in 0..200 {
        let mut consumer = frame.drawables[1].clone();
        consumer.id = format!("consumer-{index}");
        frame.drawables.push(consumer);
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 16.);
    assert_eq!(stats.masks, 1);
    assert_eq!(stats.mask_cache_hits, 201);
}

#[test]
fn edits_do_not_overwrite_buffers_referenced_by_submitted_frames() {
    let context = MetalContext::new().unwrap();
    let source = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &source,
            width: 1,
            height: 1,
        },
    )]));
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let mut frame = DrawableFrame {
        drawables: vec![quad("mesh", "red")],
        ..Default::default()
    };
    let mut outputs = Vec::new();
    for _ in 0..2 {
        renderer.sync_model(&frame, &catalog).unwrap();
        renderer
            .update_view(ViewportConfig {
                transform: Affine2 {
                    b: Vec2::new(0., -1.),
                    ..Affine2::IDENTITY
                },
                target_extent: Vec2::new(64., 64.),
                mask_scale: 1.,
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let command = context.queue().new_command_buffer();
        renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap();
        command.commit();
        outputs.push(output);
        for p in &mut frame.drawables[0].positions {
            p.x += 64.;
        }
    }
    let barrier = context.queue().new_command_buffer();
    barrier.commit();
    barrier.wait_until_completed();
    assert_eq!(center(&read_rgba8(&outputs[0]).unwrap()), [255, 0, 0, 255]);
    assert_eq!(center(&read_rgba8(&outputs[1]).unwrap()), [0; 4]);
}

#[test]
fn unavailable_masked_geometry_needs_no_texture_and_does_not_escape_a_disabled_composite() {
    let mut missing = quad("missing", "not-loaded");
    missing.renderable = false;
    let mut dependent = quad("dependent", "also-not-loaded");
    dependent.masks = vec!["missing".into()];
    dependent.renderable = false;
    let frame = DrawableFrame {
        drawables: vec![missing, dependent],
        offscreens: vec![OffscreenFrame {
            id: "pending-composite".into(),
            masks: vec!["missing".into()],
            renderable: false,
            enabled: true,
            opacity: 1.,
            ..Default::default()
        }],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(center(&pixels), [0, 0, 0, 0]);
    assert_eq!(stats.active_surfaces, 0);
}
