#![cfg(target_os = "windows")]
use anyhow::{Context, Result};
use std::{
    future::Future,
    sync::Arc,
    task::{Poll, Waker},
};
use zipzi_core::{
    evaluation::{OffscreenFrame, RenderCommand},
    Drawable, DrawableFrame, Vec2,
};
use zipzi_render::{texture::straight_rgba_mipmaps, Affine2, ViewportConfig};
use zipzi_render_d3d11::{
    upload_rgba8, D3d11Context, D3d11Renderer, D3d11Target, D3d11Texture, D3d11TextureCatalog,
};
use zipzi_render_wgpu::{
    wgpu, WgpuContext, WgpuRenderer, WgpuTargetConfig, WgpuTexture, WgpuTextureCatalog,
};

const EXTENT: [u32; 2] = [129, 127];

struct Harness {
    native: D3d11Context,
    renderer: D3d11Renderer,
    target: D3d11Target,
    sources: Vec<D3d11Texture>,
    wgpu: WgpuContext,
    wgpu_renderer: WgpuRenderer,
    wgpu_target: wgpu::Texture,
    wgpu_sources: Vec<wgpu::TextureView>,
}

impl Harness {
    fn new() -> Result<Self> {
        let native = D3d11Context::new()?;
        let renderer = D3d11Renderer::new(native.device().clone())?;
        let target = D3d11Target::new(native.device(), EXTENT)?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        };
        let mut adapter = std::pin::pin!(instance.request_adapter(&options));
        let mut task = std::task::Context::from_waker(Waker::noop());
        let adapter = loop {
            match adapter.as_mut().poll(&mut task) {
                Poll::Ready(result) => break result?,
                Poll::Pending => std::thread::yield_now(),
            }
        };
        let wgpu = WgpuContext::from_adapter(adapter).map_err(status)?;
        let config = WgpuTargetConfig {
            width: EXTENT[0],
            height: EXTENT[1],
            format: wgpu::TextureFormat::Rgba8Unorm,
        };
        let wgpu_renderer = WgpuRenderer::new(wgpu.device(), config).map_err(status)?;
        let wgpu_target = wgpu.output_texture(config).map_err(status)?;
        let mut sources = Vec::new();
        let mut wgpu_sources = Vec::new();
        for pixels in [
            [255, 70, 30, 180, 40, 80, 240, 255],
            [255, 255, 255, 128, 255, 255, 255, 255],
            [255, 0, 255, 0, 20, 220, 80, 190],
        ] {
            let mips = straight_rgba_mipmaps(2, 1, &pixels);
            sources.push(upload_rgba8(native.device(), 2, 1, &pixels, &mips)?);
            wgpu_sources.push(
                wgpu.upload_rgba8(2, 1, &pixels, &mips)
                    .map_err(status)?
                    .create_view(&Default::default()),
            );
        }
        eprintln!(
            "D3D11 {} vs {} {}",
            native.name(),
            wgpu.adapter_name(),
            wgpu.adapter_backend()
        );
        Ok(Self {
            native,
            renderer,
            target,
            sources,
            wgpu,
            wgpu_renderer,
            wgpu_target,
            wgpu_sources,
        })
    }

    fn compare(
        &mut self,
        frame: &DrawableFrame,
        scale: f32,
        offset: Vec2,
        label: &str,
    ) -> Result<()> {
        let view = ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(scale, 0.),
                b: Vec2::new(0., -scale),
                origin: offset,
            },
            target_extent: Vec2::new(EXTENT[0] as f32, EXTENT[1] as f32),
            mask_scale: f64::from(scale),
        };
        let keys = ["base", "mask", "front"];
        let native_catalog = D3d11TextureCatalog::new(keys.into_iter().zip(&self.sources));
        let command = self.renderer.record(
            &self.target,
            Arc::new(frame.clone()),
            view,
            &native_catalog,
            |_| {},
        )?;
        self.native.submit(&command)?;
        let actual = self.native.read_rgba8(&self.target)?;
        let mut wgpu_catalog = WgpuTextureCatalog::new(
            keys.into_iter()
                .zip(&self.wgpu_sources)
                .map(|(key, view)| {
                    (
                        key.to_owned(),
                        WgpuTexture {
                            view,
                            width: 2,
                            height: 1,
                        },
                    )
                })
                .collect(),
        );
        for key in keys {
            wgpu_catalog.set_repeat(key, true);
            wgpu_catalog.set_revision(key, 1);
        }
        let output = self.wgpu_target.create_view(&Default::default());
        self.wgpu_renderer
            .render(
                self.wgpu.device(),
                self.wgpu.queue(),
                &output,
                frame,
                &wgpu_catalog,
                view,
            )
            .map_err(status)?;
        let reference = self.wgpu.read_rgba8(&self.wgpu_target).map_err(status)?;
        let max = actual
            .iter()
            .zip(&reference)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .context("empty pixels")?;
        let mean = actual
            .iter()
            .zip(&reference)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum::<u64>() as f64
            / actual.len() as f64;
        assert!(max <= 3 && mean < 0.4, "{label}: max={max}, mean={mean}");
        Ok(())
    }
}

