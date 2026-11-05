//! Production overlay regression tests; no window or benchmark sampling.
#![cfg(target_os = "macos")]
use crate::mesh_edges;
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, PaintSurface, Pixels, Quad, Scene, SurfaceSource, point, px,
    rgba, size,
};
use zipzi_editor_core::{OverlaySnapshot, OverlayVertex, ViewportPoint};
use zipzi_editor_render::{MetalOverlayRenderer, OverlayStyle, OverlayViewport};

fn pt(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
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
                editable: true,
                kind: zipzi_editor_core::OverlayVertexKind::Mesh,
                id: 3,
                position: pt(20.25, 20.5),
                selected: true,
                hovered: true,
            },
            OverlayVertex {
                editable: true,
                kind: zipzi_editor_core::OverlayVertexKind::Mesh,
                id: 9,
                position: pt(24.25, 22.5),
                selected: false,
                hovered: false,
            },
            OverlayVertex {
                editable: true,
                kind: zipzi_editor_core::OverlayVertexKind::Mesh,
                id: 11,
                position: pt(64., 32.),
                selected: false,
                hovered: true,
            },
        ],
        ..Default::default()
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
        restricted: color(0xb0b0b0ff),
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
        for v in [false, true].into_iter().flat_map(|emphasized| {
            overlay
                .vertices
                .iter()
                .filter(move |v| (v.selected || v.hovered) == emphasized)
        }) {
            let r = 3.;
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
            let mut marker = quad(
                b.scale(1.),
                if v.selected { 0xff4060c0 } else { 0xf0f0f0b0 },
                mask.scale(1.),
            );
            marker.corner_radii = gpui_kit::Corners::all(px(r * viewport.scale)).scale(1.);
            if !v.selected || v.hovered {
                marker.border_widths = gpui_kit::Edges::all(px(viewport.scale)).scale(1.);
                marker.border_color = rgba(if v.hovered { 0x62c878ff } else { 0x000000ff }).into();
            }
            scene.insert_primitive(marker);
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

#[path = "../../src/canvas_overlay.rs"]
mod canvas_overlay;

struct ScenePainter {
    scene: Scene,
    scale: f32,
    mask: ContentMask<Pixels>,
}
impl canvas_overlay::Painter for ScenePainter {
    fn quad_layer(&mut self, bounds: Bounds<Pixels>, paint: impl FnOnce(&mut Self)) {
        let clipped = bounds.intersect(&self.mask.bounds);
        if !clipped.is_empty() {
            self.scene.push_layer(clipped.scale(self.scale));
        }
        paint(self);
        if !clipped.is_empty() {
            self.scene.pop_layer();
        }
    }
    fn paint_quad(&mut self, q: gpui_kit::PaintQuad) {
        self.scene.insert_primitive(Quad {
            order: 0,
            border_style: q.border_style,
            bounds: q.bounds.scale(self.scale),
            content_mask: self.mask.scale(self.scale),
            background: q.background,
            border_color: q.border_color,
            corner_radii: q.corner_radii.scale(self.scale),
            border_widths: q.border_widths.scale(self.scale),
        });
    }
    fn paint_path(&mut self, mut path: gpui_kit::Path<Pixels>, color: gpui_kit::Hsla) {
        path.color = color.into();
        path.content_mask = self.mask;
        self.scene.insert_primitive(path.scale(self.scale));
    }
}

#[test]
fn semantic_markers_survive_crossing_lines_and_coincident_glue() -> anyhow::Result<()> {
    use canvas_overlay::Painter;
    use zipzi_editor_core::{
        GluePointState as S, OverlayDeformer, OverlayGlue, OverlayGluePoint, OverlayVertexKind as K,
    };
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut gpui = MetalRenderer::new_headless(Default::default());
        for dark in [false, true] {
            let palette = canvas_overlay::Palette::new(dark);
            let mut painter = ScenePainter {
                scene: Scene::default(),
                scale: 2.,
                mask: ContentMask {
                    bounds: Bounds::new(point(px(0.), px(0.)), size(px(220.), px(100.))),
                },
            };
            painter.paint_quad(gpui_kit::fill(
                painter.mask.bounds,
                if dark {
                    rgba(0x18202cff)
                } else {
                    rgba(0xe5d5c5ff)
                },
            ));
            let overlay = OverlaySnapshot {
                vertices: [
                    (20., K::Mesh),
                    (40., K::Mesh),
                    (60., K::Warp),
                    (100., K::RotationCenter),
                    (20., K::Mesh),
                    (40., K::Warp),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (x, kind))| OverlayVertex {
                    editable: i < 4,
                    id: i as u32,
                    position: pt(x, if i < 4 { 50. } else { 20. }),
                    selected: i == 1,
                    hovered: true,
                    kind,
                })
                .collect(),
                deformers: vec![OverlayDeformer {
                    outline_only: false,
                    kind: zipzi_editor_core::ControlKind::Rotation,
                    lines: vec![(pt(8., 50.), pt(210., 50.))],
                    selected: true,
                    hovered: false,
                }],
                glues: vec![OverlayGlue {
                    points: [
                        (130., 50., S::Editable),
                        (150., 50., S::Restricted),
                        (180., 50., S::Reference),
                        (180., 50., S::Editable),
                        (130., 72., S::Reference),
                        (150., 72., S::Restricted),
                    ]
                    .into_iter()
                    .map(|(x, y, state)| OverlayGluePoint {
                        position: pt(x, y),
                        state,
                        selected: false,
                        hovered: false,
                    })
                    .collect(),
                    pairs: vec![
                        (pt(130., 50.), pt(150., 50.)),
                        (pt(180., 50.), pt(180., 50.)),
                        (pt(130., 72.), pt(150., 72.)),
                    ],
                    selected: true,
                    hovered: false,
                }],
                handles: std::iter::once(zipzi_editor_core::OverlayHandle {
                    tangent: true,
                    rotation: false,
                    editable: true,
                    position: pt(80., 50.),
                    round: true,
                    hovered: false,
                    selected: true,
                })
                .chain(std::iter::once(zipzi_editor_core::OverlayHandle {
                    tangent: false,
                    rotation: false,
                    editable: false,
                    position: pt(70., 20.),
                    round: true,
                    selected: true,
                    hovered: false,
                }))
                .chain([false, true].into_iter().flat_map(|round| {
                    [false, true].into_iter().flat_map(move |selected| {
                        [false, true].into_iter().map(move |hovered| {
                            zipzi_editor_core::OverlayHandle {
                                tangent: false,
                                rotation: false,
                                editable: true,
                                position: pt(
                                    20. + 30. * (u8::from(round) * 2 + u8::from(hovered)) as f32,
                                    if selected { 88. } else { 72. },
                                ),
                                round,
                                selected,
                                hovered,
                            }
                        })
                    })
                }))
                .collect(),
                ..Default::default()
            };
            canvas_overlay::paint_lines(
                &overlay,
                painter.mask.bounds,
                palette,
                false,
                &mut painter,
            );
            canvas_overlay::paint_markers(
                &overlay,
                point(px(0.), px(0.)),
                palette,
                true,
                true,
                &mut painter,
            );
            painter.scene.finish();
            let image = gpui.render_scene_to_image(
                &painter.scene,
                size(DevicePixels(440), DevicePixels(200)),
            )?;
            let rgb = |c: gpui_kit::Hsla| {
                let c: gpui_kit::Rgba = c.into();
                [
                    (c.r * 255.).round() as u8,
                    (c.g * 255.).round() as u8,
                    (c.b * 255.).round() as u8,
                    255,
                ]
            };
            assert_eq!(
                image.get_pixel(40, 100).0,
                rgb(palette.hollow),
                "mesh stays above the line"
            );
            assert_eq!(
                image.get_pixel(44, 100).0,
                rgb(palette.deformer),
                "hovered mesh border uses the shared hover color"
            );
            for x in [80, 84] {
                assert_eq!(
                    image.get_pixel(x, 100).0,
                    if x == 80 {
                        [255, 0, 0, 255]
                    } else {
                        rgb(palette.deformer)
                    },
                    "hover preserves selected fill and changes only its border"
                );
            }
            assert_eq!(
                image.get_pixel(160, 100).0,
                rgb(palette.deformer),
                "selected Bezier handle shares the deformer green"
            );
            assert_eq!(
                image.get_pixel(120, 100).0,
                [255, 255, 255, 255],
                "warp control has a white center even on a dark canvas"
            );
            assert_eq!(
                image.get_pixel(120, 94).0,
                rgb(palette.deformer),
                "hovered Bezier anchors have a green border in either theme"
            );
            for x in [40, 100, 160, 220] {
                assert_eq!(
                    image.get_pixel(x, 144).0,
                    [255, 255, 255, 255],
                    "idle/hovered circular and square handles have white centers"
                );
                assert_eq!(
                    image.get_pixel(x + 4, 144).0,
                    if x == 100 || x == 220 {
                        rgb(palette.deformer)
                    } else {
                        [0, 0, 0, 255]
                    },
                    "hover changes handle border color within its original extent"
                );
                assert_eq!(
                    image.get_pixel(x, 176).0,
                    rgb(palette.deformer),
                    "active circular and square handles have green centers"
                );
                assert_eq!(
                    image.get_pixel(x + 4, 176).0,
                    rgb(palette.deformer),
                    "active circular and square handles have matching fill and border"
                );
            }
            for x in [260, 360] {
                assert_eq!(
                    image.get_pixel(x, 100).0,
                    [255, 255, 255, 255],
                    "Glue retains a white mesh vertex center"
                );
                assert_eq!(
                    image.get_pixel(x + 4, 100).0,
                    rgb(palette.glue),
                    "Glue endpoints use the common six-pixel marker with a yellow border"
                );
            }
            for x in [264, 364] {
                assert_eq!(
                    image.get_pixel(x, 104).0,
                    [255, 208, 0, 255],
                    "Glue has a compact, uniform yellow marker"
                );
            }
            assert_eq!(
                image.get_pixel(304, 104).0,
                rgb(palette.restricted_border),
                "restricted Glue point uses the common restricted border"
            );
            assert_eq!(
                image.get_pixel(300, 99).0,
                [176, 176, 176, 255],
                "restricted Glue point keeps a plain gray center"
            );
            assert_ne!(
                image.get_pixel(260, 144).0,
                [255, 255, 255, 255],
                "reference endpoint has no draggable white center"
            );
            assert_ne!(
                image.get_pixel(265, 148).0,
                [255, 208, 0, 255],
                "reference endpoint uses a muted outline"
            );
            for x in [40, 80, 140] {
                assert_eq!(
                    image.get_pixel(x, 40).0,
                    [176, 176, 176, 255],
                    "restricted ordinary controls and handles have gray centers"
                );
                assert_eq!(
                    image.get_pixel(x + 4, 40).0,
                    if x == 140 {
                        rgb(palette.restricted_border)
                    } else {
                        rgb(palette.deformer)
                    },
                    "restricted controls retain size while hover emphasizes their border"
                );
            }
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../../target/overlay-style-{}.png",
                if dark { "dark" } else { "light" }
            ));
            image.save(path)?;
            // The production Metal mesh pass must match the GPUI fallback's
            // white center, in-place hover border and solid red selection.
            let gpu = gpui.shared_metal_context();
            let mut renderer = MetalOverlayRenderer::new(gpu.device.clone())?;
            let viewport = OverlayViewport {
                extent: [192, 192],
                scale: 2.,
                origin: [0., 0.],
            };
            let texture = renderer
                .draw_pass(
                    &gpu.queue,
                    viewport,
                    &overlay,
                    palette.native(),
                    zipzi_editor_render::OverlayPass::Vertices,
                )?
                .unwrap();
            let metal_image = gpui.render_scene_to_image(
                &make_scene(&overlay, viewport, Some(texture)),
                size(DevicePixels(192), DevicePixels(192)),
            )?;
            assert_eq!(
                metal_image.get_pixel(40, 40).0,
                [176, 176, 176, 255],
                "Metal displays a disabled mesh point in gray"
            );
            assert_eq!(
                metal_image.get_pixel(44, 40).0,
                rgb(palette.deformer),
                "Metal disabled mesh hover border matches the fallback"
            );
            for (x, color) in [
                (40, [255, 255, 255, 255]),
                (44, rgb(palette.deformer)),
                (80, [255, 0, 0, 255]),
                (84, rgb(palette.deformer)),
            ] {
                assert_eq!(
                    metal_image.get_pixel(x, 100).0,
                    color,
                    "Metal mesh marker at {x}"
                );
            }
        }
        Ok(())
    })
}

