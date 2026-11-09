use super::common::*;
use crate::support::{pointer_input as input, present_canvas as shown};
use std::time::Duration;
use zipzi_editor_core::*;

fn p(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
}
fn configure(editor: &mut Editor) -> anyhow::Result<()> {
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushMode(BrushMode::Inflate))?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 80.,
            weight: 1.,
            hardness: 1.,
            ..Default::default()
        },
    ))
}
fn down(editor: &mut Editor, point: ViewportPoint, shift: bool) -> anyhow::Result<()> {
    assert!(!input(
        editor,
        CanvasInput::Down {
            point,
            shift,
            alt: false,
            primary: false
        }
    )?);
    Ok(())
}
fn hold(editor: &mut Editor, ticks: u32, total: Duration) -> anyhow::Result<()> {
    for i in 0..ticks {
        input(
            editor,
            CanvasInput::BrushTick {
                elapsed: total * (i + 1) / ticks - total * i / ticks,
            },
        )?;
    }
    Ok(())
}
fn weight_points(editor: &Editor) -> Vec<(ViewportPoint, f32)> {
    editor
        .overlay()
        .selection_weights
        .map_or_else(Vec::new, |w| {
            w.points.into_iter().map(|(p, w, _)| (p, w)).collect()
        })
}

#[test]
fn inflation_stationary_hold_is_time_based_and_commits_once_with_undo_redo() -> anyhow::Result<()> {
    let mut results = Vec::new();
    for rate in [30, 60, 120] {
        let (mut editor, _dir) = setup()?;
        let vertices = editor.overlay().vertices;
        configure(&mut editor)?;
        let before = editor.document().unwrap().selected_vertex_positions();
        let center = p(vertices[0].position.x - 20., vertices[0].position.y);
        down(&mut editor, center, false)?;
        assert_eq!(
            editor.overlay().active_gesture,
            Some(CanvasGestureKind::BrushInflate)
        );
        assert!(!editor.document().unwrap().has_pending_edit());
        hold(&mut editor, rate, Duration::from_secs(1))?;
        let after = editor.document().unwrap().selected_vertex_positions();
        assert!(after[0].1.x > before[0].1.x + 5.);
        assert!(after[0].1.x < before[0].1.x + 20.);
        assert_eq!(after[0].1.y, before[0].1.y);
        for (i, vertex) in vertices.iter().enumerate().skip(1) {
            if (vertex.position.x - center.x).hypot(vertex.position.y - center.y) >= 40. {
                assert_eq!(after[i], before[i]);
            }
        }
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        input(&mut editor, CanvasInput::Up { point: center })?;
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        assert!(!editor.document().unwrap().has_pending_edit());
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
        results.push(after.iter().map(|p| p.1).collect::<Vec<_>>());
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
    Ok(())
}

#[test]
fn inflation_shift_switches_direction_live_and_motion_reaches_new_points_without_painting()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let vertices = editor.overlay().vertices;
    configure(&mut editor)?;
    let before = editor.document().unwrap().selected_vertex_positions();
    let first = p(vertices[0].position.x - 20., vertices[0].position.y);
    down(&mut editor, first, false)?;
    hold(&mut editor, 10, Duration::from_millis(500))?;
    let inflated = editor.document().unwrap().selected_vertex_positions();
    input(
        &mut editor,
        CanvasInput::ModifiersChanged {
            shift: true,
            alt: false,
        },
    )?;
    assert!(editor.overlay().selection_brush.unwrap().subtract);
    hold(&mut editor, 10, Duration::from_millis(500))?;
    assert!(
        editor.document().unwrap().selected_vertex_positions()[0]
            .1
            .x
            < inflated[0].1.x
    );
    let second = p(vertices[1].position.x - 20., vertices[1].position.y);
    input(
        &mut editor,
        CanvasInput::Move {
            point: second,
            left_down: true,
        },
    )?;
    hold(&mut editor, 10, Duration::from_millis(500))?;
    let after = editor.document().unwrap().selected_vertex_positions();
    assert!(after[1].1.x < before[1].1.x);
    assert!(weight_points(&editor).is_empty());
    input(&mut editor, CanvasInput::Up { point: second })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn inflation_cancellation_and_mode_switch_restore_geometry_and_old_painted_weights()
-> anyhow::Result<()> {
    for cancellation in 0..7 {
        let (mut editor, _dir) = setup()?;
        let vertices = editor.overlay().vertices;
        editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
        editor.execute(EditorCommand::SetBrushSelectionSettings(
            BrushSelectionSettings {
                diameter: 8.,
                weight: 0.6,
                ..Default::default()
            },
        ))?;
        down(&mut editor, vertices[2].position, false)?;
        input(
            &mut editor,
            CanvasInput::Up {
                point: vertices[2].position,
            },
        )?;
        let old_weights = weight_points(&editor);
        configure(&mut editor)?;
        let before = editor.document().unwrap().selected_vertex_positions();
        let center = p(vertices[0].position.x - 20., vertices[0].position.y);
        down(&mut editor, center, false)?;
        hold(&mut editor, 2, Duration::from_millis(100))?;
        assert_ne!(
            editor.document().unwrap().selected_vertex_positions(),
            before
        );
        match cancellation {
            0 => {
                input(&mut editor, CanvasInput::Cancel)?;
            }
            1 => {
                input(
                    &mut editor,
                    CanvasInput::Move {
                        point: center,
                        left_down: false,
                    },
                )?;
            }
            2 => editor.execute(EditorCommand::SetTool(ToolId::Select))?,
            3 => editor.execute(EditorCommand::SetBrushMode(BrushMode::Select))?,
            4 => editor.set_canvas_metrics(ViewportMetrics {
                width: 600.,
                height: 512.,
                scale_factor: 2.,
            }),
            5 => {
                assert!(
                    input(
                        &mut editor,
                        CanvasInput::Move {
                            point: p(f32::NAN, 1.),
                            left_down: true
                        }
                    )
                    .is_err()
                );
            }
            _ => {
                input(&mut editor, CanvasInput::Up { point: center })?;
            }
        }
        assert!(!editor.gesture_active());
        if cancellation != 6 {
            assert_eq!(
                editor.document().unwrap().selected_vertex_positions(),
                before
            );
        }
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (usize::from(cancellation == 6), 0)
        );
        shown(&mut editor)?;
        let weights: Vec<_> = weight_points(&editor).iter().map(|p| p.1).collect();
        assert_eq!(weights, old_weights.iter().map(|p| p.1).collect::<Vec<_>>());
    }
    Ok(())
}

