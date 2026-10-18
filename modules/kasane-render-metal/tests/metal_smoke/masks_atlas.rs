use std::{collections::HashMap, sync::Arc};

use super::helpers::*;
use kasane_core::{
    evaluation::{DrawableFrame, OffscreenFrame, RenderCommand},
    types::{Canvas, Vec2},
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal::MTLPixelFormat, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer,
    MetalTargetConfig, MetalTexture, MetalTextureCatalog,
};

#[test]
fn cropped_masks_follow_zoom_pan_and_resize_including_empty_inverted_masks() {
    let context = MetalContext::new().unwrap();
    let texture = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let mut catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &texture,
            width: 1,
            height: 1,
        },
    )]));
    catalog.set_revision("red", 1);
    for (count, inverted, offscreen) in [
        (1, false, false),
        (1, true, false),
        (40, false, false),
        (40, true, false),
        (1, false, true),
    ] {
        let mut frame = DrawableFrame {
            model_space: (Canvas::new(4096., 4096., Vec2::default(), 1.)).into(),
            ..Default::default()
        };
        for i in 0..count {
            let mut source = quad(&format!("mask-{i}"), "red");
            source.visible = false;
            for point in &mut source.positions {
                point.x *= 32.;
                point.y *= 64.;
            }
            let mut consumer = quad(&format!("mesh-{i}"), "red");
            for point in &mut consumer.positions {
                point.x *= 64.;
                point.y *= 64.;
            }
            consumer.masks = vec![source.id.clone()];
            consumer.inverted_mask = inverted;
            frame.drawables.extend([source, consumer]);
        }
        if offscreen {
            frame.offscreens = vec![
                OffscreenFrame {
                    id: "outer".into(),
                    enabled: true,
                    opacity: 1.,
                    multiply_color: [1.; 4],
                    masks: vec!["mask-0".into()],
                    ..Default::default()
                },
                OffscreenFrame {
                    id: "inner".into(),
                    parent_offscreen_id: Some("outer".into()),
                    enabled: true,
                    opacity: 1.,
                    multiply_color: [1.; 4],
                    masks: vec!["mask-0".into()],
                    ..Default::default()
                },
            ];
            frame.render_plan = vec![
                RenderCommand::DrawMesh {
                    mesh_id: "mask-0".into(),
                },
                RenderCommand::BeginOffscreen {
                    offscreen_id: "outer".into(),
                },
                RenderCommand::BeginOffscreen {
                    offscreen_id: "inner".into(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "mesh-0".into(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "inner".into(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "outer".into(),
                },
            ];
        }
        let mut target = MetalTargetConfig {
            width: 64,
            height: 64,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalRenderer::new(&context, target).unwrap();
        renderer.sync_model(&frame, &catalog).unwrap();
        for (side, center_x) in [
            (64, 2048.),
            (64, 2048.125),
            (64, 1024.),
            (64, 3072.),
            (128, 2048.),
            (64, 2048.),
        ] {
            target.width = side;
            target.height = side;
            renderer.resize(target).unwrap();
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2 {
                        a: Vec2::new(32., 0.),
                        b: Vec2::new(0., 32.),
                        origin: Vec2::new(
                            side as f32 / 2. - center_x * 32.,
                            side as f32 / 2. - 2048. * 32.,
                        ),
                    },
                    target_extent: Vec2::new(side as f32, side as f32),
                    mask_scale: 32.,
                })
                .unwrap();
            let output = context.output_texture(target).unwrap();
            let command = context.queue().new_command_buffer();
            let stats = renderer
                .encode(command, &output, MetalOutputMode::Replace, &catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            assert_eq!(
                command.status(),
                kasane_render_metal::metal::MTLCommandBufferStatus::Completed
            );
            assert!(
                stats.attachment_budget_bytes < 128 * 1024 * 1024,
                "Only the visible mask region should be allocated: {stats:?}"
            );
            if center_x == 2048.125 {
                assert_eq!(
                    stats.masks, 0,
                    "A small pan within the snapped crop reuses masks"
                );
            }
            let pixels = read_rgba8(&output).unwrap();
            for x in [0, side / 4, side * 3 / 4, side - 1] {
                let model_x = center_x + (x as f32 + 0.5 - side as f32 / 2.) / 32.;
                let covered = (model_x < 2048.) != inverted;
                let expected = if covered { [255, 0, 0, 255] } else { [0; 4] };
                let offset = ((side / 2 * side + x) * 4) as usize;
                assert_eq!(
                    &pixels[offset..offset + 4],
                    &expected,
                    "count={count}, inverted={inverted}, center={center_x}, side={side}, x={x}"
                );
            }
        }
    }
}

#[test]
fn cropped_mask_sampling_stays_stable_across_integer_pixel_pans() {
    let context = MetalContext::new().unwrap();
    let texture = context.upload_rgba8(1, 1, &[255; 4], &[]).unwrap();
    let mut catalog = MetalTextureCatalog::new(HashMap::from([(
        "solid".into(),
        MetalTexture {
            view: &texture,
            width: 1,
            height: 1,
        },
    )]));
    catalog.set_revision("solid", 1);
    let mut source = quad("mask", "solid");
    source.visible = false;
    source.indices = Arc::from([0, 1, 2]);
    let mut consumer = quad("mesh", "solid");
    consumer.masks = vec![source.id.clone()];
    let frame = DrawableFrame {
        model_space: (Canvas::new(64., 64., Vec2::default(), 1.)).into(),
        drawables: vec![source, consumer],
        ..Default::default()
    };
    let target = MetalTargetConfig {
        width: 256,
        height: 256,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    let capture = |renderer: &mut MetalRenderer, zoom: f32, pan: f32| {
        renderer
            .update_view(ViewportConfig {
                transform: Affine2 {
                    a: Vec2::new(zoom, 0.),
                    b: Vec2::new(0., zoom),
                    origin: Vec2::new(128. - 32. * zoom + pan, 128. - 32. * zoom),
                },
                target_extent: Vec2::new(256., 256.),
                mask_scale: f64::from(zoom),
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let command = context.queue().new_command_buffer();
        renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap();
        command.commit();
        command.wait_until_completed();
        assert_eq!(
            command.status(),
            kasane_render_metal::metal::MTLCommandBufferStatus::Completed
        );
        read_rgba8(&output).unwrap()
    };
    for zoom in [20., 24.] {
        let before = capture(&mut renderer, zoom, 0.);
        assert!(before
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0 && p[3] < 255));
        let after = capture(&mut renderer, zoom, 64.);
        let mut worst_difference = 0;
        for y in 0..256 {
            for x in 0..192 {
                let a = before[(y * 256 + x) * 4 + 3];
                let b = after[(y * 256 + x + 64) * 4 + 3];
                worst_difference = worst_difference.max(a.abs_diff(b));
            }
        }
        assert!(
            worst_difference <= 1,
            "At zoom {zoom}, a pan changed mask coverage by {worst_difference}/255"
        );
        assert_eq!(before, capture(&mut renderer, zoom, 0.));
    }
}

#[test]
fn visible_masks_lower_resolution_when_shared_budget_is_exhausted() {
    let context = MetalContext::new().unwrap();
    let texture = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &texture,
            width: 1,
            height: 1,
        },
    )]));
    let mut frame = DrawableFrame {
        model_space: (Canvas::new(8000., 8000., Vec2::default(), 1.)).into(),
        ..Default::default()
    };
    for i in 0..8 {
        let mut source = quad(&format!("mask-{i}"), "red");
        for point in &mut source.positions {
            point.x *= 125.;
            point.y *= 125.;
        }
        source.visible = false;
        let mut consumer = source.clone();
        consumer.id = format!("mesh-{i}");
        consumer.visible = true;
        consumer.masks = vec![source.id.clone()];
        frame.drawables.extend([source, consumer]);
    }
    let target = MetalTargetConfig {
        width: 4096,
        height: 4096,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(4096., 4096.),
            mask_scale: 1.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    assert_eq!(stats.mask_resolution_limit, 2048);
    assert!(stats.attachment_budget_bytes <= kasane_render::OFFSCREEN_BUDGET_BYTES as u64);
    command.commit();
    command.wait_until_completed();
    assert_eq!(
        command.status(),
        kasane_render_metal::metal::MTLCommandBufferStatus::Completed
    );
    let pixels = read_rgba8(&output).unwrap();
    for (x, y) in [(0, 0), (4095, 0), (2048, 2048), (0, 4095), (4095, 4095)] {
        let offset = (y * 4096 + x) * 4;
        assert_eq!(&pixels[offset..offset + 4], &[255, 0, 0, 255]);
    }
}