fn status(error: zipzi_core::Status) -> anyhow::Error {
    anyhow::anyhow!("{}: {}", error.code, error.message)
}

fn quad(id: &str, [x, y, w, h]: [f32; 4]) -> Drawable {
    Drawable {
        id: id.into(),
        texture_asset_id: id.into(),
        positions: vec![
            Vec2::new(x, y),
            Vec2::new(x + w, y),
            Vec2::new(x + w, y - h),
            Vec2::new(x, y - h),
        ],
        uvs: Arc::from([
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ]),
        indices: Arc::from([0, 1, 2, 0, 2, 3]),
        ..Default::default()
    }
}

fn group(id: &str, parent: Option<&str>) -> OffscreenFrame {
    OffscreenFrame {
        renderable: true,
        enabled: true,
        id: id.into(),
        runtime_id: id.into(),
        owner_part_id: String::new(),
        parent_offscreen_id: parent.map(str::to_owned),
        opacity: 0.8,
        render_order: 0,
        blend_mode: 0,
        flags: 0,
        masks: Vec::new(),
        multiply_color: [0.7, 0.9, 1., 1.],
        screen_color: [0.1, 0.2, 0.05, 1.],
    }
}

#[test]
fn recorded_geometry_survives_drops_reordering_and_renderer_teardown() -> Result<()> {
    let mut context = D3d11Context::new()?;
    let device = context.device().clone();
    let source = upload_rgba8(&device, 2, 1, &[255, 40, 0, 255, 0, 80, 255, 128], &[])?;
    let catalog = D3d11TextureCatalog::new([("base", &source), ("mask", &source)]);
    let target = D3d11Target::new(&device, EXTENT)?;
    let view = ViewportConfig {
        transform: Affine2 {
            b: Vec2::new(0., -1.),
            ..Affine2::IDENTITY
        },
        target_extent: Vec2::new(EXTENT[0] as f32, EXTENT[1] as f32),
        mask_scale: 1.,
    };
    let mut frame = DrawableFrame {
        drawables: vec![
            quad("base", [5., -4., 70., 90.]),
            quad("mask", [0., 0., 50., 100.]),
        ],
        ..Default::default()
    };
    frame.drawables[0].masks = vec!["mask".into()];
    frame.drawables[1].visible = false;
    let mut frames = vec![Arc::new(frame.clone())];
    for position in &mut frame.drawables[0].positions {
        position.x += 20.;
    }
    frame.drawables[0].opacity = 0.4;
    frames.push(Arc::new(frame.clone()));
    // Same Arc, but its preceding command list can be discarded by the host.
    frames.push(frames[1].clone());
    frame.drawables[0].indices = Arc::from([0, 1, 2]);
    frame.drawables[0].uvs = Arc::from([Vec2::new(0.8, 0.); 4]);
    frames.push(Arc::new(frame.clone()));
    frame.drawables[0].renderable = false;
    frame.drawables[1].visible = true;
    frames.push(Arc::new(frame.clone()));
    frame.drawables[0].renderable = true;
    frame.drawables[0].positions.push(Vec2::new(15., -110.));
    frame.drawables[0].uvs = Arc::from([Vec2::new(0.2, 0.); 5]);
    frame.drawables[0].indices = Arc::from([0, 2, 4]);
    frames.push(Arc::new(frame));

    let mut expected = Vec::new();
    for frame in &frames {
        let mut fresh = D3d11Renderer::new(device.clone())?;
        let list = fresh.record(&target, frame.clone(), view, &catalog, |_| {})?;
        context.submit(&list)?;
        expected.push(context.read_rgba8(&target)?);
    }
    assert_ne!(expected[0], expected[1]);
    assert_ne!(expected[2], expected[3]);
    let mut renderer = D3d11Renderer::new(device)?;
    let mut commands = frames
        .iter()
        .map(|frame| {
            renderer
                .record(&target, frame.clone(), view, &catalog, |_| {})
                .map(Some)
        })
        .collect::<Result<Vec<_>>>()?;
    commands[1] = None;
    renderer.clear_model();
    drop(renderer);
    for i in [5, 2, 0, 4, 3, 0, 2] {
        context.submit(commands[i].as_ref().unwrap())?;
        assert_eq!(
            context.read_rgba8(&target)?,
            expected[i],
            "recorded frame {i}"
        );
    }
    Ok(())
}