#[test]
fn inflation_noops_and_long_holds_stay_finite_and_leave_the_brush_center_fixed()
-> anyhow::Result<()> {
    for case in 0..5 {
        let (mut editor, _dir) = setup()?;
        let v = editor.overlay().vertices[0].clone();
        configure(&mut editor)?;
        let before = editor.document().unwrap().selected_vertex_positions();
        let center = match case {
            0 | 1 => v.position,
            2 => p(v.position.x - 40., v.position.y),
            _ => p(-1000., -1000.),
        };
        if case == 1 {
            editor.execute(EditorCommand::SetBrushSelectionSettings(
                BrushSelectionSettings {
                    weight: 0.,
                    ..editor.brush_selection_settings()
                },
            ))?;
        }
        down(&mut editor, center, false)?;
        hold(&mut editor, 10, Duration::from_millis(500))?;
        input(&mut editor, CanvasInput::Up { point: center })?;
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            before
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    for shrink in [false, true] {
        let (mut editor, _dir) = setup()?;
        let v = editor.overlay().vertices[0].clone();
        configure(&mut editor)?;
        let center = p(v.position.x - 20., v.position.y);
        down(&mut editor, center, shrink)?;
        hold(&mut editor, 100, Duration::from_secs(10))?;
        let frame = shown(&mut editor)?;
        let model = editor.document().unwrap().selected_vertex_positions()[0].1;
        let at = frame.transform.to_viewport(ModelPoint {
            x: model.x,
            y: model.y,
        });
        assert!(at.x.is_finite() && at.y.is_finite());
        assert!(at.x > center.x && at.x < center.x + 40.);
        input(&mut editor, CanvasInput::Cancel)?;
    }
    Ok(())
}

#[test]
fn inflation_warp_edits_only_lattice_and_preserves_controller_and_old_selection()
-> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    let object = ObjectRef::new(ObjectKind::Deformer, warp);
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    configure(&mut editor)?;
    let v = editor
        .overlay()
        .vertices
        .iter()
        .find(|v| v.kind == OverlayVertexKind::WarpGrid)
        .unwrap()
        .clone();
    let properties = |editor: &Editor| {
        editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]
            .clone()
    };
    let before = properties(&editor);
    let center = p(v.position.x - 20., v.position.y);
    down(&mut editor, center, false)?;
    hold(&mut editor, 10, Duration::from_millis(500))?;
    input(&mut editor, CanvasInput::Up { point: center })?;
    let after = properties(&editor);
    assert_ne!(after, before);
    assert_eq!(after["bezier"], before["bezier"]);
    assert_eq!(after["rows"], before["rows"]);
    assert_eq!(after["columns"], before["columns"]);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(weight_points(&editor).is_empty());
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(properties(&editor), before);
    Ok(())
}

