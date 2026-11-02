use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

fn mesh_editor() -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef)> {
    let (doc, dir, _, _, _) = rich_document()?;
    let object = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::from_u128(6).to_string());
    let mut editor = crate::support::editor_from_document(doc);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    Ok((editor, dir, object))
}
fn vertices(editor: &mut Editor, object: &ObjectRef) -> anyhow::Result<Vec<ModelPoint>> {
    Ok(shown(editor)?
        .geometry
        .mesh(&object.id)
        .unwrap()
        .vertices
        .iter()
        .map(|(_, p)| *p)
        .collect())
}
fn gesture(
    editor: &mut Editor,
    start: ViewportPoint,
    end: ViewportPoint,
    shift: bool,
    alt: bool,
    cancel: bool,
) -> anyhow::Result<()> {
    pointer_input(
        editor,
        CanvasInput::Down {
            point: start,
            shift,
            alt,
            primary: false,
        },
    )?;
    pointer_input(
        editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    pointer_input(
        editor,
        if cancel {
            CanvasInput::Cancel
        } else {
            CanvasInput::Up { point: end }
        },
    )?;
    Ok(())
}
#[test]
fn mesh_frame_transforms_in_canvas_space_through_a_warp_parent_and_undoes_atomically()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = mesh_editor()?;
    let before = vertices(&mut editor, &object)?;
    assert_eq!(editor.overlay().handles.len(), 9);
    let handles = editor.overlay().handles;
    let start = ViewportPoint {
        x: (handles[0].position.x + handles[1].position.x) / 2.,
        y: handles[0].position.y,
    };
    gesture(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 20.,
            y: start.y + 8.,
        },
        true,
        false,
        false,
    )?;
    let after = vertices(&mut editor, &object)?;
    for (a, b) in before.iter().zip(&after) {
        assert!((b.x - a.x - 20.).abs() < 0.002);
        assert!((b.y - a.y).abs() < 0.002);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(vertices(&mut editor, &object)?, before);
    Ok(())
}
#[test]
fn mesh_frame_option_shift_scaling_uses_center_and_cancel_restores_points() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = mesh_editor()?;
    let before = vertices(&mut editor, &object)?;
    let center = ModelPoint {
        x: (before[0].x + before[2].x) / 2.,
        y: (before[0].y + before[2].y) / 2.,
    };
    for cancel in [true, false] {
        shown(&mut editor)?;
        let start = editor.overlay().handles[4].position;
        let span = (before[2].x - before[0].x) / 2.;
        gesture(
            &mut editor,
            start,
            ViewportPoint {
                x: start.x + span,
                y: start.y + span,
            },
            true,
            true,
            cancel,
        )?;
        let after = vertices(&mut editor, &object)?;
        if cancel {
            assert_eq!(after, before);
        } else {
            for (a, b) in before.iter().zip(&after) {
                assert!((b.x - center.x - 2. * (a.x - center.x)).abs() < 0.002);
                assert!((b.y - center.y - 2. * (a.y - center.y)).abs() < 0.002);
            }
        }
    }
    Ok(())
}
#[test]
fn point_selection_frame_and_nudges_preserve_unselected_vertices_and_locks() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = mesh_editor()?;
    let before = vertices(&mut editor, &object)?;
    let overlay = editor.overlay();
    click(&mut editor, overlay.vertices[0].position, false)?;
    assert!(editor.overlay().warp_frame_lines.is_empty());
    assert!(editor.overlay().handles.is_empty());
    click(&mut editor, overlay.vertices[1].position, true)?;
    assert!(!editor.overlay().warp_frame_lines.is_empty());
    assert_eq!(editor.overlay().handles.len(), 9);
    let binding = editor.primary_viewport_binding().unwrap();
    editor.nudge_selection(binding, ModelVector { x: 0.1, y: 10. })?;
    let after = vertices(&mut editor, &object)?;
    for i in 0..2 {
        assert!((after[i].x - before[i].x - 0.1).abs() < 0.002);
        assert!((after[i].y - before[i].y - 10.).abs() < 0.002);
    }
    assert_eq!(&after[2..], &before[2..]);
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![object.clone()],
        locked: true,
    })?;
    assert!(
        editor
            .nudge_selection(binding, ModelVector { x: 1., y: 0. })
            .is_err()
    );
    assert_eq!(vertices(&mut editor, &object)?, after);
    Ok(())
}