#[test]
fn mesh_passes_cache_independently_and_exclude_control_points() -> anyhow::Result<()> {
    use zipzi_editor_render::OverlayPass as P;
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let mut renderer = MetalOverlayRenderer::new(gpu.device.clone())?;
        let viewport = OverlayViewport {
            extent: [160, 160],
            scale: 2.,
            origin: [0., 0.],
        };
        let mut overlay = geometry();
        let edges = renderer
            .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Edges)?
            .unwrap();
        let vertices = renderer
            .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Vertices)?
            .unwrap();
        overlay.vertices[0].hovered = false;
        let same_edges = renderer
            .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Edges)?
            .unwrap();
        assert!(std::ptr::eq(edges.as_ref(), same_edges.as_ref()));
        let new_vertices = renderer
            .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Vertices)?
            .unwrap();
        assert!(!std::ptr::eq(vertices.as_ref(), new_vertices.as_ref()));
        overlay.vertices[0].editable = false;
        let restricted = renderer
            .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Vertices)?
            .unwrap();
        assert!(
            !std::ptr::eq(new_vertices.as_ref(), restricted.as_ref()),
            "editability invalidates the vertex texture"
        );
        let unchanged_edges = renderer
            .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Edges)?
            .unwrap();
        assert!(std::ptr::eq(edges.as_ref(), unchanged_edges.as_ref()));
        for v in &mut overlay.vertices {
            v.kind = zipzi_editor_core::OverlayVertexKind::Warp;
        }
        assert!(
            renderer
                .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Vertices)?
                .is_none()
        );
        assert!(
            renderer
                .draw_pass(&gpu.queue, viewport, &overlay, style(), P::Edges)?
                .is_some()
        );
        Ok(())
    })
}