#[test]
fn raw_mask_and_inverted_mask_use_source_alpha() {
    let mut source = quad("source", "half");
    source.visible = false;
    source.opacity = 0.0;
    let mut target = quad("target", "red");
    target.masks = vec!["source".into()];
    let mut frame = DrawableFrame {
        model_space: (Canvas::new(64.0, 64.0, Vec2::default(), 1.0)).into(),
        drawables: vec![source, target],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.masks, 1);
    assert!((i16::from(center(&pixels)[3]) - 128).abs() <= 2);
    frame.drawables[1].inverted_mask = true;
    let (pixels, _) = fixture(&frame);
    assert!((i16::from(center(&pixels)[3]) - 127).abs() <= 2);
}

#[test]
fn duplicate_mask_sources_do_not_accumulate_alpha() {
    let mut source = quad("source", "half");
    source.visible = false;
    let mut target = quad("target", "red");
    target.masks = vec!["source".into()];
    let mut frame = DrawableFrame {
        model_space: (Canvas::new(64.0, 64.0, Vec2::default(), 1.0)).into(),
        drawables: vec![source, target],
        ..Default::default()
    };
    let expected = fixture(&frame).0;
    frame.drawables[1].masks.push("source".into());
    assert_eq!(center(&fixture(&frame).0), center(&expected));
}

