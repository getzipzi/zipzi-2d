use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

fn grouped_rotation() -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef)> {
    let (mut doc, dir, warp, _, _) = rich_document()?;
    let plan = doc.plan_scene_object_creation(
        SceneObjectType::Rotation,
        "Rotation".into(),
        &[ObjectRef::new(ObjectKind::Deformer, warp)],
    )?;
    let object = doc
        .execute_scene_object_creation(&plan)?
        .suggested_selection[0]
        .clone();
    let mut editor = crate::support::editor_from_document(doc);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 640.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    Ok((editor, dir, object))
}

fn radius(frame: &CanvasFrame, object: &ObjectRef) -> f32 {
    let control = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == *object)
        .unwrap();
    let gizmo = frame
        .geometry
        .deformer_gizmo(control, frame.transform)
        .unwrap();
    (gizmo.bounds[2] - gizmo.bounds[0]) / 2.
}

#[test]
fn scaling_ring_and_handles_follow_each_presented_frame_in_every_viewport() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    let original = shown(&mut editor)?;
    let second = ViewportId::new();
    editor.register_viewport(second, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(
        second,
        ViewportMetrics {
            width: 480.,
            height: 400.,
            scale_factor: 1.,
        },
    );
    let second = editor.viewport_binding(second).unwrap();
    editor.set_viewport_zoom(second, 0.5, None)?;
    let peer = editor.preview_for(second)?.canvas.unwrap();
    assert!(editor.present_frame(peer.clone()));
    let start = editor.overlay().handles[0].position;
    let control = original
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let center = original
        .geometry
        .deformer_gizmo(control, original.transform)
        .unwrap()
        .center;
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    let mut frames = vec![(original, peer)];
    for ratio in [1.5, 2.] {
        pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: rotation_scale_pointer(center, start, ratio),
                left_down: true,
            },
        )?;
        frames.push((
            editor.preview()?.canvas.unwrap(),
            editor.preview_for(second)?.canvas.unwrap(),
        ));
    }
    for (index, (main, peer)) in frames.into_iter().enumerate() {
        if index > 0 {
            assert!(editor.present_frame(main.clone()));
            assert!(editor.present_frame(peer.clone()));
        }
        for frame in [main, peer] {
            let c = frame
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap();
            let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
            let expected = (64. * [1., 1.5, 2.][index] * frame.transform.scale).max(24.);
            assert!((radius(&frame, &object) - expected).abs() < 0.001);
            let overlay = editor.overlay_for_viewport(frame.version.binding);
            let near = |a: ViewportPoint, b: ViewportPoint| {
                assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} vs {b:?}");
            };
            near(overlay.handles[0].position, gizmo.handles[0].1);
            assert_eq!(overlay.rotation_lines.len(), gizmo.outline.len());
            for ((a, b), (c, d)) in overlay.rotation_lines.into_iter().zip(gizmo.outline) {
                near(a, c);
                near(b, d);
            }
        }
    }
    pointer_input(&mut editor, CanvasInput::Cancel)?;
    Ok(())
}

#[test]
fn command_rotation_controls_keep_children_fixed_and_cancel_return_and_undo_restore_all_channels()
-> anyhow::Result<()> {
    for handle in 0..4 {
        let (mut editor, _dir, object) = grouped_rotation()?;
        let frame = shown(&mut editor)?;
        let c = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
        let start = match handle {
            0 => gizmo.center,
            1 => frame.geometry.control_points(c, frame.transform)[1],
            2 => gizmo.outline[12].0,
            _ => gizmo.handles[0].1,
        };
        let end = if handle == 0 {
            ViewportPoint {
                x: start.x + 14.,
                y: start.y + 9.,
            }
        } else if handle == 3 {
            rotation_scale_pointer(gizmo.center, start, 1.5)
        } else {
            let (sin, cos) = 23.4f32.to_radians().sin_cos();
            let (x, y) = (start.x - gizmo.center.x, start.y - gizmo.center.y);
            ViewportPoint {
                x: gizmo.center.x + x * cos - y * sin,
                y: gizmo.center.y + x * sin + y * cos,
            }
        };
        let children = |frame: &CanvasFrame| {
            frame
                .geometry
                .meshes
                .iter()
                .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
                .collect::<Vec<_>>()
        };
        let before = children(&frame);
        let origin = c.points.clone();
        let history = editor.document().unwrap().history_lengths();
        for finish in 0..3 {
            pointer_input(
                &mut editor,
                CanvasInput::Down {
                    point: start,
                    shift: false,
                    alt: false,
                    primary: true,
                },
            )?;
            // Repeated previews must always compensate from the press state.
            for _ in 0..2 {
                pointer_input(
                    &mut editor,
                    CanvasInput::Move {
                        point: end,
                        left_down: true,
                    },
                )?;
                assert!(editor.overlay().deformer_keep_children);
                for (shift, alt) in [(true, false), (true, true), (false, false)] {
                    pointer_input(&mut editor, CanvasInput::ModifiersChanged { shift, alt })?;
                    let after = shown(&mut editor)?;
                    for (a, b) in before.iter().zip(children(&after)) {
                        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
                    }
                    let c = after
                        .geometry
                        .controls
                        .iter()
                        .find(|c| c.object == object)
                        .unwrap();
                    let gizmo = after.geometry.deformer_gizmo(c, after.transform).unwrap();
                    assert_eq!(editor.overlay().handles[0].position, gizmo.handles[0].1);
                }
                let after = shown(&mut editor)?;
                for (a, b) in before.iter().zip(children(&after)) {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
                }
                if finish == 1 {
                    pointer_input(
                        &mut editor,
                        CanvasInput::Move {
                            point: start,
                            left_down: true,
                        },
                    )?;
                }
            }
            pointer_input(
                &mut editor,
                match finish {
                    0 => CanvasInput::Cancel,
                    1 => CanvasInput::Up { point: start },
                    _ => CanvasInput::Up { point: end },
                },
            )?;
            let after = shown(&mut editor)?;
            if finish < 2 {
                assert_eq!(editor.document().unwrap().history_lengths(), history);
                assert_eq!(control_points(&mut editor, &object)?, origin);
                assert_eq!(children(&after), before);
            }
        }
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(children(&shown(&mut editor)?), before);
        assert_eq!(control_points(&mut editor, &object)?, origin);
        editor.execute(EditorCommand::Redo)?;
        let redo = shown(&mut editor)?;
        for (a, b) in before.iter().zip(children(&redo)) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
    }
    Ok(())
}