#[test]
fn full_canvas_native_matches_shared_fallback_and_legacy_deformer_styles() -> anyhow::Result<()> {
    use std::sync::Arc;
    use zipzi_editor_core::{
        ControlKind, GluePointState, OverlayDeformer, OverlayGlue, OverlayGluePoint, OverlayHandle,
        OverlayRotationBone,
    };
    use zipzi_editor_render::canvas_overlay::*;
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let mut renderer = MetalOverlayRenderer::new(gpu.device.clone())?;
        let mut overlay = geometry();
        overlay.vertices.extend(
            [
                zipzi_editor_core::OverlayVertexKind::WarpGrid,
                zipzi_editor_core::OverlayVertexKind::Warp,
                zipzi_editor_core::OverlayVertexKind::RotationCenter,
                zipzi_editor_core::OverlayVertexKind::RotationTip,
            ]
            .into_iter()
            .enumerate()
            .flat_map(|(row, kind)| {
                (0..8).map(move |state| OverlayVertex {
                    id: (row * 8 + state) as u32,
                    kind,
                    position: pt(20. + state as f32 * 24., 70. + row as f32 * 18.),
                    editable: state & 4 == 0,
                    selected: state & 1 != 0,
                    hovered: state & 2 != 0,
                })
            }),
        );
        overlay.deformers = vec![OverlayDeformer {
            outline_only: true,
            kind: ControlKind::Warp,
            lines: vec![
                (pt(10., 10.), pt(220., 10.)),
                (pt(220., 10.), pt(220., 140.)),
            ],
            selected: false,
            hovered: false,
        }];
        overlay.bezier_lines = vec![
            (pt(10., 40.), pt(100., 52.)),
            (pt(100., 52.), pt(210., 35.)),
        ];
        overlay.bezier_tangents = vec![(pt(100., 52.), pt(140., 20.))];
        overlay.warp_frame_lines = vec![(pt(6., 6.), pt(225., 6.)), (pt(6., 6.), pt(6., 145.))];
        overlay.rotation_bones = vec![OverlayRotationBone {
            points: [pt(20., 45.), pt(45., 38.), pt(180., 45.), pt(45., 52.)],
            scale: 1.,
            selected: false,
            hovered: false,
        }];
        overlay.handles = vec![OverlayHandle {
            position: pt(140., 20.),
            tangent: true,
            rotation: false,
            round: true,
            selected: true,
            hovered: false,
            editable: true,
        }];
        overlay.glues = vec![OverlayGlue {
            pairs: vec![(pt(20., 155.), pt(180., 155.))],
            points: vec![
                OverlayGluePoint {
                    position: pt(20., 155.),
                    state: GluePointState::Editable,
                    selected: false,
                    hovered: true,
                },
                OverlayGluePoint {
                    position: pt(180., 155.),
                    state: GluePointState::Reference,
                    selected: false,
                    hovered: false,
                },
            ],
            selected: true,
            hovered: false,
        }];
        for dark in [false, true] {
            for scale in [1., 1.5, 2.] {
                let palette = canvas_overlay::Palette::new(dark);
                let origin = point(px(2.25), px(1.375));
                let bounds = Bounds::new(origin, size(px(236.), px(174.)));
                let extent = [
                    ((bounds.right().as_f32() * scale).ceil() - (origin.x.as_f32() * scale).floor())
                        as u32,
                    ((bounds.bottom().as_f32() * scale).ceil()
                        - (origin.y.as_f32() * scale).floor()) as u32,
                ];
                let viewport = OverlayViewport {
                    extent,
                    scale,
                    origin: [origin.x.as_f32(), origin.y.as_f32()],
                };
                let list = Arc::new(CanvasDrawList::new(
                    &overlay,
                    [236., 174.],
                    palette.canvas(),
                    CanvasOptions::default(),
                )?);
                let image = renderer
                    .draw_prepared(&gpu.queue, viewport, list.clone())?
                    .unwrap();
                let painter = || {
                    let mut p = ScenePainter {
                        scene: Scene::default(),
                        scale,
                        mask: ContentMask { bounds },
                    };
                    canvas_overlay::Painter::paint_quad(
                        &mut p,
                        gpui_kit::fill(bounds, rgba(0x18202cff)),
                    );
                    p
                };
                let mut shared = painter();
                canvas_overlay::paint_prepared(&list, origin, true, &mut shared);
                shared.scene.finish();
                let mut legacy = painter();
                mesh_edges::paint_edges(&overlay.lines, origin, |path| {
                    canvas_overlay::Painter::paint_path(
                        &mut legacy,
                        path,
                        palette.mesh.opacity(0.45),
                    )
                });
                canvas_overlay::paint_lines(&overlay, bounds, palette, false, &mut legacy);
                canvas_overlay::paint_markers(&overlay, origin, palette, true, true, &mut legacy);
                legacy.scene.finish();
                let mut native = painter();
                native.scene.insert_primitive(PaintSurface {
                    order: 0,
                    bounds: Bounds::new(
                        point(
                            px((viewport.origin[0] * scale).floor()),
                            px((viewport.origin[1] * scale).floor()),
                        ),
                        size(px(extent[0] as f32), px(extent[1] as f32)),
                    )
                    .scale(1.),
                    content_mask: ContentMask {
                        bounds: bounds.scale(scale),
                    },
                    source: SurfaceSource::MetalTexture(image),
                    opacity: 1.,
                });
                native.scene.finish();
                let target = size(
                    DevicePixels((240. * scale) as i32),
                    DevicePixels((180. * scale) as i32),
                );
                let reference = gpui.render_scene_to_image(&shared.scene, target)?;
                for (name, scene) in [("native", &native.scene), ("legacy", &legacy.scene)] {
                    let actual = gpui.render_scene_to_image(scene, target)?;
                    let sum: u64 = actual
                        .pixels()
                        .zip(reference.pixels())
                        .flat_map(|(a, b)| {
                            a.0.into_iter()
                                .zip(b.0)
                                .map(|(a, b)| u64::from(a.abs_diff(b)))
                        })
                        .sum();
                    let mean = sum as f64 / (actual.width() * actual.height() * 4) as f64;
                    println!(
                        "full canvas {name} dark={dark} dpi={scale} mean_channel_error={mean:.4}"
                    );
                    assert!(mean < 1.5, "{name} image differs: {mean}");
                    if name == "native" && scale == 2. {
                        // A whole-image average hides a translucent border regression
                        // on a six-pixel control. Sample the idle Warp's border itself.
                        let x = ((20. + origin.x.as_f32() - 3.) * scale + 1.) as u32;
                        let y = ((88. + origin.y.as_f32()) * scale) as u32;
                        let a = actual.get_pixel(x, y).0;
                        let b = reference.get_pixel(x, y).0;
                        assert!(
                            a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2),
                            "idle Warp border differs: native={a:?}, GPUI={b:?}"
                        );
                    }
                }
            }
        }
        Ok(())
    })
}

