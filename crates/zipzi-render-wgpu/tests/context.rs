use std::{collections::HashMap, sync::Arc};
use zipzi_core::{
    evaluation::{Drawable, DrawableFrame},
    Vec2,
};
use zipzi_render::{Affine2, ViewportConfig};
use zipzi_render_wgpu::*;

#[test]
fn validates_uploads_and_reads_bgra_with_padded_rows() {
    let c = WgpuContext::new().unwrap();
    assert!(c.upload_rgba8(0, 1, &[], &[]).is_err());
    assert!(c.upload_rgba8(2, 1, &[0; 4], &[]).is_err());
    assert!(c
        .upload_rgba8(2, 1, &[0; 8], &[(2, 1, vec![0; 8])])
        .is_err());
    assert!(c
        .upload_rgba8(1, 1, &[0; 4], &[(1, 1, vec![0; 4])])
        .is_err());
    let target = WgpuTargetConfig {
        width: 3,
        height: 2,
        format: wgpu::TextureFormat::Bgra8Unorm,
    };
    let texture = c.output_texture(target).unwrap();
    c.queue().write_texture(
        texture.as_image_copy(),
        &[10, 20, 90, 255].repeat(6),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(12),
            rows_per_image: Some(2),
        },
        texture.size(),
    );
    assert_eq!(c.read_rgba8(&texture).unwrap(), [90, 20, 10, 255].repeat(6));
    assert!(c
        .output_texture(WgpuTargetConfig {
            width: u32::MAX,
            ..target
        })
        .is_err());
}
#[test]
fn submitted_frames_keep_geometry_and_composite_preserves_host_background() {
    let c = WgpuContext::new().unwrap();
    let target = WgpuTargetConfig {
        width: 16,
        height: 16,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let source = c.upload_rgba8(1, 1, &[255, 0, 0, 128], &[]).unwrap();
    let view = source.create_view(&Default::default());
    let textures = WgpuTextureCatalog::new(HashMap::from([(
        "red".into(),
        WgpuTexture {
            view: &view,
            width: 1,
            height: 1,
        },
    )]));
    let mut frame = DrawableFrame {
        drawables: vec![Drawable {
            id: "mesh".into(),
            texture_asset_id: "red".into(),
            positions: vec![Vec2::new(0., 0.), Vec2::new(8., 0.), Vec2::new(0., -16.)],
            uvs: Arc::from([Vec2::default(); 3]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }],
        ..Default::default()
    };
    let viewport = ViewportConfig {
        transform: Affine2 {
            b: Vec2::new(0., -1.),
            ..Affine2::IDENTITY
        },
        target_extent: Vec2::new(16., 16.),
        mask_scale: 1.,
    };
    let a = c.output_texture(target).unwrap();
    let b = c.output_texture(target).unwrap();
    let mut renderer = WgpuRenderer::new(c.device(), target).unwrap();
    {
        let output_view = a.create_view(&Default::default());
        let mut encoder = c.device().create_command_encoder(&Default::default());
        assert_eq!(
            renderer
                .encode(
                    WgpuEncodeTarget {
                        device: c.device(),
                        queue: c.queue(),
                        encoder: &mut encoder,
                        output: &output_view,
                        output_mode: WgpuOutputMode::Replace,
                    },
                    &textures,
                )
                .unwrap_err()
                .code,
            "MISSING_MODEL"
        );
    }
    for (i, output) in [&a, &b].into_iter().enumerate() {
        if i == 1 {
            for p in &mut frame.drawables[0].positions {
                p.x += 8.;
            }
        }
        renderer.sync_model(c.device(), &frame, &textures).unwrap();
        renderer.update_view(c.device(), viewport).unwrap();
        c.queue().write_texture(
            output.as_image_copy(),
            &[0, 0, 255, 255].repeat(256),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64),
                rows_per_image: Some(16),
            },
            output.size(),
        );
        let output_view = output.create_view(&Default::default());
        let mut encoder = c.device().create_command_encoder(&Default::default());
        renderer
            .encode(
                WgpuEncodeTarget {
                    device: c.device(),
                    queue: c.queue(),
                    encoder: &mut encoder,
                    output: &output_view,
                    output_mode: WgpuOutputMode::Composite,
                },
                &textures,
            )
            .unwrap();
        c.queue().submit([encoder.finish()]);
    }
    let a = c.read_rgba8(&a).unwrap();
    let b = c.read_rgba8(&b).unwrap();
    let pixel = |v: &[u8], x: usize| v[(16 + x) * 4..][..4].to_vec();
    assert_eq!(pixel(&a, 1), [128, 0, 127, 255]);
    assert_eq!(pixel(&a, 9), [0, 0, 255, 255]);
    assert_eq!(pixel(&b, 1), [0, 0, 255, 255]);
    assert_eq!(pixel(&b, 9), [128, 0, 127, 255]);
}
