use super::super::support::present_canvas as shown;
use super::common::*;
use zipzi_core::BindingAxisEditing;
use zipzi_editor_core::*;

#[test]
fn rotation_gizmo_retains_arbitrary_angles_at_small_authored_scales_and_large_origins()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let mut angle = 0.;
    for (step, delta) in [13., 7., -3.5, 20.].into_iter().enumerate() {
        editor.execute(EditorCommand::AdjustDeformer {
            object: object.clone(),
            adjustment: DeformerAdjustment {
                translation: if step == 0 {
                    ModelVector {
                        x: 100_000.,
                        y: 100_000.,
                    }
                } else {
                    ModelVector::default()
                },
                angle: delta,
                scale: if step == 0 { 0.0001 } else { 1. },
            },
        })?;
        angle += delta;
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|control| control.object == object)
            .unwrap();
        for zoom in [0.1, 1., 4.] {
            let transform = ViewportTransform {
                scale: zoom,
                offset: ViewportPoint {
                    x: 256. - control.points[0].x * zoom,
                    y: 256. + control.points[0].y * zoom,
                },
                metrics: frame.transform.metrics,
            };
            let points = frame.geometry.control_points(control, transform);
            let dx = points[1].x - points[0].x;
            let dy = points[0].y - points[1].y;
            let gizmo = frame.geometry.deformer_gizmo(control, transform).unwrap();
            let shown_angle = -gizmo.frame_angle.to_degrees();
            assert!(
                (shown_angle - angle).abs() < 0.001,
                "angle {angle}, shown {shown_angle}, zoom {zoom}"
            );
            let radius = (gizmo.handles[0].1.x - gizmo.center.x)
                .hypot(gizmo.handles[0].1.y - gizmo.center.y);
            assert!((dx.hypot(dy) - radius).abs() < 0.001);
            assert!((radius - 128. * 0.0001 * zoom).abs() < 0.00005);
            assert_eq!(
                frame.geometry.hit(
                    points[1],
                    transform,
                    std::slice::from_ref(&object),
                    ToolId::Select,
                    DeformerVisibility::default()
                ),
                // At subpixel sizes the root and tip share the same target;
                // translation keeps precedence over an invisible scale marker.
                Some(HitTarget::Component(ComponentRef {
                    object: object.clone(),
                    id: 0,
                }))
            );
        }
    }
    Ok(())
}

#[test]
fn warp_and_rotation_drags_present_intermediate_geometry_before_the_latest_pose()
-> anyhow::Result<()> {
    for rotation in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(rotation)?;
        let start = editor.overlay().vertices[0].position;
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                primary: false,
                point: start,
                shift: false,
                alt: false,
            },
        )?;
        let mut frames = Vec::new();
        for x in [10., 20., 30.] {
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: ViewportPoint {
                        x: start.x + x,
                        y: start.y,
                    },
                    left_down: true,
                },
            )?;
            frames.push(editor.preview()?.canvas.unwrap());
        }
        assert!(editor.present_frame(frames[0].clone()));
        assert!(editor.has_presented_frame(Some(frames[0].version)));
        let displayed = frames[0]
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let anchor = frames[0].transform.to_viewport(displayed.points[0]);
        assert_eq!(editor.overlay().vertices[0].position, anchor);
        assert!((anchor.x - start.x - 10.).abs() < 0.01);
        assert!(editor.present_frame(frames[1].clone()));
        assert!(!editor.present_frame(frames[0].clone()));
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Up {
                point: ViewportPoint {
                    x: start.x + 30.,
                    y: start.y,
                },
            },
        )?;
        assert!(editor.present_frame(frames[2].clone()));
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        shown(&mut editor)?;
        assert!((editor.overlay().vertices[0].position.x - start.x).abs() < 0.01);
    }
    Ok(())
}

