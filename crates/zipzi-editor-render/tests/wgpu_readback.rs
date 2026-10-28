#![cfg(feature = "wgpu-backend")]
use std::sync::Arc;
use zipzi_core::{Drawable, DrawableFrame, Vec2};
use zipzi_editor_core::{
    SceneSnapshot, SceneTexture, ViewportMetrics, ViewportPoint, ViewportTransform,
};
use zipzi_editor_render::{WgpuSceneRenderer, WgpuTargetConfig};

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