/// CPU paint-boundary comparison, excluding OS input, display and readback.
/// Run explicitly in Release for performance acceptance; debug runs are diagnostic.
#[test]
#[ignore = "explicit canvas CPU benchmark"]
fn native_canvas_cpu_submission_benchmark() -> anyhow::Result<()> {
    use std::time::Instant;
    use zipzi_editor_core::{ControlKind, OverlayDeformer};
    use zipzi_editor_render::canvas_overlay::*;
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let mut renderer = MetalOverlayRenderer::new(gpu.device.clone())?;
        let viewport = OverlayViewport {
            extent: [1024, 1024],
            scale: 2.,
            origin: [0.; 2],
        };
        let palette = canvas_overlay::Palette::new(true);
        let mut rows = Vec::new();
        for count in [256, 2048] {
            let mut overlay = OverlaySnapshot::default();
            let mut cache = CanvasOverlayCache::default();
            for i in 0..count {
                let (x, y) = ((i % 32) as f32 * 15. + 6., (i / 32 % 32) as f32 * 15. + 6.);
                overlay.deformers.push(OverlayDeformer {
                    kind: ControlKind::Warp,
                    outline_only: false,
                    selected: true,
                    hovered: false,
                    lines: vec![
                        (pt(x, y), pt(x + 10., y)),
                        (pt(x + 10., y), pt(x + 10., y + 10.)),
                        (pt(x + 10., y + 10.), pt(x, y + 10.)),
                        (pt(x, y + 10.), pt(x, y)),
                    ],
                });
                overlay.vertices.extend(
                    [(x, y), (x + 10., y), (x, y + 10.), (x + 10., y + 10.)]
                        .into_iter()
                        .enumerate()
                        .map(|(n, (x, y))| OverlayVertex {
                            id: (i * 4 + n) as u32,
                            kind: zipzi_editor_core::OverlayVertexKind::WarpGrid,
                            position: pt(x, y),
                            selected: n == 0,
                            hovered: false,
                            editable: true,
                        }),
                );
            }
            let (mut prepare, mut fallback, mut native, mut legacy) =
                (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for frame in 0..44 {
                overlay.vertices[0].position.x = 6. + frame as f32 * 0.01;
                let start = Instant::now();
                let list = cache.prepare(
                    &overlay,
                    [512.; 2],
                    palette.canvas(),
                    CanvasOptions::default(),
                )?;
                let a = start.elapsed().as_secs_f64() * 1e6;
                let painter = || ScenePainter {
                    scene: Scene::default(),
                    scale: 2.,
                    mask: ContentMask {
                        bounds: Bounds::new(point(px(0.), px(0.)), size(px(512.), px(512.))),
                    },
                };
                let start = Instant::now();
                let mut old = painter();
                canvas_overlay::paint_lines(&overlay, old.mask.bounds, palette, false, &mut old);
                canvas_overlay::paint_markers(
                    &overlay,
                    point(px(0.), px(0.)),
                    palette,
                    true,
                    true,
                    &mut old,
                );
                old.scene.finish();
                let d = start.elapsed().as_secs_f64() * 1e6;
                std::hint::black_box(&old.scene);
                let start = Instant::now();
                let mut shared = painter();
                canvas_overlay::paint_prepared(&list, point(px(0.), px(0.)), true, &mut shared);
                shared.scene.finish();
                let b = start.elapsed().as_secs_f64() * 1e6;
                std::hint::black_box(&shared.scene);
                let start = Instant::now();
                let texture = renderer.draw_prepared(&gpu.queue, viewport, list)?.unwrap();
                let c = start.elapsed().as_secs_f64() * 1e6;
                assert_eq!(renderer.canvas_stats().draw_calls, 1);
                // Drain outside the CPU submission measurement and bound in-flight resources.
                let completed = gpu.queue.new_command_buffer();
                completed.commit();
                completed.wait_until_completed();
                std::hint::black_box(texture);
                if frame >= 4 {
                    prepare.push(a);
                    fallback.push(b);
                    native.push(c);
                    legacy.push(d);
                }
            }
            let quantiles = |mut v: Vec<f64>| {
                v.sort_by(f64::total_cmp);
                serde_json::json!({"p50_us":v[v.len()/2],"p95_us":v[v.len()*95/100]})
            };
            let row = serde_json::json!({"deformers":count,"debug":cfg!(debug_assertions),"shared_prepare":quantiles(prepare),
                "legacy_gpui_emit":quantiles(legacy),"shared_gpui_emit":quantiles(fallback),"native_encode_submit":quantiles(native)});
            println!("{row}");
            rows.push(row);
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/canvas-overlay-cpu.json");
        std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
        Ok(())
    })
}