#[test]
fn inflation_footprint_respects_shape_angle_strength_zoom_and_dpi() -> anyhow::Result<()> {
    for (shape, angle, offset, affected) in [
        (BrushShape::Circle, 0., p(50., 0.), false),
        (BrushShape::Square, 45., p(50., 0.), true),
        (BrushShape::Line, 0., p(20., 0.), true),
        (BrushShape::Line, 90., p(20., 0.), false),
        (BrushShape::Line, 90., p(0., 20.), true),
    ] {
        let mut displacements = Vec::new();
        for (zoom, dpi, strength) in [(1.37, 1.5, 1.), (2., 2., 1.), (2., 1., 0.25)] {
            let (mut editor, _dir) = setup()?;
            editor.set_canvas_metrics(ViewportMetrics {
                width: 512.,
                height: 512.,
                scale_factor: dpi,
            });
            editor.set_zoom_at(zoom, Some(p(256., 256.)));
            let frame = shown(&mut editor)?;
            configure(&mut editor)?;
            editor.execute(EditorCommand::SetBrushSelectionSettings(
                BrushSelectionSettings {
                    shape,
                    angle,
                    weight: strength,
                    ..editor.brush_selection_settings()
                },
            ))?;
            let vertex = editor.overlay().vertices[0].position;
            let all_vertices = editor.overlay().vertices;
            let before = editor.document().unwrap().selected_vertex_positions();
            let center = p(vertex.x - offset.x, vertex.y - offset.y);
            down(&mut editor, center, false)?;
            hold(&mut editor, 10, Duration::from_millis(500))?;
            let actual = editor.document().unwrap().selected_vertex_positions()[0].1;
            let actual = frame.transform.to_viewport(ModelPoint {
                x: actual.x,
                y: actual.y,
            });
            let movement = (actual.x - vertex.x).hypot(actual.y - vertex.y);
            assert_eq!(
                movement > 0.001,
                affected,
                "{shape:?}, angle {angle}, zoom {zoom}"
            );
            displacements.push(movement);
            let after = editor.document().unwrap().selected_vertex_positions();
            for (i, vertex) in all_vertices.iter().enumerate() {
                if editor
                    .brush_selection_settings()
                    .influence(vertex.position, center, center)
                    == 0.
                {
                    assert_eq!(
                        after[i], before[i],
                        "Outside points must remain exact at fractional zoom"
                    );
                }
            }
            input(&mut editor, CanvasInput::Cancel)?;
        }
        assert!((displacements[0] - displacements[1]).abs() < 0.001);
        if affected {
            assert!(displacements[2] < displacements[1]);
        }
    }
    Ok(())
}

#[test]
fn inflation_only_authors_the_active_keyform_and_skips_non_editable_poses() -> anyhow::Result<()> {
    let (doc, _dir) = crate::support::sample_document_with_binding(true)?;
    let parameter = doc.parameters()[0].id.clone();
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let original = shown(&mut editor)?.geometry.meshes[0].vertices.clone();
    configure(&mut editor)?;
    let vertex = editor.overlay().vertices[0].position;
    let center = p(vertex.x - 20., vertex.y);
    down(&mut editor, center, false)?;
    hold(&mut editor, 10, Duration::from_millis(500))?;
    input(&mut editor, CanvasInput::Up { point: center })?;
    let edited = shown(&mut editor)?.geometry.meshes[0].vertices.clone();
    assert_ne!(edited, original);
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.clone(),
        value: 1.,
    })?;
    assert_eq!(shown(&mut editor)?.geometry.meshes[0].vertices, original);
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter,
        value: 0.5,
    })?;
    let interpolated = shown(&mut editor)?.geometry.meshes[0].vertices.clone();
    let vertex = editor.overlay().vertices[0].position;
    down(&mut editor, p(vertex.x - 20., vertex.y), false)?;
    assert!(!editor.gesture_active());
    hold(&mut editor, 10, Duration::from_millis(500))?;
    assert_eq!(
        shown(&mut editor)?.geometry.meshes[0].vertices,
        interpolated
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn inflation_mode_keeps_cmd_drag_priority_and_does_not_tick_a_move_or_resize() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    configure(&mut editor)?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            weight: 0.5,
            ..editor.brush_selection_settings()
        },
    ))?;
    let vertex = editor.overlay().vertices[0].position;
    editor.execute(EditorCommand::SetBrushMode(BrushMode::Select))?;
    down(&mut editor, vertex, false)?;
    input(&mut editor, CanvasInput::Up { point: vertex })?;
    editor.execute(EditorCommand::SetBrushMode(BrushMode::Inflate))?;
    let before = editor.document().unwrap().selected_vertex_positions();
    input(
        &mut editor,
        CanvasInput::Down {
            point: vertex,
            shift: false,
            alt: false,
            primary: true,
        },
    )?;
    assert_eq!(
        editor.overlay().active_gesture,
        Some(CanvasGestureKind::Move)
    );
    hold(&mut editor, 10, Duration::from_millis(500))?;
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        before
    );
    input(
        &mut editor,
        CanvasInput::Up {
            point: p(vertex.x + 20., vertex.y),
        },
    )?;
    let after = editor.document().unwrap().selected_vertex_positions();
    assert!((after[0].1.x - before[0].1.x - 10.).abs() < 0.001);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let binding = shown(&mut editor)?.version.binding;
    let token = editor
        .begin_brush_size_gesture(binding, vertex)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(
        token,
        CanvasInput::BrushTick {
            elapsed: Duration::from_millis(100),
        },
    )?;
    editor.update_canvas_gesture(
        token,
        CanvasInput::Up {
            point: p(vertex.x + 10., vertex.y),
        },
    )?;
    assert_eq!(editor.brush_selection_settings().diameter, 100.);
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        after
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}
