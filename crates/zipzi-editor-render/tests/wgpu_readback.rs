#![cfg(feature = "wgpu-backend")]
use std::sync::Arc;
use zipzi_core::{Drawable, DrawableFrame, Vec2};
use zipzi_editor_core::{
    SceneSnapshot, SceneTexture, ViewportMetrics, ViewportPoint, ViewportTransform,
};
use zipzi_editor_render::{WgpuImageReadback, WgpuSceneRenderer, WgpuTargetConfig};

#[test]
fn immutable_frames_keep_pixels_across_viewports_and_texture_changes() -> anyhow::Result<()> {
    let frame = Arc::new(DrawableFrame {
        drawables: vec![Drawable {
            id: "triangle".into(),
            texture_asset_id: "color".into(),
            positions: vec![Vec2::new(0., 0.), Vec2::new(60., 0.), Vec2::new(0., -60.)],
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
            cache_key: "red".into(),
            width: 1,
            height: 1,
            rgba: Arc::from([255, 0, 0, 128]),
        }]),
    };
    // Non-aligned width exercises GPU row padding during readback.
    let target = WgpuTargetConfig {
        width: 67,
        height: 64,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut renderer = WgpuSceneRenderer::new(target)?;
    println!(
        "adapter={} backend={}",
        renderer.context().adapter_name(),
        renderer.context().adapter_backend()
    );
    let transform = ViewportTransform {
        scale: 1.,
        offset: ViewportPoint::default(),
        metrics: ViewportMetrics {
            width: 67.,
            height: 64.,
            scale_factor: 1.,
        },
    };
    let first = renderer.submit_transformed(target, &scene, transform)?;
    let pixels = renderer
        .context()
        .read_rgba8(&first.image)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(pixels.len(), 67 * 64 * 4);
    assert_eq!(
        &pixels[(5 * 67 + 5) * 4..(5 * 67 + 5) * 4 + 4],
        &[128, 0, 0, 128]
    );
    assert_eq!(&pixels[(63 * 67 + 66) * 4..], &[0, 0, 0, 0]);
    let first_bgra = renderer.read_straight_bgra8(&first.image)?;
    assert_eq!(
        &first_bgra[(5 * 67 + 5) * 4..(5 * 67 + 5) * 4 + 4],
        &[0, 0, 255, 128]
    );
    let other = renderer.submit_transformed(
        target,
        &scene,
        ViewportTransform {
            offset: ViewportPoint { x: 20., y: 0. },
            ..transform
        },
    )?;
    let moved = renderer
        .context()
        .read_rgba8(&other.image)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_ne!(pixels, moved);
    assert_eq!(renderer.stats().1, 1, "view changes reuse textures");
    assert_eq!(
        renderer.last_frame_stats().vertex_upload_bytes,
        0,
        "view changes reuse geometry"
    );
    let mut changed = scene.clone();
    Arc::make_mut(&mut changed.textures)[0].cache_key = "green".into();
    Arc::make_mut(&mut changed.textures)[0].rgba = Arc::from([0, 255, 0, 255]);
    let latest = renderer.submit_transformed(target, &changed, transform)?;
    let green = renderer
        .context()
        .read_rgba8(&latest.image)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(
        &green[(5 * 67 + 5) * 4..(5 * 67 + 5) * 4 + 4],
        &[0, 255, 0, 255]
    );
    assert_eq!(renderer.stats().1, 2);
    assert_ne!(first_bgra, renderer.read_straight_bgra8(&latest.image)?);
    assert_eq!(first_bgra, renderer.read_straight_bgra8(&first.image)?);
    assert_eq!(
        pixels,
        renderer
            .context()
            .read_rgba8(&first.image)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        "published frames must remain immutable when another viewport renders"
    );
    Ok(())
}

#[test]
fn gpu_bgra_matches_integer_conversion_for_every_channel_and_alpha() -> anyhow::Result<()> {
    let renderer = WgpuSceneRenderer::new(WgpuTargetConfig {
        width: 1,
        height: 1,
        format: wgpu::TextureFormat::Rgba8Unorm,
    })?;
    let context = renderer.context();
    let mut readback = WgpuImageReadback::new(context.device());
    // Odd widths exercise dispatch edges and packed rows. Revisit sizes and swap
    // channel order to expose stale data after staging reuse and resizing.
    for (width, format) in [
        (257, wgpu::TextureFormat::Rgba8Unorm),
        (67, wgpu::TextureFormat::Bgra8Unorm),
        (257, wgpu::TextureFormat::Bgra8Unorm),
        (257, wgpu::TextureFormat::Rgba8Unorm),
    ] {
        let height = 65536u32.div_ceil(width);
        let mut source = Vec::new();
        let mut expected = Vec::new();
        for index in 0..width * height {
            let value = (index % 256) as u8;
            let alpha = ((index / 256) % 256) as u8;
            let red = value;
            let green = value.wrapping_add(91);
            let blue = 255 - value;
            // Includes nonzero colors at alpha=0 and colors exceeding alpha:
            // additive blends can produce the latter and must still saturate.
            source.extend_from_slice(&if format == wgpu::TextureFormat::Bgra8Unorm {
                [blue, green, red, alpha]
            } else {
                [red, green, blue, alpha]
            });
            let straight = |v: u8| {
                if alpha == 0 {
                    0
                } else {
                    ((u32::from(v) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8
                }
            };
            expected.extend_from_slice(&[straight(blue), straight(green), straight(red), alpha]);
        }
        let texture = context
            .output_texture(WgpuTargetConfig {
                width,
                height,
                format,
            })
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        context.queue().write_texture(
            texture.as_image_copy(),
            &source,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            texture.size(),
        );
        let image = readback.read(context, &texture)?;
        assert_eq!(image, expected, "width={width} format={format:?}");
        assert_eq!(readback.read(context, &texture)?, image, "same-size reuse");
    }
    let srgb = context
        .output_texture(WgpuTargetConfig {
            width: 1,
            height: 1,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        })
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert!(
        readback.read(context, &srgb).is_err(),
        "sRGB must not be silently decoded"
    );
    Ok(())
}