#[test]
fn warp_marquee_preserves_object_and_moves_selected_controls_only() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let p = editor.overlay().vertices[0].position;
    // Cross the grid's first row, starting outside the gizmo.
    drag(
        &mut editor,
        ViewportPoint {
            x: p.x - 35.,
            y: p.y - 10.,
        },
        ViewportPoint {
            x: p.x + 400.,
            y: p.y + 10.,
        },
        false,
    )?;
    assert_eq!(
        editor.document().unwrap().context().selections,
        [object.clone()]
    );
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        4
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    drag(
        &mut editor,
        p,
        ViewportPoint {
            x: p.x + 20.,
            y: p.y + 10.,
        },
        false,
    )?;
    let after = control_points(&mut editor, &object)?;
    for i in 0..2 {
        assert!((after[i].x - before[i].x - 20.).abs() < 0.001);
    }
    assert_eq!(&after[2..], &before[2..]);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    Ok(())
}
#[test]
fn warp_gizmo_scales_all_points_and_cancels_as_one_edit() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let start = gizmo
        .handles
        .iter()
        .find(|(h, _)| *h == DeformerHandle::Scale(4))
        .unwrap()
        .1;
    let end = ViewportPoint {
        x: start.x + 48. * frame.transform.scale,
        y: start.y + 20. * frame.transform.scale,
    };
    assert_eq!(
        frame.geometry.hit(
            start,
            frame.transform,
            &[object.clone()],
            ToolId::Select,
            DeformerVisibility::default()
        ),
        Some(HitTarget::DeformerHandle(
            object.clone(),
            DeformerHandle::Scale(4)
        ))
    );
    drag(&mut editor, start, end, true)?;
    let after = control_points(&mut editor, &object)?;
    assert_eq!(after[2], before[2]);
    assert!((after[1].x - before[2].x - 240.).abs() < 0.001);
    assert!((after[1].y - before[2].y + 240.).abs() < 0.001);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    shown(&mut editor)?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    assert!(editor.overlay().interaction_label.is_some());
    crate::support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    Ok(())
}

#[test]
fn warp_alt_scale_uses_the_frozen_center_under_nested_parent_spaces() -> anyhow::Result<()> {
    for reversed in [false, true] {
        for (handle, uniform) in [(4, false), (4, true), (3, false)] {
            let (mut editor, _dir, object, _) = bound_deformer_editor(false, reversed)?;
            let before = control_points(&mut editor, &object)?;
            let frame = shown(&mut editor)?;
            let control = frame
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap();
            let gizmo = frame
                .geometry
                .deformer_gizmo(control, frame.transform)
                .unwrap();
            let start = gizmo
                .handles
                .iter()
                .find(|(h, _)| *h == DeformerHandle::Scale(handle))
                .unwrap()
                .1;
            let (corner, _) = gizmo.scale_anchor(handle);
            let end = ViewportPoint {
                x: start.x + (corner.x - gizmo.center.x) * 0.5,
                y: start.y
                    + if handle == 3 {
                        18.
                    } else {
                        (corner.y - gizmo.center.y) * 0.25
                    },
            };
            let center = frame.transform.to_model(gizmo.center);
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    primary: false,
                    point: start,
                    shift: uniform,
                    alt: true,
                },
            )?;
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true,
                },
            )?;
            let after = control_points(&mut editor, &object)?;
            let sy = if uniform {
                1.5
            } else if handle == 3 {
                1.
            } else {
                1.25
            };
            for (a, b) in before.iter().zip(&after) {
                assert!((b.x - (center.x + (a.x - center.x) * 1.5)).abs() < 0.02);
                assert!((b.y - (center.y + (a.y - center.y) * sy)).abs() < 0.02);
            }
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            editor.execute(EditorCommand::Undo)?;
            assert_eq!(control_points(&mut editor, &object)?, before);
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    primary: false,
                    point: start,
                    shift: uniform,
                    alt: true,
                },
            )?;
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true,
                },
            )?;
            crate::support::pointer_input(&mut editor, CanvasInput::Cancel)?;
            assert_eq!(control_points(&mut editor, &object)?, before);
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
        }
    }
    Ok(())
}

