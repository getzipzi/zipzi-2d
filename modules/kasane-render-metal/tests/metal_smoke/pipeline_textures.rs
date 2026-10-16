use std::{collections::HashMap, sync::Arc};

use super::helpers::*;
use kasane_core::{
    evaluation::{Drawable, DrawableFrame},
    types::{Canvas, Vec2},
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal::MTLPixelFormat, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer,
    MetalTargetConfig, MetalTexture, MetalTextureCatalog,
};

#[test]
fn renders_pixels_through_native_metal() {
    const SIDE: u32 = 64;
    let context = MetalContext::new().unwrap();
    let source = context
        .upload_rgba8(1, 1, &[255, 40, 20, 255], &[])
        .unwrap();
    let textures = MetalTextureCatalog::new(HashMap::from([(
        "solid".to_owned(),
        MetalTexture {
            view: &source,
            width: 1,
            height: 1,
        },
    )]));
    let target = MetalTargetConfig {
        width: SIDE,
        height: SIDE,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
    let frame = DrawableFrame {
        canvas: Canvas::new(SIDE as f32, SIDE as f32, Vec2::new(0.0, 0.0), 1.0),
        drawables: vec![Drawable {
            id: "triangle".into(),
            texture_asset_id: "solid".into(),
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(SIDE as f32, 0.0),
                Vec2::new(0.0, -(SIDE as f32)),
            ],
            uvs: Arc::from([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &textures).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(SIDE as f32, SIDE as f32),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    renderer
        .encode(command, &output, MetalOutputMode::Replace, &textures)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    let pixels = read_rgba8(&output).unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * SIDE as usize + x) * 4..][..4];
    assert_eq!(pixel(8, 8), &[255, 40, 20, 255]);
    assert_eq!(pixel(60, 60), &[0, 0, 0, 0]);
}

#[test]
fn filtered_transparent_texels_do_not_tint_normal_or_clipped_edges() {
    let context = MetalContext::new().unwrap();
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let base = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    for transparent in [[0, 0, 0, 0], [255, 0, 255, 0]] {
        let mut rgba = vec![255, 255, 255, 255];
        rgba.extend(transparent);
        let source = context.upload_rgba8(2, 1, &rgba, &[]).unwrap();
        let catalog = MetalTextureCatalog::new(HashMap::from([
            (
                "edge".into(),
                MetalTexture {
                    view: &source,
                    width: 2,
                    height: 1,
                },
            ),
            (
                "base".into(),
                MetalTexture {
                    view: &base,
                    width: 1,
                    height: 1,
                },
            ),
        ]));
        for mode in [None, Some(0), Some(256)] {
            let mut edge = quad("edge", "edge");
            edge.raw_blend_mode = mode;
            let mut draws = vec![];
            if mode == Some(256) {
                draws.push(quad("base", "base"));
            }
            draws.push(edge);
            let frame = DrawableFrame {
                canvas: Canvas::new(64., 64., Vec2::new(0., 0.), 1.),
                drawables: draws,
                ..Default::default()
            };
            let output = context.output_texture(target).unwrap();
            let mut renderer = MetalRenderer::new(&context, target).unwrap();
            renderer.sync_model(&frame, &catalog).unwrap();
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2::IDENTITY,
                    target_extent: Vec2::new(64., 64.),
                    mask_scale: 1.,
                })
                .unwrap();
            let command = context.queue().new_command_buffer();
            renderer
                .encode(command, &output, MetalOutputMode::Replace, &catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            let pixels = read_rgba8(&output).unwrap();
            let actual = &pixels[(32 * 64 + 32) * 4..][..4];
            // At this UV, the opaque white texel contributes 31/64 coverage.
            let expected: [u8; 4] = if mode == Some(256) {
                [255, 124, 124, 255]
            } else {
                [124; 4]
            };
            for (a, e) in actual.iter().zip(expected) {
                assert!(
                    (i16::from(*a) - i16::from(e)).abs() <= 1,
                    "transparent={transparent:?}, mode={mode:?}: {actual:?}, expected={expected:?}"
                );
            }
        }
    }
}

#[test]
fn packed_mesh_buffers_preserve_vertex_and_index_offsets() {
    let mut left = quad("left", "red");
    for position in &mut left.positions {
        position.x *= 0.5;
    }
    // Only the upper/right triangle of the left half.
    left.indices = Arc::from([0, 1, 2]);
    let mut right = quad("right", "green");
    for position in &mut right.positions {
        position.x = 32. + position.x * 0.5;
    }
    let (pixels, stats) = fixture(&DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: vec![left, right],
        ..Default::default()
    });
    assert_eq!(stats.buffer_uploads, 2, "one vertex and one index upload");
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
    assert_eq!(pixel(24, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(8, 56), [0; 4]);
    assert_eq!(pixel(40, 56), [0, 255, 0, 255]);
    assert_eq!(pixel(56, 8), [0, 255, 0, 255]);
}

#[test]
fn empty_mesh_does_not_allocate_a_destination_snapshot() {
    let mut empty = quad("empty", "red");
    empty.indices = Arc::from([]);
    empty.raw_blend_mode = Some(1);
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![empty],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.destination_copies, 0);
    assert_eq!(center(&pixels), [0; 4]);
}

