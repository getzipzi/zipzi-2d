use std::{collections::HashMap, sync::Arc};
use zipzi_core::{
    evaluation::{Drawable, DrawableFrame},
    Vec2,
};
use zipzi_render::{Affine2, ViewportConfig};
use zipzi_render_wgpu::*;
fn frame() -> DrawableFrame {
    DrawableFrame {
        drawables: vec![Drawable {
            id: "mesh".into(),
            texture_asset_id: "art".into(),
            positions: vec![Vec2::new(0., 0.), Vec2::new(16., 0.), Vec2::new(0., -16.)],
            uvs: Arc::from([Vec2::default(); 3]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn view() -> ViewportConfig {
    ViewportConfig {
        transform: Affine2 {
            b: Vec2::new(0., -1.),
            ..Affine2::IDENTITY
        },
        target_extent: Vec2::new(8., 8.),
        mask_scale: 1.,
    }
}
fn render(
    c: &WgpuContext,
    r: &mut WgpuRenderer,
    output: &wgpu::Texture,
    textures: &WgpuTextureCatalog<'_>,
) -> WgpuRenderStats {
    let mut encoder = c.device().create_command_encoder(&Default::default());
    let output_view = output.create_view(&Default::default());
    let stats = r
        .encode(
            WgpuEncodeTarget {
                device: c.device(),
                queue: c.queue(),
                encoder: &mut encoder,
                output: &output_view,
                output_mode: WgpuOutputMode::Replace,
            },
            textures,
        )
        .unwrap();
    c.queue().submit([encoder.finish()]);
    stats
}
#[test]
fn stable_bindings_are_reused_but_view_replacement_and_resize_do_not_reuse_stale_resources() {
    let c = WgpuContext::new().unwrap();
    let mut target = WgpuTargetConfig {
        width: 8,
        height: 8,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut r = WgpuRenderer::new(c.device(), target).unwrap();
    for (i, color) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        // New allocation with the same asset/revision must replace bindings.
        let texture = c.upload_rgba8(1, 1, color, &[]).unwrap();
        let v = texture.create_view(&Default::default());
        let mut catalog = WgpuTextureCatalog::new(HashMap::from([(
            "art".into(),
            WgpuTexture {
                view: &v,
                width: 1,
                height: 1,
            },
        )]));
        catalog.set_revision("art", 1);
        r.sync_model(c.device(), &frame(), &catalog).unwrap();
        r.update_view(c.device(), view()).unwrap();
        let output = c.output_texture(target).unwrap();
        let cold = render(&c, &mut r, &output, &catalog);
        assert_eq!(cold.bind_group_creations, 1);
        let warm = render(&c, &mut r, &output, &catalog);
        assert_eq!(warm.bind_group_creations, 0);
        assert_eq!(warm.uniform_buffer_creations, 0);
        assert_eq!(warm.color_attachment_bytes, 0);
        assert_eq!(warm.render_passes, 1);
        assert_eq!(warm.attachment_bytes, 8 * 8 * 4);
        assert_eq!(&c.read_rgba8(&output).unwrap()[..4], color);
        if i == 1 {
            target.format = wgpu::TextureFormat::Bgra8Unorm;
            r.resize(c.device(), target).unwrap();
        }
    }
}
#[test]
fn replace_preserves_self_sampled_input_by_using_an_intermediate() {
    let c = WgpuContext::new().unwrap();
    let target = WgpuTargetConfig {
        width: 8,
        height: 8,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let output = c.output_texture(target).unwrap();
    c.queue().write_texture(
        output.as_image_copy(),
        &[255, 0, 0, 255].repeat(64),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(8),
        },
        output.size(),
    );
    let v = output.create_view(&Default::default());
    let catalog = WgpuTextureCatalog::new(HashMap::from([(
        "art".into(),
        WgpuTexture {
            view: &v,
            width: 8,
            height: 8,
        },
    )]));
    let mut r = WgpuRenderer::new(c.device(), target).unwrap();
    r.sync_model(c.device(), &frame(), &catalog).unwrap();
    r.update_view(c.device(), view()).unwrap();
    let stats = render(&c, &mut r, &output, &catalog);
    assert_eq!(stats.color_attachment_bytes, 8 * 8 * 4);
    assert_eq!(&c.read_rgba8(&output).unwrap()[..4], &[255, 0, 0, 255]);
}
#[test]
fn context_uses_the_adapters_texture_resolution() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let instance = wgpu::Instance::default();
    let mut future = std::pin::pin!(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }));
    let adapter = loop {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            break result.unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    let c = WgpuContext::new().unwrap();
    assert_eq!(
        c.device().limits().max_texture_dimension_2d,
        adapter.limits().max_texture_dimension_2d
    );
    let width = 8193.min(adapter.limits().max_texture_dimension_2d);
    c.upload_rgba8(width, 1, &[100, 200, 10, 255].repeat(width as usize), &[])
        .unwrap();
}