#[test]
fn warp_shift_click_defers_toggle_and_shift_drag_locks_the_first_canvas_axis() -> anyhow::Result<()>
{
    for reversed in [false, true] {
        for horizontal in [false, true] {
            let (mut editor, _dir, object, _) = bound_deformer_editor(false, reversed)?;
            let before = control_points(&mut editor, &object)?;
            let frame = shown(&mut editor)?;
            let start = editor.overlay().vertices[0].position;
            click(&mut editor, start, false)?;
            let selected = |editor: &Editor| {
                editor
                    .overlay()
                    .vertices
                    .iter()
                    .filter(|v| v.selected)
                    .count()
            };
            assert_eq!(selected(&editor), 1);
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    primary: false,
                    point: start,
                    shift: true,
                    alt: false,
                },
            )?;
            assert_eq!(selected(&editor), 1);
            crate::support::pointer_input(&mut editor, CanvasInput::Up { point: start })?;
            assert_eq!(selected(&editor), 0);
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            click(&mut editor, start, false)?;
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    primary: false,
                    point: start,
                    shift: true,
                    alt: false,
                },
            )?;
            let first = ViewportPoint {
                x: start.x + if horizontal { 12. } else { 4. },
                y: start.y + if horizontal { 4. } else { 12. },
            };
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: first,
                    left_down: true,
                },
            )?;
            let end = ViewportPoint {
                x: start.x + if horizontal { 20. } else { 45. },
                y: start.y + if horizontal { 45. } else { 20. },
            };
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true,
                },
            )?;
            let constrained = ViewportPoint {
                x: if horizontal { end.x } else { start.x },
                y: if horizontal { start.y } else { end.y },
            };
            let expected = frame.transform.to_model(constrained);
            let after = control_points(&mut editor, &object)?;
            assert!((after[0].x - expected.x).abs() < 0.02);
            assert!((after[0].y - expected.y).abs() < 0.02);
            crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
            assert_eq!(selected(&editor), 1);
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            editor.execute(EditorCommand::Undo)?;
            assert_eq!(control_points(&mut editor, &object)?, before);
        }
    }

    Ok(())
}
#[test]
fn warp_outer_edge_moves_without_selecting_descendant_mesh() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let (a, b) = gizmo.outline[0];
    let start = ViewportPoint {
        x: (a.x + b.x) / 2.,
        y: a.y,
    };
    drag(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 20.,
            y: start.y - 16.,
        },
        false,
    )?;
    let after = control_points(&mut editor, &object)?;
    for (a, b) in before.iter().zip(&after) {
        assert!((b.x - a.x - 20.).abs() < 0.001);
        assert!((b.y - a.y - 16.).abs() < 0.001);
    }
    assert_eq!(editor.document().unwrap().context().selections, [object]);
    Ok(())
}
#[test]
fn rotation_scale_handle_and_numeric_adjustment_preserve_pivot_and_history() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let before = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let start = gizmo.handles[0].1;
    let end = rotation_scale_pointer(gizmo.center, start, 2.);
    drag(&mut editor, start, end, false)?;
    let after = control_points(&mut editor, &object)?;
    assert_eq!(before[0], after[0]);
    let index = editor.document().unwrap().object_index();
    assert_eq!(
        index.get(&object).unwrap().properties["rotation"]["pose"]["scale"],
        2.
    );
    editor.execute(EditorCommand::Undo)?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: zipzi_editor_core::ModelVector { x: 10., y: 20. },
            angle: 90.,
            scale: 1.5,
        },
    })?;
    let after = control_points(&mut editor, &object)?;
    assert!((after[0].x - before[0].x - 10.).abs() < 0.001);
    assert!((after[0].y - before[0].y - 20.).abs() < 0.001);
    assert!(
        after[1].y > after[0].y,
        "positive numeric angle turns counterclockwise"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: zipzi_editor_core::ModelVector::default(),
            angle: 0.,
            scale: 1.,
        },
    })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    assert!(
        editor
            .execute(EditorCommand::AdjustDeformer {
                object: object.clone(),
                adjustment: DeformerAdjustment {
                    translation: zipzi_editor_core::ModelVector { x: 10., y: 0. },
                    angle: 0.,
                    scale: 101.
                }
            })
            .is_err()
    );
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert!(!editor.document().unwrap().has_pending_edit());
    Ok(())
}
#[test]
fn delayed_bezier_frames_keep_the_explicit_subdivision_grid() -> anyhow::Result<()> {
    let (mut editor, dir, object) = deformer_editor(false)?;
    explicit_warp_grid(&mut editor, &dir, &object)?;
    editor.execute(EditorCommand::ToggleDeformerGrid)?;
    shown(&mut editor)?;
    let original_grid = editor.overlay().guide_lines;
    assert!(!original_grid.is_empty());
    let start = editor
        .overlay()
        .handles
        .iter()
        .find(|h| h.round)
        .unwrap()
        .position;
    let bent = ViewportPoint {
        x: start.x,
        y: start.y - 40.,
    };
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: bent,
            left_down: true,
        },
    )?;
    let refined = shown(&mut editor)?;
    assert_eq!(
        refined
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .points
            .len(),
        25
    );
    let refined_grid = editor.overlay().guide_lines;
    assert_eq!(refined_grid.len(), original_grid.len());
    assert_ne!(refined_grid, original_grid);

    // Queue another bent pose, then restore the original points before it arrives.
    let delayed = editor.preview()?.canvas.unwrap();
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: start,
            left_down: true,
        },
    )?;
    let restored = editor.preview()?.canvas.unwrap();
    assert_eq!(
        restored
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .points
            .len(),
        25
    );
    assert!(editor.overlay().guide_lines == refined_grid);
    assert!(editor.present_frame(delayed));
    assert!(editor.overlay().guide_lines == refined_grid);
    assert!(editor.present_frame(restored));
    assert!(editor.overlay().guide_lines == original_grid);
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: start })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn subdivisions_are_opt_in_and_never_become_hit_targets() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let frame = shown(&mut editor)?;
    assert!(editor.overlay().guide_lines.is_empty());
    editor.execute(EditorCommand::ToggleDeformerGrid)?;
    assert!(!editor.overlay().guide_lines.is_empty());
    let overlay = editor.overlay();
    let p = overlay
        .guide_lines
        .iter()
        .map(|(a, b)| ViewportPoint {
            x: (a.x + b.x) / 2.,
            y: (a.y + b.y) / 2.,
        })
        .find(|p| {
            overlay
                .vertices
                .iter()
                .map(|v| v.position)
                .chain(overlay.handles.iter().map(|h| h.position))
                .all(|q| (q.x - p.x).hypot(q.y - p.y) > 8.)
        })
        .unwrap();
    let hit = frame.geometry.hit(
        p,
        frame.transform,
        &[object],
        ToolId::Select,
        DeformerVisibility::default(),
    );
    assert!(!matches!(hit, Some(HitTarget::Component(_))));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn unqueried_warp_snapshot_retains_keyform_and_parent_space_after_edits() -> anyhow::Result<()> {
    for reversed in [false, true] {
        let (mut editor, _dir, object, parameter_id) = bound_deformer_editor(false, reversed)?;
        // No overlay or hit query has synthesized the old frame's controller yet.
        let old = shown(&mut editor)?;
        let parent = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(22).to_string());
        let before = old
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .points[0];
        for target in [&parent, &object] {
            editor.execute(EditorCommand::AdjustDeformer {
                object: target.clone(),
                adjustment: DeformerAdjustment {
                    translation: zipzi_editor_core::ModelVector { x: 12., y: 8. },
                    angle: 0.,
                    scale: 1.,
                },
            })?;
        }
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id,
            value: 0.5,
        })?;
        let current = shown(&mut editor)?;
        assert_ne!(
            current
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap()
                .points[0],
            before
        );
        // Late selection of the retained frame must still hit its original anchor.
        assert_eq!(
            old.geometry.hit(
                old.transform.to_viewport(before),
                old.transform,
                std::slice::from_ref(&object),
                ToolId::Select,
                DeformerVisibility::default(),
            ),
            Some(HitTarget::Component(ComponentRef {
                object: object.clone(),
                id: 1 << 31,
            }))
        );
        // At an interpolated parameter value the current frame uses lattice controls.
        let first = editor.overlay().vertices[0].clone();
        assert_eq!(first.id, 0);
    }
    Ok(())
}