#[test]
fn all_raw_blends_and_nested_offscreens_match_wgpu_dx12() -> Result<()> {
    let mut gpu = Harness::new()?;
    let mut frame = DrawableFrame {
        drawables: vec![
            quad("base", [0., 0., 128., 127.]),
            quad("mask", [13., -9., 50., 70.]),
            quad("front", [4., -5., 100., 105.]),
        ],
        ..Default::default()
    };
    frame.drawables[1].visible = false;
    frame.drawables[2].masks = vec!["mask".into()];
    frame.drawables[2].multiply_color = [0.8, 0.6, 0.9, 1.];
    frame.drawables[2].screen_color = [0.2, 0.1, 0.3, 1.];
    for inverted in [false, true] {
        frame.drawables[2].inverted_mask = inverted;
        for color in 0..18 {
            for alpha in 0..5 {
                frame.drawables[2].raw_blend_mode = Some(color | (alpha << 8));
                gpu.compare(
                    &frame,
                    1.,
                    Vec2::default(),
                    &format!("mesh {color}/{alpha}, inverted={inverted}"),
                )?;
            }
        }
    }
    use RenderCommand::*;
    frame.render_plan = vec![
        DrawMesh {
            mesh_id: "base".into(),
        },
        DrawMesh {
            mesh_id: "mask".into(),
        },
        BeginOffscreen {
            offscreen_id: "outer".into(),
        },
        BeginOffscreen {
            offscreen_id: "inner".into(),
        },
        DrawMesh {
            mesh_id: "front".into(),
        },
        EndOffscreen {
            offscreen_id: "inner".into(),
        },
        EndOffscreen {
            offscreen_id: "outer".into(),
        },
    ];
    frame.offscreens = vec![group("outer", None), group("inner", Some("outer"))];
    frame.offscreens[0].masks = vec!["mask".into()];
    frame.offscreens[0].flags = 8;
    frame.offscreens[1].blend_mode = 3 | (4 << 8);
    frame.drawables[2].raw_blend_mode = Some(10);
    for mode in [0, 1, 2, 4, 16, 17, 256, 512, 768, 1024] {
        frame.offscreens[0].blend_mode = mode;
        gpu.compare(
            &frame,
            0.7,
            Vec2::new(-3.25, 11.5),
            &format!("nested {mode}"),
        )?;
    }
    Ok(())
}
