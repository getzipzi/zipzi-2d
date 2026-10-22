use super::super::support::present_canvas as shown;
use super::common::*;
use editor_core::*;

#[test]
fn warp_and_rotation_drags_present_intermediate_geometry_before_the_latest_pose()
-> anyhow::Result<()> {
    for rotation in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(rotation)?;
        let start = editor.overlay().vertices[0].position;
        editor.canvas_input(CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
        })?;
        let mut frames = Vec::new();
        for x in [10., 20., 30.] {
            editor.canvas_input(CanvasInput::Move {
                point: ViewportPoint {
                    x: start.x + x,
                    y: start.y,
                },
                left_down: true,
            })?;
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
        editor.canvas_input(CanvasInput::Up {
            point: ViewportPoint {
                x: start.x + 30.,
                y: start.y,
            },
        })?;
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
        2
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
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
        alt: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: end,
        left_down: true,
    })?;
    assert!(editor.overlay().interaction_label.is_some());
    editor.canvas_input(CanvasInput::Cancel)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
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
    let end = ViewportPoint {
        x: gizmo.center.x + 2. * (start.x - gizmo.center.x),
        y: gizmo.center.y + 2. * (start.y - gizmo.center.y),
    };
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
            translation: editor_core::ModelVector { x: 10., y: 20. },
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
            translation: editor_core::ModelVector::default(),
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
                    translation: editor_core::ModelVector { x: 10., y: 0. },
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
fn delayed_bezier_frames_keep_their_own_subdivision_grid() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
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
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
        alt: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: bent,
        left_down: true,
    })?;
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
    assert!(refined_grid.len() > original_grid.len());

    // Queue another refined pose, then restore the legacy lattice before it arrives.
    let delayed = editor.preview()?.canvas.unwrap();
    editor.canvas_input(CanvasInput::Move {
        point: start,
        left_down: true,
    })?;
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
        4
    );
    assert!(editor.overlay().guide_lines == refined_grid);
    assert!(editor.present_frame(delayed));
    assert!(editor.overlay().guide_lines == refined_grid);
    assert!(editor.present_frame(restored));
    assert!(editor.overlay().guide_lines == original_grid);
    editor.canvas_input(CanvasInput::Up { point: start })?;
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
                    translation: editor_core::ModelVector { x: 12., y: 8. },
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
                    translation: editor_core::ModelVector { x: 12., y: 8. },
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
            assert!(
                editor
                    .execute(EditorCommand::AdjustDeformer {
                        object: object.clone(),
                        adjustment: DeformerAdjustment {
                            translation: editor_core::ModelVector::default(),
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
            translation: editor_core::ModelVector::default(),
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
fn rotation_gizmo_follows_canvas_scale_with_matching_pick_geometry() -> anyhow::Result<()> {
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
                assert!((radius(points[1]) - 540. * zoom).abs() < 0.001);
                assert!((radius(gizmo.outline[0].0) - 280. * zoom).abs() < 0.001);
                assert!((radius(gizmo.handles[0].1) - 400. * zoom).abs() < 0.001);
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
                    Some(HitTarget::RotationHandle(object.clone()))
                );
                assert_eq!(
                    hit(gizmo.handles[0].1),
                    Some(HitTarget::DeformerHandle(
                        object.clone(),
                        DeformerHandle::RotationScale
                    ))
                );
                // The opposite quadrant of the scaled ring is clear of the axes.
                assert_eq!(
                    hit(gizmo.outline[30].0),
                    Some(HitTarget::DeformerHandle(
                        object.clone(),
                        DeformerHandle::Rotate
                    ))
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
    use kasane_sdk::{
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
        let axis = kasane_core::BindingAxis::new("", vec![-1., 1.]);
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