#[test]
fn selection_transform_nudges_respect_each_viewports_isolation() -> anyhow::Result<()> {
    for display in [IsolationDisplay::Dim, IsolationDisplay::Hide] {
        let (mut editor, _dir) = setup()?;
        let objects: Vec<_> = editor
            .layers()
            .iter()
            .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
            .collect();
        let before = [
            vertices(&mut editor, &objects[0])?,
            vertices(&mut editor, &objects[1])?,
        ];
        let binding = editor.primary_viewport_binding().unwrap();
        let other = ViewportId::new();
        editor.register_viewport(other, ViewportKind::Modeling)?;
        editor.set_viewport_metrics(other, editor.canvas_metrics());
        let other = editor.viewport_binding(other).unwrap();
        editor.execute(EditorCommand::SelectObject(objects[0].clone()))?;
        editor.toggle_viewport_isolation(binding)?;
        editor.set_viewport_isolation_display(binding, display)?;
        editor.execute(EditorCommand::SelectObjects(objects.clone()))?;
        editor.nudge_selection(binding, ModelVector { x: 1., y: 0. })?;
        for (a, b) in before[0].iter().zip(vertices(&mut editor, &objects[0])?) {
            assert!((b.x - a.x - 1.).abs() < 0.001);
            assert_eq!(b.y, a.y);
        }
        assert_eq!(vertices(&mut editor, &objects[1])?, before[1]);
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(vertices(&mut editor, &objects[0])?, before[0]);
        editor.execute(EditorCommand::SelectObject(objects[1].clone()))?;
        assert!(
            editor
                .nudge_selection(binding, ModelVector { x: 1., y: 0. })
                .is_err()
        );
        assert_eq!(vertices(&mut editor, &objects[1])?, before[1]);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
        editor.nudge_selection(other, ModelVector { x: 1., y: 0. })?;
        for (a, b) in before[1].iter().zip(vertices(&mut editor, &objects[1])?) {
            assert!((b.x - a.x - 1.).abs() < 0.001);
            assert_eq!(b.y, a.y);
        }
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    }
    Ok(())
}

#[test]
fn selection_transform_hidden_meshes_have_no_handles_or_writes() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let objects: Vec<_> = editor
        .layers()
        .iter()
        .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
        .collect();
    editor.execute(EditorCommand::SelectObject(objects[0].clone()))?;
    let before = [
        vertices(&mut editor, &objects[0])?,
        vertices(&mut editor, &objects[1])?,
    ];
    let handles = editor.overlay().handles;
    let start = ViewportPoint {
        x: (handles[0].position.x + handles[1].position.x) / 2.,
        y: handles[0].position.y,
    };
    editor.execute(EditorCommand::SetObjectVisible {
        object: objects[0].clone(),
        visible: false,
    })?;
    shown(&mut editor)?;
    assert!(editor.overlay().handles.is_empty());
    assert!(editor.overlay().warp_frame_lines.is_empty());
    let history = editor.document().unwrap().history_lengths();
    gesture(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 20.,
            y: start.y,
        },
        false,
        false,
        false,
    )?;
    assert_eq!(vertices(&mut editor, &objects[0])?, before[0]);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    editor.execute(EditorCommand::SelectObjects(objects.clone()))?;
    editor.nudge_selection(
        editor.primary_viewport_binding().unwrap(),
        ModelVector { x: 1., y: 0. },
    )?;
    assert_eq!(vertices(&mut editor, &objects[0])?, before[0]);
    for (a, b) in before[1].iter().zip(vertices(&mut editor, &objects[1])?) {
        assert!((b.x - a.x - 1.).abs() < 0.001);
        assert_eq!(b.y, a.y);
    }
    Ok(())
}

#[test]
fn selection_transform_isolation_keeps_a_selected_child_editable_outside_its_parent_scope()
-> anyhow::Result<()> {
    let (mut editor, _dir, mesh) = mesh_editor()?;
    let before = vertices(&mut editor, &mesh)?;
    let binding = editor.primary_viewport_binding().unwrap();
    editor.toggle_viewport_isolation(binding)?;
    let parent = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(4).to_string());
    let parent_before = control_points(&mut editor, &parent)?;
    editor.execute(EditorCommand::SelectObjects(vec![
        parent.clone(),
        mesh.clone(),
    ]))?;
    editor.nudge_selection(binding, ModelVector { x: 1., y: 0. })?;
    for (a, b) in before.iter().zip(vertices(&mut editor, &mesh)?) {
        assert!((b.x - a.x - 1.).abs() < 0.002);
        assert!((b.y - a.y).abs() < 0.002);
    }
    assert_eq!(control_points(&mut editor, &parent)?, parent_before);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(vertices(&mut editor, &mesh)?, before);
    Ok(())
}