#[test]
fn rejects_array_textures_before_encoding_or_readback() {
    use kasane_render_metal::metal::{
        MTLStorageMode, MTLTextureType, MTLTextureUsage, TextureDescriptor,
    };
    let context = MetalContext::new().unwrap();
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let descriptor = TextureDescriptor::new();
    descriptor.set_texture_type(MTLTextureType::D2Array);
    descriptor.set_array_length(2);
    descriptor.set_width(64);
    descriptor.set_height(64);
    descriptor.set_pixel_format(target.format);
    descriptor.set_storage_mode(MTLStorageMode::Shared);
    descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    let array = context.device().new_texture(&descriptor);
    let output = context.output_texture(target).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "array".into(),
        MetalTexture {
            view: &array,
            width: 64,
            height: 64,
        },
    )]));
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        ..Default::default()
    };
    let viewport = ViewportConfig {
        transform: Affine2::IDENTITY,
        target_extent: Vec2::new(64.0, 64.0),
        mask_scale: 1.0,
    };
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer.update_view(viewport).unwrap();
    let command = context.queue().new_command_buffer();
    assert_eq!(
        renderer
            .encode(command, &array, MetalOutputMode::Replace, &catalog)
            .unwrap_err()
            .code,
        "INVALID_TARGET"
    );
    frame.drawables.push(quad("mesh", "array"));
    renderer.sync_model(&frame, &catalog).unwrap();
    assert_eq!(
        renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap_err()
            .code,
        "INVALID_TEXTURE"
    );
    assert_eq!(read_rgba8(&array).unwrap_err().code, "INVALID_READBACK");
}

#[test]
fn texture_rows_follow_the_shared_uv_orientation() {
    let context = MetalContext::new().unwrap();
    let mut rgba = vec![0u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let offset = (y * 64 + x) * 4;
            let color = match (x < 32, y < 32) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 255, 255],
            };
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }
    let source = context.upload_rgba8(64, 64, &rgba, &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "image".to_owned(),
        MetalTexture {
            view: &source,
            width: 64,
            height: 64,
        },
    )]));
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("mesh", "image")],
        ..Default::default()
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(64.0, 64.0),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    let pixels = read_rgba8(&output).unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
    assert_eq!(pixel(8, 8), &[0, 0, 255, 255]);
    assert_eq!(pixel(56, 8), &[255, 255, 255, 255]);
    assert_eq!(pixel(8, 56), &[255, 0, 0, 255]);
    assert_eq!(pixel(56, 56), &[0, 255, 0, 255]);
}