#[test]
fn ring_uses_a_small_initial_size_and_follows_own_parent_scale_and_zoom() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    let frame = shown(&mut editor)?;
    let control = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let origin = control.points[0];
    let initial = radius(&frame, &object);
    assert!((initial - 64.).abs() < 0.001);
    assert!((origin.x - 128.).abs() < 0.001 && (origin.y - 128.).abs() < 0.001);
    let child = frame
        .geometry
        .controls
        .iter()
        .find(|c| {
            c.kind == ControlKind::Warp && frame.geometry.ancestors(&c.object).contains(&object)
        })
        .unwrap()
        .object
        .clone();
    editor.execute(EditorCommand::AdjustDeformer {
        object: child,
        adjustment: DeformerAdjustment {
            angle: 0.,
            scale: 4.,
            translation: ModelVector::default(),
        },
    })?;
    let larger_children = shown(&mut editor)?;
    let content_radius = larger_children
        .geometry
        .member_meshes(&object)
        .iter()
        .flat_map(|m| &m.vertices)
        .map(|(_, p)| (p.x - origin.x).hypot(p.y - origin.y))
        .fold(0., f32::max);
    assert!(content_radius * frame.transform.scale > initial * 2.);
    assert!((radius(&larger_children, &object) - initial).abs() < 0.001);
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            angle: 37.,
            scale: 2.,
            translation: ModelVector::default(),
        },
    })?;
    let scaled = shown(&mut editor)?;
    assert!((radius(&scaled, &object) - initial * 2.).abs() < 0.001);

    let plan = editor.document().unwrap().plan_scene_object_creation(
        SceneObjectType::Rotation,
        "Parent".into(),
        std::slice::from_ref(&object),
    )?;
    let parent = editor
        .execute_scene_object_creation(editor.active_id().unwrap(), &plan)?
        .suggested_selection[0]
        .clone();
    editor.execute(EditorCommand::AdjustDeformer {
        object: parent,
        adjustment: DeformerAdjustment {
            angle: -23.,
            scale: 1.5,
            translation: ModelVector::default(),
        },
    })?;
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    let inherited = shown(&mut editor)?;
    assert!((radius(&inherited, &object) - initial * 3.).abs() < 0.001);
    for zoom in [MIN_ZOOM, 0.5, 2., MAX_ZOOM] {
        editor.set_zoom_at(zoom, None);
        let zoomed = shown(&mut editor)?;
        assert!((radius(&zoomed, &object) - (initial * 3. * zoom).max(24.)).abs() < 0.002);
    }
    Ok(())
}

#[test]
fn rotation_controls_keep_plain_hover_and_drag_overlays_with_matching_scale_geometry()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
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
    let initial_radius = radius(&frame, &object);
    let start = gizmo.outline[24].0;
    let baseline = editor.overlay();
    let tip = frame.geometry.control_points(control, frame.transform)[1];
    for point in [gizmo.center, tip, gizmo.handles[0].1, start] {
        editor.hover_canvas(frame.version.binding, Some(point))?;
        let hovered = editor.overlay();
        assert!(hovered.active_handle.is_some() || hovered.active_component.is_some());
        assert!(hovered.vertices.iter().all(|v| !v.hovered));
        assert!(hovered.handles.iter().all(|h| !h.hovered));
        assert!(hovered.deformers.iter().all(|d| !d.hovered));
        assert_eq!(hovered.rotation_lines, baseline.rotation_lines);
    }
    assert_eq!(editor.overlay().active_handle, Some(DeformerHandle::Rotate));
    let center = gizmo.center;
    let end = ViewportPoint {
        x: center.x - (start.y - center.y),
        y: center.y + (start.x - center.x),
    };
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    shown(&mut editor)?;
    let overlay = editor.overlay();
    assert_eq!(overlay.rotation_lines.len(), baseline.rotation_lines.len());
    assert_eq!(overlay.guide_lines, baseline.guide_lines);
    assert_eq!(overlay.gizmo_lines, baseline.gizmo_lines);
    assert!(overlay.interaction_label.unwrap().contains("90.0°"));
    pointer_input(&mut editor, CanvasInput::Cancel)?;
    let frame = shown(&mut editor)?;
    assert!((radius(&frame, &object) - initial_radius).abs() < 0.001);
    let start = editor.overlay().handles[0].position;
    let end = rotation_scale_pointer(center, start, 2.);
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    shown(&mut editor)?;
    let handle = editor.overlay().handles[0].position;
    assert!((handle.x - end.x).hypot(handle.y - end.y) < 0.001);
    let index = editor.document().unwrap().object_index();
    assert!(
        (index.get(&object).unwrap().properties["rotation"]["pose"]["scale"]
            .as_f64()
            .unwrap()
            - 2.)
            .abs()
            < 0.001
    );
    pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    Ok(())
}