#[test]
fn selection_transform_container_frame_excludes_hidden_contents() -> anyhow::Result<()> {
    let (mut editor, _dir, mesh) = mesh_editor()?;
    let before = vertices(&mut editor, &mesh)?;
    let part = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(3).to_string());
    editor.execute(EditorCommand::SelectObject(part))?;
    shown(&mut editor)?;
    assert_eq!(editor.overlay().handles.len(), 9);
    editor.execute(EditorCommand::SetObjectVisible {
        object: mesh.clone(),
        visible: false,
    })?;
    shown(&mut editor)?;
    assert!(editor.overlay().handles.is_empty());
    assert!(editor.overlay().warp_frame_lines.is_empty());
    let history = editor.document().unwrap().history_lengths();
    assert!(
        editor
            .nudge_selection(
                editor.primary_viewport_binding().unwrap(),
                ModelVector { x: 1., y: 0. }
            )
            .is_err()
    );
    assert_eq!(vertices(&mut editor, &mesh)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    Ok(())
}

#[test]
fn selection_transform_nudges_follow_original_points_mode_and_undo() -> anyhow::Result<()> {
    for original in [false, true] {
        let (mut doc, _dir) = crate::support::rectangle_document(true)?;
        let id = |n| uuid::Uuid::from_u128(n).to_string();
        doc.select(&id(909))?;
        doc.set_preview_parameter(&id(904), 1.)?;
        let mut editor = crate::support::editor_from_document(doc);
        editor.set_canvas_metrics(ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        });
        if original {
            editor.execute(EditorCommand::ToggleOriginalPoints)?;
        }
        shown(&mut editor)?;
        let before = editor.document().unwrap().selected_vertex_positions();
        let p = editor.overlay().vertices[0].position;
        click(&mut editor, p, false)?;
        editor.nudge_selection(
            editor.primary_viewport_binding().unwrap(),
            ModelVector { x: 1., y: 0. },
        )?;
        let after = editor.document().unwrap().selected_vertex_positions();
        let expected = if original { 1. } else { 4. / 3. };
        assert!((after[0].1.x - before[0].1.x - expected).abs() < 0.001);
        assert_eq!(after[0].1.y, before[0].1.y);
        assert_eq!(&after[1..], &before[1..]);
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            before
        );
        editor.execute(EditorCommand::Redo)?;
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            after
        );
    }
    Ok(())
}

