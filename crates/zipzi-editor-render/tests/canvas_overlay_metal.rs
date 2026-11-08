#![cfg(target_os = "macos")]
use std::sync::Arc;
use zipzi_editor_core::{OverlaySnapshot, OverlayVertex, OverlayVertexKind as K, ViewportPoint};
use zipzi_editor_render::{MetalOverlayRenderer, OverlayViewport, canvas_overlay::*};
use zipzi_render_metal::metal::*;
fn p(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
}
fn pixels(device: &DeviceRef, queue: &CommandQueueRef, image: &TextureRef) -> Vec<u8> {
    let row = (image.width() * 4).div_ceil(256) * 256;
    let buffer = device.new_buffer(row * image.height(), MTLResourceOptions::StorageModeShared);
    let command = queue.new_command_buffer();
    let blit = command.new_blit_command_encoder();
    blit.copy_from_texture_to_buffer(
        image,
        0,
        0,
        MTLOrigin { x: 0, y: 0, z: 0 },
        MTLSize {
            width: image.width(),
            height: image.height(),
            depth: 1,
        },
        &buffer,
        0,
        row,
        row * image.height(),
        MTLBlitOption::empty(),
    );
    blit.end_encoding();
    command.commit();
    command.wait_until_completed();
    assert_eq!(command.status(), MTLCommandBufferStatus::Completed);
    let data = unsafe {
        std::slice::from_raw_parts(
            buffer.contents().cast::<u8>(),
            (row * image.height()) as usize,
        )
    };
    data.chunks_exact(row as usize)
        .flat_map(|r| r[..image.width() as usize * 4].iter().copied())
        .collect()
}
fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    bytes[((y * width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
fn expected(c: Color) -> [u8; 4] {
    [
        (c[2] * c[3] * 255.).round() as u8,
        (c[1] * c[3] * 255.).round() as u8,
        (c[0] * c[3] * 255.).round() as u8,
        (c[3] * 255.).round() as u8,
    ]
}
#[test]
fn all_control_states_render_and_retained_surfaces_survive_updates() -> anyhow::Result<()> {
    let device = Device::system_default().expect("Metal GPU");
    let queue = device.new_command_queue();
    let mut renderer = MetalOverlayRenderer::new(device.clone())?;
    let kinds = [
        K::Mesh,
        K::WarpGrid,
        K::Warp,
        K::RotationCenter,
        K::RotationTip,
    ];
    let mut overlay = OverlaySnapshot {
        vertices: kinds
            .into_iter()
            .enumerate()
            .flat_map(|(row, kind)| {
                (0..8).map(move |flags| OverlayVertex {
                    id: (row * 8 + flags) as u32,
                    kind,
                    position: p(12. + flags as f32 * 16., 12. + row as f32 * 16.),
                    selected: flags & 1 != 0,
                    hovered: flags & 2 != 0,
                    editable: flags & 4 == 0,
                })
            })
            .collect(),
        ..Default::default()
    };
    for dark in [false, true] {
        let palette = CanvasPalette::new(dark);
        let list = Arc::new(CanvasDrawList::new(
            &overlay,
            [140., 92.],
            palette,
            CanvasOptions::default(),
        )?);
        for scale in [1., 1.5, 2.] {
            let viewport = OverlayViewport {
                extent: [(140. * scale) as u32, (92. * scale) as u32],
                scale,
                origin: [0.25, -0.375],
            };
            let image = renderer
                .draw_prepared(&queue, viewport, list.clone())?
                .unwrap();
            assert_eq!(renderer.canvas_stats().draw_calls, 1);
            assert!(std::ptr::eq(
                image.as_ref(),
                renderer
                    .draw_prepared(&queue, viewport, list.clone())?
                    .unwrap()
                    .as_ref()
            ));
            assert!(renderer.canvas_stats().cached);
            assert_eq!(renderer.canvas_stats().upload_bytes, 0);
            let before = pixels(&device, &queue, &image);
            for v in &overlay.vertices {
                let x = ((v.position.x + viewport.origin[0]) * scale
                    - (viewport.origin[0] * scale).floor())
                .floor() as u32;
                let y = ((v.position.y + viewport.origin[1]) * scale
                    - (viewport.origin[1] * scale).floor())
                .floor() as u32;
                let c = if !v.editable {
                    palette.restricted
                } else if v.kind == K::WarpGrid {
                    if v.selected {
                        palette.hollow
                    } else {
                        let mut c = palette.mesh;
                        c[3] = if v.hovered { 1. } else { 0.6 };
                        c
                    }
                } else if !v.selected {
                    palette.hollow
                } else if v.kind == K::Mesh {
                    palette.mesh_selected
                } else {
                    palette.deformer
                };
                let actual = pixel(&before, viewport.extent[0], x, y);
                assert!(
                    actual
                        .iter()
                        .zip(expected(c))
                        .all(|(a, b)| a.abs_diff(b) <= 1),
                    "kind={:?} scale={scale} got={actual:?} expected={:?}",
                    v.kind,
                    expected(c)
                );
            }
            let mut changed = (*list).clone();
            changed.primitives[0].color = [0., 0., 1., 1.];
            let next = renderer
                .draw_prepared(&queue, viewport, Arc::new(changed))?
                .unwrap();
            assert!(!std::ptr::eq(image.as_ref(), next.as_ref()));
            assert_eq!(before, pixels(&device, &queue, &image));
            let other = device.new_command_queue();
            assert!(!std::ptr::eq(
                image.as_ref(),
                renderer
                    .draw_prepared(&other, viewport, list.clone())?
                    .unwrap()
                    .as_ref()
            ));
        }
    }
    overlay.vertices.clear();
    assert!(
        renderer
            .draw_prepared(
                &queue,
                OverlayViewport {
                    extent: [16, 16],
                    scale: 1.,
                    origin: [0.; 2]
                },
                Arc::new(CanvasDrawList::default())
            )?
            .is_none()
    );
    Ok(())
}
#[test]
fn translucent_marker_border_composites_over_its_fill() -> anyhow::Result<()> {
    let device = Device::system_default().expect("Metal GPU");
    let queue = device.new_command_queue();
    let mut renderer = MetalOverlayRenderer::new(device.clone())?;
    let palette = CanvasPalette::new(false);
    let overlay = OverlaySnapshot {
        vertices: vec![OverlayVertex {
            id: 1,
            kind: K::Warp,
            position: p(12., 12.),
            selected: false,
            hovered: false,
            editable: true,
        }],
        ..Default::default()
    };
    let list = Arc::new(CanvasDrawList::new(
        &overlay,
        [24.; 2],
        palette,
        CanvasOptions::default(),
    )?);
    let image = renderer
        .draw_prepared(
            &queue,
            OverlayViewport {
                extent: [48; 2],
                scale: 2.,
                origin: [0.; 2],
            },
            list,
        )?
        .unwrap();
    let bytes = pixels(&device, &queue, &image);
    // GPUI paints the 65%-opaque idle Warp border over the white marker fill.
    // Check a full-coverage border pixel, not the center or whole-image average.
    let border = palette.deformer.map(|v| v * 0.65 + 0.35);
    let actual = pixel(&bytes, 48, 18, 24);
    assert!(
        actual
            .iter()
            .zip(expected(border))
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "idle Warp border={actual:?}, expected={:?}",
        expected(border)
    );
    Ok(())
}

#[test]
fn bone_alpha_marker_order_and_even_odd_lasso_are_visible() -> anyhow::Result<()> {
    use zipzi_editor_core::OverlayRotationBone;
    let device = Device::system_default().expect("Metal GPU");
    let queue = device.new_command_queue();
    let mut renderer = MetalOverlayRenderer::new(device.clone())?;
    let snapshot = OverlaySnapshot {
        lines: vec![(p(10., 5.5), p(90., 5.5))],
        rotation_bones: vec![OverlayRotationBone {
            points: [p(10., 20.), p(30., 10.), p(90., 20.), p(30., 30.)],
            selected: false,
            hovered: false,
            scale: 1.,
        }],
        vertices: vec![OverlayVertex {
            id: 1,
            kind: K::Warp,
            position: p(30., 20.),
            selected: false,
            hovered: false,
            editable: true,
        }],
        lasso: vec![p(10., 50.), p(90., 90.), p(10., 90.), p(90., 50.)],
        ..Default::default()
    };
    let palette = CanvasPalette::new(false);
    let list = Arc::new(CanvasDrawList::new(
        &snapshot,
        [100.; 2],
        palette,
        CanvasOptions::default(),
    )?);
    let image = renderer
        .draw_prepared(
            &queue,
            OverlayViewport {
                extent: [200, 200],
                scale: 2.,
                origin: [0.; 2],
            },
            list,
        )?
        .unwrap();
    let bytes = pixels(&device, &queue, &image);
    let line = pixel(&bytes, 200, 100, 11);
    assert!(
        line.iter()
            .zip([65_u8, 65, 65, 115])
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "line color/coverage={line:?}"
    );
    assert_eq!(
        pixel(&bytes, 200, 60, 40),
        [255; 4],
        "control center covers translucent bone"
    );
    assert_eq!(
        pixel(&bytes, 200, 100, 40),
        [29, 29, 29, 51],
        "unselected bone retains twenty percent alpha"
    );
    assert!(
        pixel(&bytes, 200, 100, 110)[3] > 0,
        "upper bow tie lobe is filled"
    );
    assert_eq!(
        pixel(&bytes, 200, 40, 140)[3],
        0,
        "outside even-odd bow tie remains transparent"
    );
    Ok(())
}

#[test]
fn weighted_selection_triangle_interpolates_vertex_alpha_on_gpu() -> anyhow::Result<()> {
    let device = Device::system_default().expect("Metal GPU");
    let queue = device.new_command_queue();
    let mut renderer = MetalOverlayRenderer::new(device.clone())?;
    let overlay = OverlaySnapshot {
        selection_weights: Some(zipzi_editor_core::OverlaySelectionWeights {
            color: [1., 0., 0., 0.8],
            points: vec![],
            triangles: vec![([p(8., 8.), p(56., 8.), p(8., 56.)], [1., 0., 0.])],
        }),
        ..Default::default()
    };
    let list = Arc::new(CanvasDrawList::new(
        &overlay,
        [64.; 2],
        CanvasPalette::new(false),
        CanvasOptions {
            lines: false,
            markers: false,
        },
    )?);
    let image = renderer
        .draw_prepared(
            &queue,
            OverlayViewport {
                extent: [64; 2],
                scale: 1.,
                origin: [0.; 2],
            },
            list,
        )?
        .unwrap();
    let bytes = pixels(&device, &queue, &image);
    for (x, y) in [(12, 12), (40, 12)] {
        let weight = 1. - ((x as f32 + 0.5 - 8.) + (y as f32 + 0.5 - 8.)) / 48.;
        let actual = pixel(&bytes, 64, x, y);
        let wanted = expected([1., 0., 0., 0.8 * weight]);
        assert!(
            actual.iter().zip(wanted).all(|(a, b)| a.abs_diff(b) <= 2),
            "({x},{y}): {actual:?} vs {wanted:?}"
        );
    }
    Ok(())
}