#[test]
fn nested_deformer_adjustments_write_only_the_exact_keyform_in_both_canvas_directions()
-> anyhow::Result<()> {
    for rotation in [false, true] {
        for reversed in [false, true] {
            let (mut editor, _dir, object, parameter_id) =
                bound_deformer_editor(rotation, reversed)?;
            let original = control_points(&mut editor, &object)?;
            editor.execute(EditorCommand::AdjustDeformer {
                object: object.clone(),
                adjustment: DeformerAdjustment {
                    translation: zipzi_editor_core::ModelVector { x: 12., y: 8. },
                    angle: 0.,
                    scale: 1.,
                },
            })?;
            let changed = control_points(&mut editor, &object)?;
            for (a, b) in original.iter().zip(&changed) {
                assert!((b.x - a.x - 12.).abs() < 0.01);
                assert!((b.y - a.y - 8.).abs() < 0.01);
            }
            editor.execute(EditorCommand::SetPreviewParameter {
                parameter_id: parameter_id.clone(),
                value: 1.,
            })?;
            assert_eq!(control_points(&mut editor, &object)?, original);
            editor.execute(EditorCommand::SetPreviewParameter {
                parameter_id: parameter_id.clone(),
                value: 0.5,
            })?;
            let interpolation = control_points(&mut editor, &object)?;
            assert!(editor.overlay().unavailable.is_some());
            assert!(editor.overlay().vertices.iter().all(|v| !v.editable));
            assert!(editor.overlay().handles.iter().all(|h| !h.editable));

            assert!(
                editor
                    .execute(EditorCommand::AdjustDeformer {
                        object: object.clone(),
                        adjustment: DeformerAdjustment {
                            translation: zipzi_editor_core::ModelVector::default(),
                            angle: 25.,
                            scale: 1.2
                        }
                    })
                    .is_err()
            );
            assert_eq!(control_points(&mut editor, &object)?, interpolation);
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            assert!(!editor.document().unwrap().has_pending_edit());
            editor.execute(EditorCommand::SetPreviewParameter {
                parameter_id,
                value: 0.,
            })?;
            shown(&mut editor)?;
            editor.execute(EditorCommand::Undo)?;
            assert_eq!(control_points(&mut editor, &object)?, original);
        }
    }
    Ok(())
}
#[test]
fn warp_rotation_handle_crosses_angle_wraps_and_a_full_turn_is_no_history() -> anyhow::Result<()> {
    for reversed in [false, true] {
        let (mut editor, _dir, object, _) = bound_deformer_editor(false, reversed)?;
        let before = control_points(&mut editor, &object)?;
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let gizmo = frame
            .geometry
            .deformer_gizmo(control, frame.transform)
            .unwrap();
        let start = gizmo
            .handles
            .iter()
            .find(|(h, _)| *h == DeformerHandle::Rotate)
            .unwrap()
            .1;
        let center = frame.transform.to_model(gizmo.center);
        let first = frame.transform.to_model(start);
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                primary: false,
                point: start,
                shift: false,
                alt: false,
            },
        )?;
        let mut end = start;
        for step in 1..=12 {
            let angle = (step as f32 * 60.).to_radians();
            let (s, c) = angle.sin_cos();
            end = frame.transform.to_viewport(ModelPoint {
                x: center.x + (first.x - center.x) * c - (first.y - center.y) * s,
                y: center.y + (first.x - center.x) * s + (first.y - center.y) * c,
            });
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true,
                },
            )?;
            let label = editor.overlay().interaction_label.unwrap();
            assert_eq!(label, format!("{:.1}°", step as f32 * 60.));
        }
        crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        assert_eq!(control_points(&mut editor, &object)?, before);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn warp_selected_frame_rotates_and_scales_only_selected_points() -> anyhow::Result<()> {
    for rotate in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(false)?;
        let before = control_points(&mut editor, &object)?;
        let first = editor.overlay().vertices[0].position;
        let second = editor.overlay().vertices[1].position;
        click(&mut editor, first, false)?;
        click(&mut editor, second, true)?;
        let frame = shown(&mut editor)?;
        let overlay = editor.overlay();
        // The selection frame follows its two anchors, rather than the whole lattice.
        let center = ModelPoint {
            x: (before[0].x + before[1].x) / 2.,
            y: (before[0].y + before[1].y) / 2.,
        };
        let viewport_center = frame.transform.to_viewport(center);
        let start = if rotate {
            overlay.handles.last().unwrap().position
        } else {
            overlay.handles[overlay.handles.len() - 1 - 5].position
        };
        let source = frame.transform.to_model(start);
        let end = if rotate {
            frame.transform.to_viewport(ModelPoint {
                x: center.x - (source.y - center.y),
                y: center.y + (source.x - center.x),
            })
        } else {
            ViewportPoint {
                x: start.x + (frame.transform.to_viewport(before[1]).x - viewport_center.x),
                y: start.y,
            }
        };
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                primary: false,
                point: start,
                shift: false,
                alt: !rotate,
            },
        )?;
        crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        let after = control_points(&mut editor, &object)?;
        for (a, b) in before[..2].iter().zip(&after[..2]) {
            let expected = if rotate {
                ModelPoint {
                    x: center.x - (a.y - center.y),
                    y: center.y + (a.x - center.x),
                }
            } else {
                ModelPoint {
                    x: center.x + (a.x - center.x) * 2.,
                    y: a.y,
                }
            };
            assert!(
                (b.x - expected.x).abs() < 0.02,
                "{a:?} -> {b:?}, expected {expected:?}"
            );
            assert!((b.y - expected.y).abs() < 0.02);
        }
        assert_eq!(&after[2..], &before[2..]);
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(control_points(&mut editor, &object)?, before);
    }

    Ok(())
}

