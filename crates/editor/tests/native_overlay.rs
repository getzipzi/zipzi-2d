//! Production overlay regression tests; no window or benchmark sampling.
#![cfg(target_os = "macos")]
use editor_core::{CanvasPoint, OverlaySnapshot, OverlayVertex};
use editor_render::{MetalOverlayRenderer, OverlayStyle, OverlayViewport};
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, PaintSurface, Quad, Scene, SurfaceSource, point, px, rgba,
    size,
};
#[path = "../src/mesh_edges.rs"]
mod mesh_edges;

fn pt(x: f32, y: f32) -> CanvasPoint {
    CanvasPoint { x, y }
}
fn geometry() -> OverlaySnapshot {
    OverlaySnapshot {
        lines: vec![
            (pt(-20., 10.25), pt(75., 45.5)),
            (pt(15., 55.), pt(60., -12.)),
            (pt(20., 20.), pt(20., 20.)),
        ],
        vertices: vec![
            OverlayVertex {
                id: 3,
                position: pt(20.25, 20.5),
                selected: true,
                hovered: true,
            },
            OverlayVertex {
                id: 9,
                position: pt(24.25, 22.5),
                selected: false,
                hovered: false,
            },
            OverlayVertex {
                id: 11,
                position: pt(64., 32.),
                selected: false,
                hovered: true,
            },
        ],
        unavailable: None,
    }
}
fn color(value: u32) -> [f32; 4] {
    let c = rgba(value);
    [c.r, c.g, c.b, c.a]
}
fn style() -> OverlayStyle {
    OverlayStyle {
        edge: color(0xa0b0c073),
        vertex: color(0xf0f0f0b0),
        selected: color(0xff4060c0),
    }
}
fn snap(v: f32) -> f32 {
    (v.abs() - 0.5).ceil().copysign(v)
}
fn make_scene(
    overlay: &OverlaySnapshot,
    viewport: OverlayViewport,
    texture: Option<metal::Texture>,
) -> Scene {
    let mut scene = Scene::default();
    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(192.), px(192.)));
    let mask = ContentMask {
        bounds: Bounds::new(point(px(12.), px(10.)), size(px(110.), px(110.))),
    };
    let quad = |bounds, color, mask| Quad {
        order: 0,
        border_style: Default::default(),
        bounds,
        content_mask: mask,
        background: rgba(color).into(),
        border_color: Default::default(),
        corner_radii: Default::default(),
        border_widths: Default::default(),
    };
    scene.insert_primitive(quad(
        bounds.scale(1.),
        0x202830ff,
        ContentMask {
            bounds: bounds.scale(1.),
        },
    ));
    if let Some(texture) = texture {
        scene.insert_primitive(PaintSurface {
            order: 0,
            bounds: Bounds::new(
                point(
                    px((viewport.origin[0] * viewport.scale).floor()),
                    px((viewport.origin[1] * viewport.scale).floor()),
                ),
                size(px(viewport.extent[0] as f32), px(viewport.extent[1] as f32)),
            )
            .scale(1.),
            content_mask: mask.scale(1.),
            source: SurfaceSource::MetalTexture(texture),
            opacity: 1.,
        });
    } else {
        mesh_edges::paint_edges(
            &overlay.lines,
            point(px(viewport.origin[0]), px(viewport.origin[1])),
            |mut path| {
                path.color = rgba(0xa0b0c073).into();
                path.content_mask = ContentMask {
                    bounds: Bounds::new(
                        mask.bounds.origin / viewport.scale,
                        mask.bounds.size / viewport.scale,
                    ),
                };
                scene.insert_primitive(path.scale(viewport.scale));
            },
        );
        for v in &overlay.vertices {
            let r = if v.hovered { 4. } else { 3. };
            let x = v.position.x + viewport.origin[0];
            let y = v.position.y + viewport.origin[1];
            let b = Bounds::from_corners(
                point(
                    px(snap((x - r) * viewport.scale)),
                    px(snap((y - r) * viewport.scale)),
                ),
                point(
                    px(snap((x + r) * viewport.scale)),
                    px(snap((y + r) * viewport.scale)),
                ),
            );
            scene.insert_primitive(quad(
                b.scale(1.),
                if v.selected { 0xff4060c0 } else { 0xf0f0f0b0 },
                mask.scale(1.),
            ));
        }
    }
    scene.finish();
    scene
}
#[test]
fn overlay_matches_paths_markers_and_retains_old_frames() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let mut renderer = MetalOverlayRenderer::new(gpu.device.clone())?;
        let overlay = geometry();
        for scale in [1., 1.5, 2.] {
            let viewport = OverlayViewport {
                extent: [160, 160],
                scale,
                origin: [7.25, 5.5],
            };
            let image = renderer
                .draw(&gpu.queue, viewport, &overlay, style())?
                .unwrap();
            let cached = renderer
                .draw(&gpu.queue, viewport, &overlay, style())?
                .unwrap();
            assert!(std::ptr::eq(image.as_ref(), cached.as_ref()));
            let old = make_scene(&overlay, viewport, Some(image.clone()));
            let expected = gpui.render_scene_to_image(
                &make_scene(&overlay, viewport, None),
                size(DevicePixels(192), DevicePixels(192)),
            )?;
            let actual =
                gpui.render_scene_to_image(&old, size(DevicePixels(192), DevicePixels(192)))?;
            let mut total = 0u64;
            let mut changed = 0;
            for (a, b) in actual.pixels().zip(expected.pixels()) {
                let delta =
                    a.0.iter()
                        .zip(b.0)
                        .map(|(a, b)| a.abs_diff(b))
                        .max()
                        .unwrap();
                changed += usize::from(delta > 4);
                total +=
                    a.0.iter()
                        .zip(b.0)
                        .map(|(a, b)| u64::from(a.abs_diff(b)))
                        .sum::<u64>();
            }
            assert!(
                total as f64 / (192. * 192. * 4.) < 0.5,
                "pixel mean error: {total}"
            );
            assert!(changed < 192 * 192 / 100, "changed pixels: {changed}");
            // Publish edits, a theme change and a resize before sampling the old frame.
            let mut edited = overlay.clone();
            edited.vertices[0].position = pt(50., 50.);
            edited.vertices[0].selected = false;
            let mut theme = style();
            theme.vertex = [0., 1., 0., 1.];
            let resized = OverlayViewport {
                extent: [200, 180],
                ..viewport
            };
            let next = renderer.draw(&gpu.queue, resized, &edited, theme)?.unwrap();
            assert!(!std::ptr::eq(image.as_ref(), next.as_ref()));
            assert_eq!((next.width(), next.height()), (200, 180));
            let changed = gpui.render_scene_to_image(
                &make_scene(&edited, resized, Some(next)),
                size(DevicePixels(192), DevicePixels(192)),
            )?;
            let x = ((50. + viewport.origin[0]) * scale).floor() as u32;
            let y = ((50. + viewport.origin[1]) * scale).floor() as u32;
            assert_eq!(
                changed.get_pixel(x, y).0,
                [0, 255, 0, 255],
                "position, selection and theme update"
            );
            let other_queue = gpu.device.new_command_queue();
            let on_other_queue = renderer
                .draw(&other_queue, viewport, &overlay, style())?
                .unwrap();
            let back_on_consumer = renderer
                .draw(&gpu.queue, viewport, &overlay, style())?
                .unwrap();
            assert!(
                !std::ptr::eq(on_other_queue.as_ref(), back_on_consumer.as_ref()),
                "a new consumer queue needs its own ordered producer"
            );
            let retained =
                gpui.render_scene_to_image(&old, size(DevicePixels(192), DevicePixels(192)))?;
            assert_eq!(actual, retained, "published frames must remain immutable");
            assert!(
                renderer
                    .draw(&gpu.queue, viewport, &OverlaySnapshot::default(), style())?
                    .is_none()
            );
        }
        Ok(())
    })
}
