//! Exercise the production edge builder through native Metal, without UI automation.
#![cfg(target_os = "macos")]
use editor_core::ViewportPoint;
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, Path, PathBuilder, Pixels, Quad, Scene, point, px, rgba,
    size,
};

use crate::mesh_edges;

fn p(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
}

fn edges_scene(batched: bool, scale: f32, zoom: f32, alpha: u8) -> Scene {
    let mut scene = Scene::default();
    let viewport = Bounds::new(point(px(12.), px(10.)), size(px(92.), px(98.)));
    let background = Bounds::new(point(px(0.), px(0.)), size(px(128.), px(128.)));
    let quad = |bounds: Bounds<Pixels>, color| Quad {
        order: 0,
        border_style: Default::default(),
        bounds: bounds.scale(scale),
        content_mask: ContentMask {
            bounds: background.scale(scale),
        },
        background: rgba(color).into(),
        border_color: Default::default(),
        corner_radii: Default::default(),
        border_widths: Default::default(),
    };
    scene.insert_primitive(quad(background, 0x202830ff));
    // Disconnected edges, crossings, shared endpoints, fractional positions and
    // edges extending beyond the viewport. Zoom changes positions, not stroke width.
    let edges: Vec<_> = [
        (p(0., 20.25), p(108., 20.25)),
        (p(25.25, 2.), p(25.25, 120.)),
        (p(40., 40.), p(72., 72.)),
        (p(40., 72.), p(72., 40.)),
        (p(72., 40.), p(90., 68.)),
        (p(5., 90.), p(15., 90.)),
        (p(85., 95.), p(100., 95.)),
    ]
    .into_iter()
    .map(|(a, b)| (p(a.x * zoom, a.y * zoom), p(b.x * zoom, b.y * zoom)))
    .collect();
    let origin = point(px(4.25), px(-2.5));
    let mut paint = |mut path: Path<Pixels>| {
        path.color = rgba(0xa0b0c000 | u32::from(alpha)).into();
        path.content_mask = ContentMask { bounds: viewport };
        scene.insert_primitive(path.scale(scale));
    };
    if batched {
        mesh_edges::paint_edges(&edges, origin, &mut paint);
    } else {
        for (a, b) in &edges {
            let mut path = PathBuilder::stroke(px(1.));
            path.move_to(origin + point(px(a.x), px(a.y)));
            path.line_to(origin + point(px(b.x), px(b.y)));
            let mut path = path.build().unwrap();
            // The original tight sprite bounds can cut off MSAA fringe pixels.
            // Pad the reference to compare complete strokes, not that old cutoff.
            path.bounds = path.bounds.dilate(px(1.));
            paint(path);
        }
    }
    // Selected and hovered vertex handles must stay above all edges.
    scene.insert_primitive(quad(
        Bounds::new(point(px(24.), px(15.)), size(px(8.), px(8.))),
        0xff4060ff,
    ));
    scene.insert_primitive(quad(
        Bounds::new(point(px(55.), px(49.)), size(px(6.), px(6.))),
        0xf0f0f0ff,
    ));
    scene.finish();
    scene
}

#[test]
fn native_batched_edges_preserve_geometry_clipping_and_handles() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut renderer = MetalRenderer::new_headless(Default::default());
        for scale in [1., 2.] {
            for zoom in [0.5, 1., 2.25] {
                for alpha in [115, 255] {
                    let side = DevicePixels((128. * scale) as i32);
                    let old = renderer.render_scene_to_image(
                        &edges_scene(false, scale, zoom, alpha),
                        size(side, side),
                    )?;
                    let new = renderer.render_scene_to_image(
                        &edges_scene(true, scale, zoom, alpha),
                        size(side, side),
                    )?;
                    for (x, y, pixel) in old.enumerate_pixels() {
                        let delta = pixel
                            .0
                            .iter()
                            .zip(new.get_pixel(x, y).0)
                            .map(|(a, b)| a.abs_diff(b))
                            .max()
                            .unwrap();
                        if delta > 2 {
                            // Shared endpoints can blend before rather than after
                            // MSAA resolve. Limit differences to this junction and
                            // at most one of the four coverage samples (144 / 4).
                            let junction =
                                ((72. * zoom + 4.25) * scale, (40. * zoom - 2.5) * scale);
                            assert!(
                                (x as f32 + 0.5 - junction.0).abs() <= scale
                                    && (y as f32 + 0.5 - junction.1).abs() <= scale
                                    && delta <= 36,
                                "unexpected pixel change at ({x}, {y}): delta={delta}, scale={scale}, zoom={zoom}, alpha={alpha}"
                            );
                        }
                    }
                    assert_eq!(
                        new.get_pixel((27. * scale) as u32, (18. * scale) as u32).0,
                        [255, 64, 96, 255],
                        "selected handle ordering"
                    );
                    assert_eq!(
                        new.get_pixel((57. * scale) as u32, (51. * scale) as u32).0,
                        [240, 240, 240, 255],
                        "hover handle ordering"
                    );
                    assert_eq!(
                        new.get_pixel((5. * scale) as u32, (18. * scale) as u32).0,
                        [32, 40, 48, 255],
                        "viewport clipping"
                    );
                    if zoom == 1. {
                        assert_eq!(
                            new.get_pixel((55. * scale) as u32, (89. * scale) as u32).0,
                            [32, 40, 48, 255],
                            "disjoint edges must not acquire connecting segments"
                        );
                    }
                }
            }
        }
        Ok(())
    })
}