#[test]
fn warp_numeric_selection_spans_independent_objects_and_whole_mode_keeps_selection()
-> anyhow::Result<()> {
    let (mut document, dir, warp, _, _) = rich_document()?;
    let saved = document.save(&dir.path().join("independent.zipzi"))?;
    let mut sdk = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::ModelSetup::default(),
    )?;
    sdk.open_project(&saved.path, None)?;
    let mut copy = sdk.transform(&warp).unwrap();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.runtime_id = "SecondWarp".into();
    for point in &mut copy.warp_mut().unwrap().points {
        point.x += 300.;
    }
    let second = ObjectRef::new(ObjectKind::Deformer, &copy.id);
    sdk.edit("Second Warp", None, |edit| edit.create_transform(copy))?;
    sdk.save_project(&saved.path, None)?;
    let first = ObjectRef::new(ObjectKind::Deformer, &warp);
    let mut editor = crate::support::editor_from_document(Document::open(&saved.path)?);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1024.,
        height: 768.,
        scale_factor: 1.,
    });
    editor.execute(EditorCommand::SelectObjects(vec![
        first.clone(),
        second.clone(),
    ]))?;
    let before_a = control_points(&mut editor, &first)?;
    let before_b = control_points(&mut editor, &second)?;
    let frame = shown(&mut editor)?;
    click(&mut editor, frame.transform.to_viewport(before_a[0]), false)?;
    click(&mut editor, frame.transform.to_viewport(before_b[0]), true)?;
    let target = editor.warp_transform_target(&first).unwrap();
    assert_eq!(target.components.len(), 2);
    let center = ModelPoint {
        x: (before_a[0].x + before_b[0].x) / 2.,
        y: (before_a[0].y + before_b[0].y) / 2.,
    };
    editor.execute(EditorCommand::AdjustWarp {
        target: target.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelVector { x: 12., y: 8. },
            angle: 90.,
            scale: 0.5,
        },
    })?;
    let after_a = control_points(&mut editor, &first)?;
    let after_b = control_points(&mut editor, &second)?;
    for (before, after) in [(&before_a, &after_a), (&before_b, &after_b)] {
        assert!((after[0].x - (center.x - (before[0].y - center.y) * 0.5 + 12.)).abs() < 0.02);
        assert!((after[0].y - (center.y + (before[0].x - center.x) * 0.5 + 8.)).abs() < 0.02);
        assert_eq!(&before[1..], &after[1..]);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &first)?, before_a);
    assert_eq!(control_points(&mut editor, &second)?, before_b);
    let frame = shown(&mut editor)?;
    click(&mut editor, frame.transform.to_viewport(before_a[0]), false)?;
    click(&mut editor, frame.transform.to_viewport(before_b[0]), true)?;
    assert_eq!(
        editor.warp_transform_target(&first).unwrap().components,
        target.components
    );
    assert!(
        editor
            .execute(EditorCommand::AdjustWarp {
                target: target.clone(),
                adjustment: DeformerAdjustment {
                    translation: ModelVector::default(),
                    angle: 15.,
                    scale: 1.
                },
            })
            .is_err(),
        "undo must not revive an old numeric destination"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    editor.execute(EditorCommand::ToggleWarpWhole)?;
    assert!(
        editor
            .warp_transform_target(&first)
            .unwrap()
            .components
            .is_empty()
    );
    editor.execute(EditorCommand::AdjustWarp {
        target: editor.warp_transform_target(&first).unwrap(),
        adjustment: DeformerAdjustment {
            translation: ModelVector { x: 5., y: 2. },
            angle: 0.,
            scale: 1.,
        },
    })?;
    let moved = control_points(&mut editor, &first)?;
    for (a, b) in before_a.iter().zip(&moved) {
        assert!((b.x - a.x - 5.).abs() < 0.02 && (b.y - a.y - 2.).abs() < 0.02);
    }
    assert_eq!(control_points(&mut editor, &second)?, before_b);
    editor.execute(EditorCommand::ToggleWarpWhole)?;
    assert_eq!(
        editor.warp_transform_target(&first).unwrap().components,
        target.components
    );
    let stale = editor.warp_transform_target(&first).unwrap();
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![second],
        locked: true,
    })?;
    let history = editor.document().unwrap().history_lengths();
    assert!(
        editor
            .execute(EditorCommand::AdjustWarp {
                target: stale,
                adjustment: DeformerAdjustment {
                    translation: ModelVector { x: 3., y: 2. },
                    angle: 45.,
                    scale: 1.
                }
            })
            .is_err()
    );
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert_eq!(control_points(&mut editor, &first)?, moved);
    Ok(())
}