#[test]
fn part_frame_moves_its_contents_and_parent_child_selection_moves_only_once() -> anyhow::Result<()>
{
    let (mut editor, _dir, object) = mesh_editor()?;
    let before = vertices(&mut editor, &object)?;
    let part = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(3).to_string());
    editor.execute(EditorCommand::SelectObjects(vec![part, object.clone()]))?;
    shown(&mut editor)?;
    assert_eq!(editor.overlay().handles.len(), 9);
    editor.nudge_selection(
        editor.primary_viewport_binding().unwrap(),
        ModelVector { x: 10., y: 0. },
    )?;
    let after = vertices(&mut editor, &object)?;
    for (a, b) in before.iter().zip(after) {
        assert!((b.x - a.x - 10.).abs() < 0.002);
    }
    Ok(())
}
#[test]
fn rotation_has_only_native_controls_and_shift_snaps_fifteen_degrees() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let canvas = shown(&mut editor)?;
    let control = canvas
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = canvas
        .geometry
        .deformer_gizmo(control, canvas.transform)
        .unwrap();
    let overlay = editor.overlay();
    assert!(overlay.warp_frame_lines.is_empty());
    assert_eq!(overlay.handles.len(), 1);
    assert!(overlay.handles.iter().all(|h| !h.round));
    let tip = canvas.geometry.control_points(control, canvas.transform)[1];
    pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: ViewportPoint {
                x: tip.x + 12.,
                y: tip.y - 12.,
            },
            left_down: false,
        },
    )?;
    assert!(
        editor.overlay().active_handle.is_none(),
        "removed frame corners must not intercept input"
    );
    let center = gizmo.center;
    let start = gizmo.outline[0].0;
    let before = control_points(&mut editor, &object)?;
    let a = -23f32.to_radians();
    let (s, c) = a.sin_cos();
    let (x, y) = (start.x - center.x, start.y - center.y);
    gesture(
        &mut editor,
        start,
        ViewportPoint {
            x: center.x + x * c - y * s,
            y: center.y + x * s + y * c,
        },
        true,
        false,
        false,
    )?;
    let index = editor.document().unwrap().object_index();
    let angle = index.get(&object).unwrap().properties["rotation"]["pose"]["angle"]
        .as_f64()
        .unwrap();
    assert!((angle - 30.).abs() < 0.01, "{angle}");
    assert_eq!(control_points(&mut editor, &object)?[0], before[0]);
    assert!(editor.overlay().warp_frame_lines.is_empty());
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn mixed_mesh_rotation_selection_frame_only_transforms_the_mesh() -> anyhow::Result<()> {
    let (mut editor, _dir, mesh) = mesh_editor()?;
    let before = vertices(&mut editor, &mesh)?;
    let frame_lines = editor.overlay().warp_frame_lines;
    let rotation = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(5).to_string());
    editor.execute(EditorCommand::SelectObjects(vec![
        rotation.clone(),
        mesh.clone(),
    ]))?;
    let rotation_before = control_points(&mut editor, &rotation)?;
    shown(&mut editor)?;
    assert_eq!(editor.overlay().warp_frame_lines, frame_lines);
    let handles = editor.overlay().handles;
    let frame = &handles[handles.len() - 9..];
    let start = ViewportPoint {
        x: (frame[0].position.x + frame[1].position.x) / 2.,
        y: frame[0].position.y,
    };
    gesture(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 10.,
            y: start.y,
        },
        false,
        false,
        false,
    )?;
    for (a, b) in before.iter().zip(vertices(&mut editor, &mesh)?) {
        assert!((b.x - a.x - 10.).abs() < 0.002);
    }
    assert_eq!(control_points(&mut editor, &rotation)?, rotation_before);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn rotation_axis_and_ring_support_coarse_and_fine_steps() -> anyhow::Result<()> {
    for axis in [false, true] {
        for (shift, alt, expected) in [(false, false, 23.4), (true, false, 30.), (true, true, 23.)]
        {
            let (mut editor, _dir, object) = deformer_editor(true)?;
            let canvas = shown(&mut editor)?;
            let control = canvas
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap();
            let gizmo = canvas
                .geometry
                .deformer_gizmo(control, canvas.transform)
                .unwrap();
            let start = if axis {
                canvas.geometry.control_points(control, canvas.transform)[1]
            } else {
                gizmo.outline[0].0
            };
            let center = gizmo.center;
            let (s, c) = (-23.4f32.to_radians()).sin_cos();
            let (x, y) = (start.x - center.x, start.y - center.y);
            gesture(
                &mut editor,
                start,
                ViewportPoint {
                    x: center.x + x * c - y * s,
                    y: center.y + x * s + y * c,
                },
                shift,
                alt,
                false,
            )?;
            let index = editor.document().unwrap().object_index();
            let angle = index.get(&object).unwrap().properties["rotation"]["pose"]["angle"]
                .as_f64()
                .unwrap();
            assert!(
                (angle - expected).abs() < 0.01,
                "axis={axis} shift={shift} alt={alt}: {angle}"
            );
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            assert!(editor.overlay().warp_frame_lines.is_empty());
        }
    }
    Ok(())
}

#[test]
fn rotation_native_scale_steps_and_cancellation_preserve_the_origin() -> anyhow::Result<()> {
    for (shift, alt, cancel, expected) in [
        (true, false, false, 1.237),
        (true, true, false, 1.24),
        (true, true, true, 1.),
    ] {
        let (mut editor, _dir, object) = deformer_editor(true)?;
        let canvas = shown(&mut editor)?;
        let control = canvas
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let origin = control.points[0];
        let gizmo = canvas
            .geometry
            .deformer_gizmo(control, canvas.transform)
            .unwrap();
        let start = gizmo.handles[0].1;
        let center = gizmo.center;
        gesture(
            &mut editor,
            start,
            ViewportPoint {
                x: center.x + (start.x - center.x) * 1.237,
                y: center.y + (start.y - center.y) * 1.237,
            },
            shift,
            alt,
            cancel,
        )?;
        let index = editor.document().unwrap().object_index();
        let scale = index.get(&object).unwrap().properties["rotation"]["pose"]["scale"]
            .as_f64()
            .unwrap();
        assert!((scale - expected).abs() < 0.0001, "{scale}");
        assert_eq!(control_points(&mut editor, &object)?[0], origin);
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (usize::from(!cancel), 0)
        );
    }
    Ok(())
}