#[test]
fn mask_atlas_matches_individual_masks_without_neighbor_bleed() {
    let mut frame = DrawableFrame {
        model_space: (Canvas::new(64., 64., Vec2::default(), 1.)).into(),
        ..Default::default()
    };
    for i in 0..40 {
        let mut source = quad(&format!("mask-{i}"), "half");
        let x = (i % 8) as f32 * 8. + 0.25;
        let y = (i / 8) as f32 * 12. + 0.125;
        for p in &mut source.positions {
            p.x = p.x / 16. + x;
            p.y = p.y / 8. - y;
        }
        source.visible = false;
        let mut consumer = source.clone();
        consumer.id = format!("consumer-{i}");
        consumer.texture_asset_id = "red".into();
        consumer.visible = true;
        consumer.masks = vec![source.id.clone()];
        consumer.inverted_mask = i % 2 == 0;
        // Cover both the mask edge and the transparent padding.
        consumer.positions[1].x += 3.;
        consumer.positions[2].x += 3.;
        frame.drawables.extend([source, consumer]);
    }
    let (atlas, stats) = fixture(&frame);
    assert_eq!(stats.masks, 40);
    assert_eq!(stats.render_passes, 2, "all masks share one atlas pass");
    let mut expected = vec![0u8; atlas.len()];
    for chunk in frame.drawables.chunks(40) {
        let (pixels, stats) = fixture(&DrawableFrame {
            model_space: (frame.model_space).into(),
            drawables: chunk.to_vec(),
            ..Default::default()
        });
        assert_eq!(stats.render_passes, 21);
        for (dst, src) in expected.iter_mut().zip(pixels) {
            *dst = (*dst).max(src); // These grid cells never overlap.
        }
    }
    assert!(atlas.iter().any(|&v| v != 0));
    assert_eq!(atlas, expected);
}

