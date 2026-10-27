//! Same inputs through the production wgpu and native Metal encoders.
#![cfg(target_os = "macos")]
use std::sync::Arc;
use zipzi_core::{
    evaluation::{Drawable, DrawableFrame, OffscreenFrame},
    Vec2,
};
use zipzi_render::{Affine2, ViewportConfig};
use zipzi_render_metal::{
    metal::MTLPixelFormat, MetalContext, MetalOutputMode, MetalRenderer, MetalTargetConfig,
    MetalTexture, MetalTextureCatalog,
};
use zipzi_render_wgpu::*;

fn quad(id: &str, texture: &str, rect: [f32; 4]) -> Drawable {
    let [x, y, w, h] = rect;
    Drawable {
        id: id.into(),
        texture_asset_id: texture.into(),
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
fn view(scale: f32, x: f32) -> ViewportConfig {
    ViewportConfig {
        transform: Affine2 {
            a: Vec2::new(scale, 0.),
            b: Vec2::new(0., -scale),
            origin: Vec2::new(x, 0.),
        },
        target_extent: Vec2::new(128., 128.),
        mask_scale: scale as f64,
    }
}
fn group(id: &str, parent: Option<&str>, order: i32) -> OffscreenFrame {
    OffscreenFrame {
        renderable: true,
        id: id.into(),
        runtime_id: id.into(),
        owner_part_id: String::new(),
        parent_offscreen_id: parent.map(str::to_owned),
        render_order: order,
        opacity: 0.8,
        enabled: true,
        blend_mode: 0,
        flags: 0,
        masks: vec![],
        multiply_color: [0.7, 0.9, 1., 1.],
        screen_color: [0.1, 0.2, 0.05, 1.],
    }
}
struct Harness {
    w: WgpuContext,
    m: MetalContext,
    wr: WgpuRenderer,
    mr: MetalRenderer,
    wt: Vec<wgpu::TextureView>,
    mt: Vec<zipzi_render_metal::metal::Texture>,
}
impl Harness {
    fn new() -> Self {
        let w = WgpuContext::new().unwrap();
        let m = MetalContext::new().unwrap();
        eprintln!("adapter: {} / {}", w.adapter_name(), w.adapter_backend());
        let pixels = [
            vec![255, 0, 255, 0, 20, 220, 80, 190],
            vec![255, 70, 30, 180, 40, 80, 240, 255],
            vec![255, 255, 255, 128, 255, 255, 255, 255],
        ];
        let wt = pixels
            .iter()
            .map(|p| {
                w.upload_rgba8(
                    2,
                    1,
                    p,
                    &zipzi_render::texture::straight_rgba_mipmaps(2, 1, p),
                )
                .unwrap()
                .create_view(&Default::default())
            })
            .collect();
        let mt = pixels
            .iter()
            .map(|p| {
                m.upload_rgba8(
                    2,
                    1,
                    p,
                    &zipzi_render::texture::straight_rgba_mipmaps(2, 1, p),
                )
                .unwrap()
            })
            .collect();
        let wr = WgpuRenderer::new(
            w.device(),
            WgpuTargetConfig {
                width: 128,
                height: 128,
                format: wgpu::TextureFormat::Rgba8Unorm,
            },
        )
        .unwrap();
        let mr = MetalRenderer::new(
            &m,
            MetalTargetConfig {
                width: 128,
                height: 128,
                format: MTLPixelFormat::RGBA8Unorm,
            },
        )
        .unwrap();
        Self {
            w,
            m,
            wr,
            mr,
            wt,
            mt,
        }
    }
    fn render(
        &mut self,
        frame: &DrawableFrame,
        view: ViewportConfig,
        sync: bool,
        label: &str,
    ) -> WgpuRenderStats {
        let mut wc = WgpuTextureCatalog::new(
            self.wt
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    (
                        i.to_string(),
                        WgpuTexture {
                            view: t,
                            width: 2,
                            height: 1,
                        },
                    )
                })
                .collect(),
        );
        let mut mc = MetalTextureCatalog::new(
            self.mt
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    (
                        i.to_string(),
                        MetalTexture {
                            view: t,
                            width: 2,
                            height: 1,
                        },
                    )
                })
                .collect(),
        );
        for i in 0..3 {
            wc.set_revision(i.to_string(), 1);
            mc.set_revision(i.to_string(), 1);
        }
        if sync {
            self.wr.sync_model(self.w.device(), frame, &wc).unwrap();
            self.mr.sync_model(frame, &mc).unwrap();
        }
        self.wr.update_view(self.w.device(), view).unwrap();
        self.mr.update_view(view).unwrap();
        let wo = self.w.output_texture(self.wr.target()).unwrap();
        let wv = wo.create_view(&Default::default());
        let mo = self.m.output_texture(self.mr.target()).unwrap();
        let mut encoder = self.w.device().create_command_encoder(&Default::default());
        let stats = self
            .wr
            .encode(
                WgpuEncodeTarget {
                    device: self.w.device(),
                    queue: self.w.queue(),
                    encoder: &mut encoder,
                    output: &wv,
                    output_mode: WgpuOutputMode::Replace,
                },
                &wc,
            )
            .unwrap();
        self.w.queue().submit([encoder.finish()]);
        let command = self.m.queue().new_command_buffer();
        self.mr
            .encode(command, &mo, MetalOutputMode::Replace, &mc)
            .unwrap();
        command.commit();
        command.wait_until_completed();
        let actual = self.w.read_rgba8(&wo).unwrap();
        let expected = zipzi_render_metal::read_rgba8(&mo).unwrap();
        let max = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        let mean = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum::<u64>() as f64
            / actual.len() as f64;
        assert!(max <= 3 && mean < 0.4, "{label}: max={max} mean={mean}");
        stats
    }
}
#[test]
fn transparent_filtering_masks_nested_crops_and_all_blends_match_metal() {
    let mut h = Harness::new();
    let mut frame = DrawableFrame {
        drawables: vec![
            quad("base", "1", [0., 0., 128., 128.]),
            quad("source", "2", [13., -9., 50., 70.]),
            quad("front", "0", [4., -5., 100., 105.]),
        ],
        ..Default::default()
    };
    frame.drawables[1].visible = false;
    frame.drawables[2].masks = vec!["source".into()];
    frame.drawables[2].render_order = 2;
    frame.drawables[2].multiply_color = [0.8, 0.6, 0.9, 1.];
    frame.drawables[2].screen_color = [0.2, 0.1, 0.3, 1.];
    for inverted in [false, true] {
        frame.drawables[2].inverted_mask = inverted;
        for color in 0..18 {
            for alpha in 0..5 {
                frame.drawables[2].raw_blend_mode = Some(color | (alpha << 8));
                h.render(
                    &frame,
                    view(1., 0.),
                    true,
                    &format!("blend {color}/{alpha}, inverted={inverted}"),
                );
            }
        }
    }
    frame.drawables[2].raw_blend_mode = None;
    use zipzi_core::evaluation::RenderCommand::*;
    frame.render_plan = vec![
        DrawMesh {
            mesh_id: "base".into(),
        },
        DrawMesh {
            mesh_id: "source".into(),
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
    frame.offscreens = vec![group("outer", None, 3), group("inner", Some("outer"), 2)];
    frame.offscreens[0].masks = vec!["source".into()];
    for mode in [0, 1, 2, 4, 16, 17, 256, 512, 768, 1024] {
        frame.offscreens[0].blend_mode = mode;
        h.render(&frame, view(1., 0.), true, &format!("nested {mode}"));
    }
    let pan = h.render(&frame, view(1.2, -3.), false, "warm pan");
    assert_eq!(pan.vertex_upload_bytes, 0);
    h.render(&frame, view(1.2, -3.), true, "unchanged submission");
}
#[test]
fn atlas_caches_geometry_and_survives_pan_and_source_change() {
    let mut h = Harness::new();
    let mut frame = DrawableFrame::default();
    for i in 0..40 {
        let x = (i % 8) as f32 * 15.;
        let y = -((i / 8) as f32) * 22.;
        let mut mask = quad(&format!("mask{i}"), "2", [x, y, 10., 18.]);
        mask.visible = false;
        let mut mesh = quad(&format!("mesh{i}"), "0", [x, y, 13., 20.]);
        mesh.masks = vec![mask.id.clone()];
        mesh.render_order = i;
        frame.drawables.extend([mask, mesh]);
    }
    let first = h.render(&frame, view(1., 0.), true, "atlas");
    assert_eq!(first.mask_redraws, 40);
    let warm = h.render(&frame, view(1., 1.), false, "atlas pan");
    assert_eq!(warm.mask_redraws, 0);
    let same = h.render(&frame, view(1., 1.), true, "same model");
    assert_eq!(same.mask_redraws, 0);
    assert_eq!(same.vertex_upload_bytes, 0);
    frame.drawables[0].positions[0].x += 2.;
    let changed = h.render(&frame, view(1., 1.), true, "changed source");
    assert!(changed.mask_redraws > 0);
}
#[test]
fn winding_and_mask_sources_follow_metal_under_reflection() {
    let mut h = Harness::new();
    let mut d = quad("front", "1", [0., 0., 64., 64.]);
    d.double_sided = false;
    d.indices = Arc::from([0, 2, 1, 0, 3, 2]);
    let mut frame = DrawableFrame {
        drawables: vec![d],
        ..Default::default()
    };
    h.render(&frame, view(1., 0.), true, "front facing");
    frame.drawables[0].indices = Arc::from([0, 1, 2, 0, 2, 3]);
    h.render(&frame, view(1., 0.), true, "back facing");
    let mut mesh = quad("consumer", "0", [0., 0., 64., 64.]);
    mesh.masks = vec!["front".into()];
    mesh.render_order = 1;
    frame.drawables.push(mesh);
    h.render(&frame, view(1., 0.), true, "culled mask source");
}