#[test]
fn frame_scale_fine_steps_are_uniform_and_centered() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = mesh_editor()?;
    let before = vertices(&mut editor, &object)?;
    let canvas = shown(&mut editor)?;
    let center = ModelPoint {
        x: (before[0].x + before[2].x) / 2.,
        y: (before[0].y + before[2].y) / 2.,
    };
    let start = editor.overlay().handles[4].position;
    let end = ViewportPoint {
        x: start.x + (before[2].x - center.x) * 0.237 * canvas.transform.scale,
        y: start.y + (center.y - before[0].y) * 0.237 * canvas.transform.scale,
    };
    gesture(&mut editor, start, end, true, true, false)?;
    let after = vertices(&mut editor, &object)?;
    for (a, b) in before.iter().zip(after) {
        assert!((b.x - center.x - (a.x - center.x) * 1.24).abs() < 0.002);
        assert!((b.y - center.y - (a.y - center.y) * 1.24).abs() < 0.002);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn rotation_center_and_frame_edge_fine_translation_snap_canvas_units() -> anyhow::Result<()> {
    for rotation in [false, true] {
        let (mut editor, _dir, object) = if rotation {
            deformer_editor(true)?
        } else {
            mesh_editor()?
        };
        let canvas = shown(&mut editor)?;
        let before = if rotation {
            control_points(&mut editor, &object)?
        } else {
            vertices(&mut editor, &object)?
        };
        let start = if rotation {
            canvas.transform.to_viewport(before[0])
        } else {
            let handles = editor.overlay().handles;
            ViewportPoint {
                x: (handles[0].position.x + handles[1].position.x) / 2.,
                y: handles[0].position.y,
            }
        };
        gesture(
            &mut editor,
            start,
            ViewportPoint {
                x: start.x + 17.4 * canvas.transform.scale,
                y: start.y + 4.2 * canvas.transform.scale,
            },
            true,
            true,
            false,
        )?;
        let after = if rotation {
            control_points(&mut editor, &object)?
        } else {
            vertices(&mut editor, &object)?
        };
        for (a, b) in before.iter().zip(after) {
            assert!((b.x - a.x - 17.).abs() < 0.002);
            assert!((b.y - a.y).abs() < 0.002);
        }
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    }
    Ok(())
}

#[test]
fn rotation_axis_and_ring_fine_steps_accumulate_across_full_turns() -> anyhow::Result<()> {
    for axis in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(true)?;
        let canvas = shown(&mut editor)?;
        let control = canvas
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let gizmo = canvas
            .geometry
            .deformer_gizmo(control, canvas.transform)
            .unwrap();
        let start = if axis {
            canvas.geometry.control_points(control, canvas.transform)[1]
        } else {
            gizmo.outline[0].0
        };
        let center = gizmo.center;
        pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: true,
                alt: true,
                primary: false,
            },
        )?;
        let mut end = start;
        for angle in [60f32, 120., 180., 240., 300., 360., 420.4] {
            let (s, c) = (-angle.to_radians()).sin_cos();
            let (x, y) = (start.x - center.x, start.y - center.y);
            end = ViewportPoint {
                x: center.x + x * c - y * s,
                y: center.y + x * s + y * c,
            };
            pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true,
                },
            )?;
        }
        pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        let index = editor.document().unwrap().object_index();
        let angle = index.get(&object).unwrap().properties["rotation"]["pose"]["angle"]
            .as_f64()
            .unwrap();
        assert!((angle - 420.).abs() < 0.01, "axis={axis}: {angle}");
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(
            editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties["rotation"]["pose"]["angle"],
            0.
        );
    }
    Ok(())
}

