use super::*;
use std::sync::Arc;
use zipzi_editor_core::{OverlayGlue, OverlayGluePoint, OverlayRotationBone};
fn p(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
}
#[test]
fn ordering_keeps_bones_below_controls_and_feedback_above_controls() {
    let palette = CanvasPalette::new(true);
    let snapshot = OverlaySnapshot {
        lines: vec![(p(0., 20.), p(100., 20.))],
        rotation_bones: vec![OverlayRotationBone {
            points: [p(10., 20.), p(30., 10.), p(90., 20.), p(30., 30.)],
            scale: 1.,
            selected: true,
            hovered: false,
        }],
        vertices: vec![OverlayVertex {
            id: 1,
            position: p(30., 20.),
            kind: K::Warp,
            selected: false,
            hovered: true,
            editable: true,
        }],
        marquee: Some((p(15., 15.), p(60., 60.))),
        ..Default::default()
    };
    let list =
        CanvasDrawList::new(&snapshot, [100.; 2], palette, CanvasOptions::default()).unwrap();
    let kinds: Vec<_> = list.primitives.iter().map(|p| p.kind()).collect();
    let marker = kinds
        .iter()
        .position(|k| *k == PrimitiveKind::Marker)
        .unwrap();
    assert!(
        kinds[1..marker]
            .iter()
            .all(|k| *k == PrimitiveKind::Triangle)
    );
    assert_eq!(list.primitives[marker].border, palette.deformer);
    assert_eq!(
        list.primitives[marker + 1].color,
        alpha(palette.selection_region, 0.12)
    );
}
#[test]
fn even_odd_lasso_keeps_self_intersection_and_holes() {
    let snapshot = OverlaySnapshot {
        lasso: vec![p(10., 10.), p(90., 90.), p(10., 90.), p(90., 10.)],
        ..Default::default()
    };
    let list = CanvasDrawList::new(
        &snapshot,
        [100.; 2],
        CanvasPalette::new(false),
        CanvasOptions::default(),
    )
    .unwrap();
    let area: f32 = list
        .primitives
        .iter()
        .filter(|p| p.kind() == PrimitiveKind::Triangle)
        .map(|p| {
            let [x, y, a, b] = p.geometry;
            let [_, c, d, _] = p.parameters;
            ((a - x) * (d - y) - (b - y) * (c - x)).abs() / 2.
        })
        .sum();
    assert!(
        (area - 3200.).abs() < 0.01,
        "self-crossing bow tie area={area}"
    );
}
#[test]
fn coincident_glue_uses_editable_selected_endpoint_and_filters_non_finite_points() {
    let palette = CanvasPalette::new(true);
    let snapshot = OverlaySnapshot {
        glues: vec![OverlayGlue {
            pairs: vec![],
            selected: false,
            hovered: false,
            points: vec![
                OverlayGluePoint {
                    position: p(20., 20.),
                    state: GluePointState::Reference,
                    selected: false,
                    hovered: true,
                },
                OverlayGluePoint {
                    position: p(20.2, 20.),
                    state: GluePointState::Editable,
                    selected: true,
                    hovered: false,
                },
                OverlayGluePoint {
                    position: p(f32::NAN, 0.),
                    state: GluePointState::Editable,
                    selected: true,
                    hovered: false,
                },
            ],
        }],
        ..Default::default()
    };
    let list =
        CanvasDrawList::new(&snapshot, [100.; 2], palette, CanvasOptions::default()).unwrap();
    assert_eq!(list.primitives.len(), 1);
    assert_eq!(list.primitives[0].color, palette.mesh_selected);
    assert_eq!(list.primitives[0].border, palette.glue);
}
#[test]
fn unchanged_repaints_reuse_instances_and_hover_view_theme_invalidate() {
    let mut cache = CanvasOverlayCache::default();
    let mut snapshot = OverlaySnapshot {
        vertices: vec![OverlayVertex {
            id: 0,
            kind: K::WarpGrid,
            position: p(20., 20.),
            selected: false,
            hovered: false,
            editable: true,
        }],
        ..Default::default()
    };
    let options = CanvasOptions::default();
    let palette = CanvasPalette::new(true);
    let a = cache
        .prepare(&snapshot, [100.; 2], palette, options)
        .unwrap();
    assert!(Arc::ptr_eq(
        &a,
        &cache
            .prepare(&snapshot, [100.; 2], palette, options)
            .unwrap()
    ));
    snapshot.vertices[0].hovered = true;
    let b = cache
        .prepare(&snapshot, [100.; 2], palette, options)
        .unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert_eq!(a.primitives[0].geometry, b.primitives[0].geometry);
    assert_ne!(a.primitives[0].border, b.primitives[0].border);
    let c = cache
        .prepare(&snapshot, [80.; 2], palette, options)
        .unwrap();
    assert!(!Arc::ptr_eq(&b, &c));
    assert!(!Arc::ptr_eq(
        &c,
        &cache
            .prepare(&snapshot, [80.; 2], CanvasPalette::new(false), options)
            .unwrap()
    ));
}
#[test]
fn dashes_retain_six_pixel_phase_and_bound_offscreen_captures() {
    assert_eq!(
        dashed_segments([0., 10.], [18., 10.], [100.; 2]),
        vec![
            ([0., 10.], [3., 10.]),
            ([6., 10.], [9., 10.]),
            ([12., 10.], [15., 10.])
        ]
    );
    assert!(dashed_segments([-1e8, 20.], [1e8, 20.], [512.; 2]).len() < 100);
    assert!(dashed_segments([f32::NAN, 0.], [10., 0.], [100.; 2]).is_empty());
    assert_eq!(size_of::<CanvasPrimitive>(), 64);
}

#[test]
fn long_offscreen_segments_are_clipped_before_gpu_float_arithmetic() {
    let snapshot = OverlaySnapshot {
        lines: vec![(p(-1e8, 20.), p(1e8, 20.)), (p(f32::NAN, 0.), p(1., 1.))],
        ..Default::default()
    };
    let list = CanvasDrawList::new(
        &snapshot,
        [100.; 2],
        CanvasPalette::new(true),
        CanvasOptions::default(),
    )
    .unwrap();
    assert_eq!(list.primitives.len(), 1);
    assert_eq!(list.primitives[0].geometry, [-1.5, 20., 101.5, 20.]);
    assert_eq!(std::mem::offset_of!(CanvasPrimitive, parameters), 16);
    assert_eq!(std::mem::offset_of!(CanvasPrimitive, color), 32);
    assert_eq!(std::mem::offset_of!(CanvasPrimitive, border), 48);
}