#[test]
fn rejected_encoding_preserves_mask_cache_and_pending_upload_stats() {
    for count in [1, 40] {
        let context = MetalContext::new().unwrap();
        let texture = context.upload_rgba8(1, 1, &[255, 0, 0, 128], &[]).unwrap();
        let mut catalog = MetalTextureCatalog::new(HashMap::from([(
            "red".into(),
            MetalTexture {
                view: &texture,
                width: 1,
                height: 1,
            },
        )]));
        catalog.set_revision("red", 1);
        let target = MetalTargetConfig {
            width: 64,
            height: 64,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalRenderer::new(&context, target).unwrap();
        let mut frame = DrawableFrame {
            model_space: (Canvas::new(64., 64., Vec2::default(), 1.)).into(),
            ..Default::default()
        };
        for i in 0..count {
            let mut source = quad(&format!("mask-{i}"), "red");
            source.visible = false;
            let mut consumer = quad(&format!("consumer-{i}"), "red");
            consumer.masks = vec![source.id.clone()];
            frame.drawables.extend([source, consumer]);
        }
        renderer.sync_model(&frame, &catalog).unwrap();
        renderer
            .update_view(ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64., 64.),
                mask_scale: 1.,
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let invalid_output = context
            .output_texture(MetalTargetConfig {
                width: 32,
                ..target
            })
            .unwrap();
        let encode = |renderer: &mut MetalRenderer| {
            let command = context.queue().new_command_buffer();
            let stats = renderer
                .encode(command, &output, MetalOutputMode::Replace, &catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            stats
        };
        let command = context.queue().new_command_buffer();
        assert_eq!(
            renderer
                .encode(command, &invalid_output, MetalOutputMode::Replace, &catalog)
                .unwrap_err()
                .code,
            "INVALID_TARGET"
        );
        let stats = encode(&mut renderer);
        assert_eq!(stats.buffer_uploads, 2);
        assert_eq!(stats.masks, count);
        let expected = read_rgba8(&output).unwrap();

        let missing = MetalTextureCatalog::new(HashMap::new());
        let command = context.queue().new_command_buffer();
        assert_eq!(
            renderer
                .encode(command, &output, MetalOutputMode::Replace, &missing)
                .unwrap_err()
                .code,
            "MISSING_TEXTURE"
        );
        let stats = encode(&mut renderer);
        assert_eq!(stats.masks, 0);
        assert_eq!(stats.mask_cache_hits, count);
        assert_eq!(stats.buffer_uploads, 0);
        assert_eq!(read_rgba8(&output).unwrap(), expected);
    }
}

#[test]
fn atlas_cache_edits_and_transition_preserve_submitted_frames() {
    let context = MetalContext::new().unwrap();
    let texture = context.upload_rgba8(1, 1, &[255, 0, 0, 128], &[]).unwrap();
    let mut catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &texture,
            width: 1,
            height: 1,
        },
    )]));
    catalog.set_revision("red", 1);
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let mut frame = DrawableFrame {
        model_space: (Canvas::new(64., 64., Vec2::default(), 1.)).into(),
        ..Default::default()
    };
    for i in 0..40 {
        let mut source = quad(&format!("mask-{i}"), "red");
        source.visible = false;
        for p in &mut source.positions {
            p.x *= 0.5;
        }
        let mut consumer = quad(&format!("consumer-{i}"), "red");
        consumer.masks = vec![source.id.clone()];
        for p in &mut consumer.positions {
            p.x = p.x / 8. + (i % 8) as f32 * 8.;
            p.y = p.y / 8. - (i / 8) as f32 * 12.;
        }
        frame.drawables.extend([source, consumer]);
    }
    let mut outputs = Vec::new();
    for step in 0..5 {
        if step == 2 {
            for source in frame.drawables.iter_mut().step_by(2) {
                for p in &mut source.positions {
                    p.x += 32.;
                }
            }
        }
        if step == 3 {
            catalog.set_revision("red", 2);
        }
        if step == 4 {
            // Shrink below the atlas threshold and edit a remaining source.
            // Its cached atlas texture must not become a standalone mask.
            frame.drawables.truncate(40);
            for p in &mut frame.drawables[0].positions {
                p.x -= 32.;
            }
        }
        renderer.sync_model(&frame, &catalog).unwrap();
        renderer
            .update_view(ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64., 64.),
                mask_scale: 1.,
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let command = context.queue().new_command_buffer();
        let stats = renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap();
        if step == 1 {
            assert_eq!(stats.masks, 0);
        }
        if step == 2 || step == 3 {
            assert_eq!(stats.masks, 40);
        }
        command.commit();
        outputs.push(output);
    }
    let barrier = context.queue().new_command_buffer();
    barrier.commit();
    barrier.wait_until_completed();
    let pixels: Vec<_> = outputs
        .iter()
        .map(|output| read_rgba8(output).unwrap())
        .collect();
    let pixel = |step: usize, x: usize| &pixels[step][(4 * 64 + x) * 4..][..4];
    assert_eq!(pixel(0, 4), [64, 0, 0, 64]);
    assert_eq!(pixel(0, 52), [0; 4]);
    assert_eq!(pixels[0], pixels[1]);
    assert_eq!(pixel(2, 4), [0; 4]);
    assert_eq!(pixel(2, 52), [64, 0, 0, 64]);
    assert_eq!(pixels[2], pixels[3]);
    assert_eq!(pixel(4, 4), [64, 0, 0, 64]);
}