#[test]
fn warp_numeric_destination_expires_when_preview_parameters_change() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    editor.execute(EditorCommand::CreateParameter(NewParameter::default()))?;
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    let target = editor.warp_transform_target(&object).unwrap();
    let before = control_points(&mut editor, &object)?;
    let history = editor.document().unwrap().history_lengths();
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter,
        value: 0.5,
    })?;
    assert!(
        editor
            .execute(EditorCommand::AdjustWarp {
                target,
                adjustment: DeformerAdjustment {
                    translation: ModelVector { x: 10., y: 0. },
                    angle: 0.,
                    scale: 1.
                },
            })
            .is_err()
    );
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    Ok(())
}

#[test]
fn warp_handle_only_selection_keeps_anchors_and_other_tangents_unchanged() -> anyhow::Result<()> {
    use zipzi_core::WarpBezierEvaluation;
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let initial = editor
        .document()
        .unwrap()
        .object_index()
        .get(&object)
        .unwrap()
        .properties["warp"]
        .clone();
    let warp: zipzi_core::WarpTransform = serde_json::from_value(initial)?;
    let original = warp.bezier.unwrap_or_else(|| {
        zipzi_core::WarpBezier::from_lattice(warp.rows, warp.columns, warp.quad, &warp.points)
    });
    let first = editor.overlay().handles[0].position;
    let second = editor.overlay().handles[1].position;
    click(&mut editor, first, false)?;
    click(&mut editor, second, true)?;
    let target = editor.warp_transform_target(&object).unwrap();
    assert_eq!(target.components.len(), 2);
    assert!(
        target
            .components
            .iter()
            .all(|c| (c.id - (1 << 31)) % 5 != 0)
    );
    editor.execute(EditorCommand::AdjustWarp {
        target: target.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelVector { x: 6., y: 3. },
            angle: 90.,
            scale: 1.,
        },
    })?;
    let changed: zipzi_core::WarpTransform = serde_json::from_value(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]
            .clone(),
    )?;
    let controller = changed.bezier.unwrap();
    for (i, (old, new)) in original.nodes.iter().zip(&controller.nodes).enumerate() {
        assert_eq!(new.position, old.position);
        for h in 0..4 {
            let selected = target
                .components
                .iter()
                .any(|c| c.id == (1 << 31) + (i * 5 + h + 1) as u32);
            if selected {
                assert_ne!(new.handles[h], old.handles[h]);
            } else {
                assert_eq!(new.handles[h], old.handles[h]);
            }
        }
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn numeric_warp_rotation_and_scale_use_displayed_center_under_parent() -> anyhow::Result<()> {
    let (mut editor, _dir, object, _) = bound_deformer_editor(false, true)?;
    let before = control_points(&mut editor, &object)?;
    let center = ModelPoint {
        x: (before[0].x + before[3].x) / 2.,
        y: (before[0].y + before[3].y) / 2.,
    };
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: zipzi_editor_core::ModelVector::default(),
            angle: 90.,
            scale: 0.5,
        },
    })?;
    let after = control_points(&mut editor, &object)?;
    for (a, b) in before.iter().zip(&after) {
        assert!((b.x - (center.x - (a.y - center.y) * 0.5)).abs() < 0.01);
        assert!((b.y - (center.y + (a.x - center.x) * 0.5)).abs() < 0.01);
    }
    Ok(())
}

