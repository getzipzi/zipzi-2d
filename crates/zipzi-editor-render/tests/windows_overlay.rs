#![cfg(target_os = "windows")]
use anyhow::{Context, Result};
use gpui::{Bounds, ContentMask, PaintSurface, ScaledPixels, Scene, SurfaceSource, point, size};
use std::sync::Arc;
use windows::Win32::{
    Foundation::HMODULE,
    Graphics::{Direct3D::*, Direct3D11::*},
};
use zipzi_editor_core::{OverlaySnapshot, OverlayVertex, OverlayVertexKind, ViewportPoint};
use zipzi_editor_render::{OverlayPass, OverlayStyle, OverlayViewport, WindowsOverlayRenderer};
fn device() -> Result<ID3D11Device> {
    let mut d = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut d),
            None,
            None,
        )?;
    }
    d.context("D3D11 hardware device")
}
fn bounds(w: u32, h: u32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(0.), ScaledPixels(0.)),
        size(ScaledPixels(w as f32), ScaledPixels(h as f32)),
    )
}
fn scene(image: Arc<gpui::DirectXSurface>) -> Scene {
    let b = bounds(image.extent[0], image.extent[1]);
    let mut s = Scene::default();
    s.insert_primitive(PaintSurface {
        order: 0,
        bounds: b,
        content_mask: ContentMask { bounds: b },
        source: SurfaceSource::DirectX(image),
        opacity: 1.,
    });
    s
}
fn style() -> OverlayStyle {
    OverlayStyle {
        edge: [1., 0., 0., 0.5],
        vertex: [1., 1., 1., 1.],
        selected: [1., 0., 0., 1.],
        restricted: [0.6, 0.6, 0.6, 1.],
    }
}
#[test]
fn mesh_markers_match_metal_coverage_flags_and_fractional_dpi() -> Result<()> {
    let device = device()?;
    let mut renderer = WindowsOverlayRenderer::new(device.clone())?;
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(device)?;
    let vertices: Vec<_> = (0..8)
        .map(|flags| OverlayVertex {
            id: flags,
            position: ViewportPoint {
                x: 10. + (flags % 4) as f32 * 16.,
                y: 10. + (flags / 4) as f32 * 18.,
            },
            selected: flags & 1 != 0,
            hovered: flags & 2 != 0,
            editable: flags & 4 == 0,
            kind: OverlayVertexKind::Mesh,
        })
        .collect();
    let overlay = OverlaySnapshot {
        vertices,
        ..Default::default()
    };
    for scale in [1., 1.5, 2.] {
        let extent = [(76. * scale) as u32, (42. * scale) as u32];
        let viewport = OverlayViewport {
            extent,
            scale,
            origin: [-0.25, 0.375],
        };
        let image = renderer
            .draw_pass(viewport, &overlay, style(), OverlayPass::Vertices)?
            .unwrap();
        assert!(Arc::ptr_eq(
            &image,
            &renderer
                .draw_pass(viewport, &overlay, style(), OverlayPass::Vertices)?
                .unwrap()
        ));
        let pixels = gpui.render(&scene(image), extent, [1.; 4])?;
        // Independent CPU evaluation of the Metal marker fragment contract.
        for v in &overlay.vertices {
            let center = [
                (v.position.x + viewport.origin[0]) * scale - (viewport.origin[0] * scale).floor(),
                (v.position.y + viewport.origin[1]) * scale - (viewport.origin[1] * scale).floor(),
            ];
            let radius = 3. * scale;
            for dy in -7..=7 {
                for dx in -7..=7 {
                    let x = center[0].floor() as i32 + dx;
                    let y = center[1].floor() as i32 + dy;
                    if x < 0 || y < 0 || x >= extent[0] as i32 || y >= extent[1] as i32 {
                        continue;
                    }
                    let d = (x as f32 + 0.5 - center[0]).hypot(y as f32 + 0.5 - center[1]);
                    let coverage = (radius - d + 0.5).clamp(0., 1.);
                    let base = if !v.editable {
                        [0.6, 0.6, 0.6]
                    } else if v.selected {
                        [1., 0., 0.]
                    } else {
                        [1.; 3]
                    };
                    let inner = (radius - scale - d + 0.5).clamp(0., 1.);
                    let mut color = base.map(|c| c * coverage);
                    let mut alpha = coverage;
                    if !v.selected || !v.editable {
                        let border = if !v.editable { 80. / 255. } else { 0. };
                        color =
                            std::array::from_fn(|i| base[i] * inner + border * (coverage - inner));
                    }
                    if v.hovered {
                        let outer = radius + 2. * scale;
                        let ring = (outer - d + 0.5).clamp(0., 1.)
                            * (d - (outer - scale) + 0.5).clamp(0., 1.)
                            * (1. - coverage);
                        for c in &mut color {
                            *c += ring;
                        }
                        alpha += ring;
                    }
                    let actual = pixels.get_pixel(x as u32, y as u32).0;
                    for i in 0..3 {
                        let expected = ((color[i] + 1. - alpha) * 255.).round() as i32;
                        assert!(
                            (actual[i] as i32 - expected).abs() <= 3,
                            "scale={scale} flags={} pixel={x},{y} channel={i} actual={actual:?} expected={expected}",
                            v.id
                        );
                    }
                    assert_eq!(actual[3], 255);
                }
            }
        }
    }
    Ok(())
}
#[test]
fn edge_marker_layers_remain_immutable_and_compose_with_gpui() -> Result<()> {
    let device = device()?;
    let mut r = WindowsOverlayRenderer::new(device.clone())?;
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(device)?;
    let viewport = OverlayViewport {
        extent: [40, 40],
        scale: 1.,
        origin: [0.; 2],
    };
    let overlay = OverlaySnapshot {
        lines: vec![(
            ViewportPoint { x: 2., y: 16. },
            ViewportPoint { x: 36., y: 16. },
        )],
        vertices: vec![OverlayVertex {
            id: 0,
            position: ViewportPoint { x: 16., y: 16. },
            selected: true,
            hovered: false,
            editable: true,
            kind: OverlayVertexKind::Mesh,
        }],
        ..Default::default()
    };
    let edge = r
        .draw_pass(viewport, &overlay, style(), OverlayPass::Edges)?
        .unwrap();
    let marker = r
        .draw_pass(viewport, &overlay, style(), OverlayPass::Vertices)?
        .unwrap();
    let mut composed = scene(edge.clone());
    composed.insert_primitive(gpui::Quad {
        bounds: Bounds::new(
            point(ScaledPixels(0.), ScaledPixels(15.)),
            size(ScaledPixels(40.), ScaledPixels(2.)),
        ),
        content_mask: ContentMask {
            bounds: bounds(40, 40),
        },
        background: gpui::rgba(0x0000ffff).into(),
        ..Default::default()
    });
    composed.insert_primitive(PaintSurface {
        order: 0,
        bounds: bounds(40, 40),
        content_mask: ContentMask {
            bounds: bounds(40, 40),
        },
        source: SurfaceSource::DirectX(marker),
        opacity: 1.,
    });
    let before = gpui.render(&composed, [40, 40], [1.; 4])?;
    assert_eq!(before.get_pixel(16, 16).0, [255, 0, 0, 255]);
    assert_eq!(before.get_pixel(8, 16).0, [0, 0, 255, 255]);
    let edge_pixels = gpui.render(&scene(edge.clone()), [40, 40], [1.; 4])?;
    assert!(edge_pixels.get_pixel(8, 15).0[1] < 250);
    assert_eq!(edge_pixels.get_pixel(8, 8).0, [255; 4]);
    let mut moved = overlay.clone();
    moved.vertices[0].position.x = 30.;
    moved.lines[0].0.y = 8.;
    r.draw_pass(viewport, &moved, style(), OverlayPass::All)?;
    assert_eq!(
        gpui.render(&composed, [40, 40], [1.; 4])?.as_raw(),
        before.as_raw()
    );
    drop(r);
    assert_eq!(
        gpui.render(&composed, [40, 40], [1.; 4])?.as_raw(),
        before.as_raw()
    );
    Ok(())
}

#[test]
fn empty_non_mesh_and_nonfinite_inputs_do_not_publish_mesh_surfaces() -> Result<()> {
    let mut renderer = WindowsOverlayRenderer::new(device()?)?;
    let viewport = OverlayViewport {
        extent: [40, 40],
        scale: 1.,
        origin: [0.; 2],
    };
    let overlay = OverlaySnapshot {
        lines: vec![(
            ViewportPoint { x: f32::NAN, y: 1. },
            ViewportPoint { x: 1., y: 1. },
        )],
        vertices: vec![OverlayVertex {
            id: 0,
            position: ViewportPoint { x: 12., y: 12. },
            selected: true,
            hovered: true,
            editable: true,
            kind: OverlayVertexKind::Warp,
        }],
        ..Default::default()
    };
    assert!(
        renderer
            .draw_pass(viewport, &overlay, style(), OverlayPass::All)?
            .is_none()
    );
    assert!(
        renderer
            .draw_pass(
                OverlayViewport {
                    scale: 0.,
                    ..viewport
                },
                &overlay,
                style(),
                OverlayPass::All
            )
            .is_err()
    );
    Ok(())
}