#[test]
fn native_translucent_crossing_keeps_source_over_blending() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut renderer = MetalRenderer::new_headless(Default::default());
        for scale in [1., 2.] {
            let bounds = Bounds::new(point(px(0.), px(0.)), size(px(64.), px(64.)));
            let mut scene = Scene::default();
            scene.insert_primitive(Quad {
                order: 0,
                border_style: Default::default(),
                bounds: bounds.scale(scale),
                content_mask: ContentMask {
                    bounds: bounds.scale(scale),
                },
                background: rgba(0x202830ff).into(),
                border_color: Default::default(),
                corner_radii: Default::default(),
                border_widths: Default::default(),
            });
            mesh_edges::paint_edges(
                &[(p(8., 32.5), p(56., 32.5)), (p(32.5, 8.), p(32.5, 56.))],
                point(px(0.), px(0.)),
                |mut path| {
                    path.color = rgba(0xa0b0c073).into();
                    path.content_mask = ContentMask { bounds };
                    scene.insert_primitive(path.scale(scale));
                },
            );
            scene.finish();
            let side = DevicePixels((64. * scale) as i32);
            let image = renderer.render_scene_to_image(&scene, size(side, side))?;
            for (x, layers) in [(16., 1), (32.5, 2)] {
                let pixel = image.get_pixel((x * scale) as u32, (32.5 * scale) as u32).0;
                let alpha = 1. - (1_f32 - 115. / 255.).powi(layers);
                for ((actual, background), foreground) in pixel[..3]
                    .iter()
                    .zip([32., 40., 48.])
                    .zip([160., 176., 192.])
                {
                    let expected = background * (1. - alpha) + foreground * alpha;
                    assert!(
                        (*actual as f32 - expected).abs() <= 2.,
                        "incorrect opacity at {layers}-stroke coverage: {pixel:?}"
                    );
                }
                assert_eq!(pixel[3], 255);
            }
        }
        Ok(())
    })
}

#[test]
fn large_mesh_retains_every_edge_past_u16_tessellation_capacity() {
    let edges: Vec<_> = (0..20_001)
        .map(|i| (p(i as f32, 0.), p(i as f32, 10.)))
        .collect();
    let mut paths = Vec::new();
    mesh_edges::paint_edges(&edges, point(px(3.), px(7.)), |path| paths.push(path));
    assert_eq!(paths.len(), 5, "large mesh must use bounded batches");
    // Straight butt-capped strokes have two triangles. Check each edge's actual
    // tessellated rectangle, including all chunk boundaries and the final edge.
    let vertices: Vec<_> = paths.iter().flat_map(|path| &path.vertices).collect();
    assert_eq!(vertices.len(), edges.len() * 6);
    for (i, rectangle) in vertices.chunks_exact(6).enumerate() {
        let xs: Vec<_> = rectangle.iter().map(|v| v.xy_position.x.as_f32()).collect();
        let ys: Vec<_> = rectangle.iter().map(|v| v.xy_position.y.as_f32()).collect();
        assert_eq!(
            xs.iter().copied().fold(f32::INFINITY, f32::min),
            i as f32 + 2.5
        );
        assert_eq!(
            xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            i as f32 + 3.5
        );
        assert_eq!(ys.iter().copied().fold(f32::INFINITY, f32::min), 7.);
        assert_eq!(ys.iter().copied().fold(f32::NEG_INFINITY, f32::max), 17.);
    }
    let mut empty_paths = 0;
    mesh_edges::paint_edges(&[], point(px(0.), px(0.)), |_| empty_paths += 1);
    assert_eq!(empty_paths, 0);
}