#[test]
fn rotation_gizmo_scales_from_a_small_initial_size_with_matching_pick_geometry()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let original = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let scene = &frame.geometry;
    let control = scene.controls.iter().find(|c| c.object == object).unwrap();
    for width in [256., 512.] {
        for zoom in [0.5, 1., 2.] {
            for dpi in [1., 2.] {
                let transform = ViewportTransform::new(
                    ViewportMetrics {
                        width,
                        height: width,
                        scale_factor: dpi,
                    },
                    ViewState {
                        zoom,
                        pan: ViewportPoint { x: 31., y: -19. },
                    },
                )
                .unwrap();
                let points = scene.control_points(control, transform);
                let gizmo = scene.deformer_gizmo(control, transform).unwrap();
                let radius = |p: ViewportPoint| (p.x - gizmo.center.x).hypot(p.y - gizmo.center.y);
                let expected = (128. * zoom).max(32.);
                assert!((radius(points[1]) - expected).abs() < 0.001);
                assert!((radius(gizmo.outline[2].0) - expected).abs() < 0.001);
                assert!((radius(gizmo.handles[0].1) - expected).abs() < 0.001);
                let selection = std::slice::from_ref(&object);
                let hit = |p| {
                    scene.hit(
                        p,
                        transform,
                        selection,
                        ToolId::Select,
                        DeformerVisibility::default(),
                    )
                };
                assert_eq!(
                    hit(points[1]),
                    Some(HitTarget::DeformerHandle(
                        object.clone(),
                        DeformerHandle::RotationScale
                    ))
                );
                assert_eq!(
                    hit(gizmo.handles[0].1),
                    Some(HitTarget::DeformerHandle(
                        object.clone(),
                        DeformerHandle::RotationScale
                    ))
                );
                // The filled bone body rotates; the tip scales.
                assert_eq!(
                    hit(rotation_body_pointer(gizmo.center, gizmo.handles[0].1)),
                    Some(HitTarget::RotationHandle(object.clone()))
                );
            }
        }
    }
    editor.zoom_by(1.);
    assert_eq!(control_points(&mut editor, &object)?, original);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn undriven_warp_binding_keeps_identity_during_control_drag() -> anyhow::Result<()> {
    use zipzi_sdk::{
        BindingAuthoringEdit as E, BindingGridEdit as G, BindingGridTarget as T, GridOperation as O,
    };
    for pending_axis in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(false)?;
        let document = editor.active_id().unwrap();
        let binding = uuid::Uuid::new_v4().to_string();
        let plan = editor.plan_binding_authoring(
            document,
            E::Grid(G::Create {
                id: binding.clone(),
                target: T::Scene(object.id.clone()),
            }),
        )?;
        editor.execute_binding_authoring(document, &plan)?;
        let axis = zipzi_core::BindingAxis::new("", vec![-1., 1.]);
        if pending_axis {
            let plan = editor.plan_binding_authoring(
                document,
                E::Grid(G::Edit {
                    owner: binding.clone(),
                    operation: O::AddAxis { axis: axis.clone() },
                }),
            )?;
            editor.execute_binding_authoring(document, &plan)?;
        }
        let before = control_points(&mut editor, &object)?;
        let start = editor.overlay().vertices[0].position;
        let history = editor.document().unwrap().history_lengths().0;
        drag(
            &mut editor,
            start,
            ViewportPoint {
                x: start.x + 20.,
                y: start.y,
            },
            false,
        )?;
        let after = control_points(&mut editor, &object)?;
        assert!((after[0].x - before[0].x - 20.).abs() < 0.001);
        assert_eq!(editor.document().unwrap().history_lengths().0, history + 1);
        let check = |editor: &Editor| {
            let index = editor.document().unwrap().object_index();
            assert!(
                index
                    .get(&ObjectRef::new(ObjectKind::SceneBinding, &binding))
                    .is_some()
            );
            if pending_axis {
                assert!(
                    index
                        .get(&ObjectRef::new(ObjectKind::BindingAxis, &axis.id))
                        .is_some()
                );
            }
        };
        check(&editor);
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(control_points(&mut editor, &object)?, before);
        check(&editor);
        editor.execute(EditorCommand::Redo)?;
        assert_eq!(control_points(&mut editor, &object)?, after);
        check(&editor);
    }
    Ok(())
}