#[test]
fn frame_rotation_fine_steps_transform_about_the_frozen_center() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = mesh_editor()?;
    let before = vertices(&mut editor, &object)?;
    let canvas = shown(&mut editor)?;
    let center = ModelPoint {
        x: (before[0].x + before[2].x) / 2.,
        y: (before[0].y + before[2].y) / 2.,
    };
    let pivot = canvas.transform.to_viewport(center);
    let start = editor.overlay().handles.last().unwrap().position;
    let (s, c) = (-23.4f32.to_radians()).sin_cos();
    let (x, y) = (start.x - pivot.x, start.y - pivot.y);
    gesture(
        &mut editor,
        start,
        ViewportPoint {
            x: pivot.x + x * c - y * s,
            y: pivot.y + x * s + y * c,
        },
        true,
        true,
        false,
    )?;
    let (s, c) = 23f32.to_radians().sin_cos();
    for (a, b) in before.iter().zip(vertices(&mut editor, &object)?) {
        let (x, y) = (a.x - center.x, a.y - center.y);
        assert!((b.x - center.x - x * c + y * s).abs() < 0.002);
        assert!((b.y - center.y - x * s - y * c).abs() < 0.002);
    }
    Ok(())
}

#[test]
fn native_rotation_scale_in_a_mixed_selection_never_writes_the_mesh() -> anyhow::Result<()> {
    let (mut editor, _dir, mesh) = mesh_editor()?;
    let before = vertices(&mut editor, &mesh)?;
    let rotation = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(5).to_string());
    editor.execute(EditorCommand::SelectObjects(vec![
        mesh.clone(),
        rotation.clone(),
    ]))?;
    let canvas = shown(&mut editor)?;
    let control = canvas
        .geometry
        .controls
        .iter()
        .find(|c| c.object == rotation)
        .unwrap();
    let gizmo = canvas
        .geometry
        .deformer_gizmo(control, canvas.transform)
        .unwrap();
    let start = gizmo.handles[0].1;
    let center = gizmo.center;
    gesture(
        &mut editor,
        start,
        ViewportPoint {
            x: center.x + (start.x - center.x) * 1.237,
            y: center.y + (start.y - center.y) * 1.237,
        },
        true,
        true,
        false,
    )?;
    let index = editor.document().unwrap().object_index();
    assert!(
        (index.get(&rotation).unwrap().properties["rotation"]["pose"]["scale"]
            .as_f64()
            .unwrap()
            - 1.24)
            .abs()
            < 0.0001
    );
    assert_eq!(vertices(&mut editor, &mesh)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn rotation_steps_follow_canvas_motion_under_a_reflected_parent() -> anyhow::Result<()> {
    let (mut document, dir, warp, rotation, _) = rich_document()?;
    let saved = document.save(&dir.path().join("reflection"))?;
    let mut sdk = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::ModelSetup::default(),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let mut parent = sdk.transform(&warp).unwrap();
    for p in &mut parent.warp_mut().unwrap().points {
        p.x = 256. - p.x;
    }
    let mut child = sdk.transform(&rotation).unwrap();
    child.parent_id = Some(warp.clone().into());
    child.rotation_mut().unwrap().pose.origin = zipzi_core::PreciseVec2::new(0.5, 0.5);
    sdk.edit("Reflected fixture", None, |e| {
        e.replace_transform(parent)?;
        e.replace_transform(child)?;
        Ok(())
    })?;
    sdk.save_project(&saved.manifest, None)?;
    for axis in [false, true] {
        let mut editor = crate::support::editor_from_document(Document::open(&saved.manifest)?);
        editor.set_canvas_metrics(ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        });
        let object = ObjectRef::new(ObjectKind::Deformer, &rotation);
        editor.execute(EditorCommand::SelectObject(object.clone()))?;
        let canvas = shown(&mut editor)?;
        let control = canvas
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let gizmo = canvas
            .geometry
            .deformer_gizmo(control, canvas.transform)
            .unwrap();
        let start = if axis {
            canvas.geometry.control_points(control, canvas.transform)[1]
        } else {
            gizmo.outline[0].0
        };
        let center = gizmo.center;
        let (s, c) = (-23.4f32.to_radians()).sin_cos();
        let (x, y) = (start.x - center.x, start.y - center.y);
        gesture(
            &mut editor,
            start,
            ViewportPoint {
                x: center.x + x * c - y * s,
                y: center.y + x * s + y * c,
            },
            true,
            true,
            false,
        )?;
        let angle = editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["rotation"]["pose"]["angle"]
            .as_f64()
            .unwrap();
        assert!((angle + 23.).abs() < 0.01, "axis={axis}: {angle}");
    }
    Ok(())
}
