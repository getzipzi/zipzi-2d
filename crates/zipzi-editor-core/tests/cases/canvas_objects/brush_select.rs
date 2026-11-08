use super::common::*;
use crate::support::{pointer_input as input, present_canvas as shown};
use zipzi_editor_core::*;

fn p(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
}
fn selected(editor: &Editor) -> Vec<u32> {
    editor
        .overlay()
        .vertices
        .iter()
        .filter(|v| v.selected)
        .map(|v| v.id)
        .collect()
}
fn stroke(
    editor: &mut Editor,
    points: &[ViewportPoint],
    shift: bool,
    alt: bool,
) -> anyhow::Result<()> {
    assert!(!input(
        editor,
        CanvasInput::Down {
            point: points[0],
            shift,
            alt,
            primary: false
        }
    )?);
    for &point in &points[1..] {
        assert!(!input(
            editor,
            CanvasInput::Move {
                point,
                left_down: true
            }
        )?);
        assert!(!editor.document().unwrap().has_pending_edit());
    }
    assert!(!input(
        editor,
        CanvasInput::Up {
            point: *points.last().unwrap()
        }
    )?);
    assert!(!editor.gesture_active());
    Ok(())
}

#[test]
fn brush_select_paint_add_erase_and_transform_share_the_existing_selection() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let vertices = editor.overlay().vertices;
    let positions = editor.document().unwrap().selected_vertex_positions();
    let objects = editor.document().unwrap().context().selections.clone();
    let modified = editor.document().unwrap().modified();
    let evaluations = editor.evaluation_count();
    editor.dispatch(editor.command(CommandId::BrushSelectTool))?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 8.,
            weight: 1.,
            ..Default::default()
        },
    ))?;
    assert!(
        editor
            .command_state(editor.command(CommandId::BrushSelectTool))
            .checked
    );
    stroke(&mut editor, &[vertices[0].position], false, false)?;
    assert_eq!(selected(&editor), vec![vertices[0].id]);
    stroke(
        &mut editor,
        &[vertices[1].position, vertices[1].position],
        true,
        false,
    )?;
    assert_eq!(selected(&editor).len(), 2);
    stroke(&mut editor, &[vertices[0].position], true, true)?;
    assert_eq!(selected(&editor), vec![vertices[1].id]);
    assert_eq!(editor.document().unwrap().context().selections, objects);
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        positions
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert_eq!(editor.document().unwrap().modified(), modified);
    assert_eq!(editor.evaluation_count(), evaluations);
    assert!(editor.preview_if_needed()?.is_none());
    for tool in [ToolId::Lasso, ToolId::Select] {
        editor.execute(EditorCommand::SetTool(tool))?;
        assert_eq!(selected(&editor), vec![vertices[1].id]);
    }
    drag(
        &mut editor,
        vertices[1].position,
        p(vertices[1].position.x + 12., vertices[1].position.y + 5.),
        false,
    )?;
    let changed = editor.document().unwrap().selected_vertex_positions();
    assert_eq!(
        changed
            .iter()
            .zip(&positions)
            .filter(|(a, b)| a != b)
            .count(),
        1
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn brush_select_sweeps_between_events_and_starting_on_a_vertex_never_deforms() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let vertex = editor.overlay().vertices[0].clone();
    let positions = editor.document().unwrap().selected_vertex_positions();
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSize(8.))?;
    stroke(
        &mut editor,
        &[
            p(vertex.position.x - 40., vertex.position.y),
            p(vertex.position.x + 40., vertex.position.y),
        ],
        false,
        false,
    )?;
    assert_eq!(selected(&editor), vec![vertex.id]);
    stroke(
        &mut editor,
        &[
            vertex.position,
            p(vertex.position.x + 15., vertex.position.y + 8.),
        ],
        false,
        false,
    )?;
    assert_eq!(selected(&editor), vec![vertex.id]);
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        positions
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn brush_select_live_preview_rolls_back_for_cancel_lost_button_tool_switch_and_resize()
-> anyhow::Result<()> {
    for cancellation in 0..5 {
        let (mut editor, _dir) = setup()?;
        let vertices = editor.overlay().vertices;
        click(&mut editor, vertices[0].position, false)?;
        let original = selected(&editor);
        editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
        editor.execute(EditorCommand::SetBrushSelectionSize(8.))?;
        input(
            &mut editor,
            CanvasInput::Down {
                point: vertices[1].position,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        assert_eq!(selected(&editor), vec![vertices[1].id]);
        match cancellation {
            0 => {
                editor.dispatch(editor.command(CommandId::CancelGesture))?;
            }
            1 => {
                input(
                    &mut editor,
                    CanvasInput::Move {
                        point: vertices[1].position,
                        left_down: false,
                    },
                )?;
            }
            2 => editor.execute(EditorCommand::SetTool(ToolId::Select))?,
            3 => editor.set_canvas_metrics(ViewportMetrics {
                width: 600.,
                height: 600.,
                scale_factor: 2.,
            }),
            _ => {
                assert!(
                    input(
                        &mut editor,
                        CanvasInput::Move {
                            point: p(f32::NAN, 0.),
                            left_down: true
                        }
                    )
                    .is_err()
                );
            }
        }
        assert_eq!(selected(&editor), original, "cancellation {cancellation}");
        assert!(!editor.gesture_active());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn brush_select_warp_targets_only_lattice_points_and_preserves_geometry() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        warp.clone(),
    )))?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSize(8.))?;
    let vertex = editor
        .overlay()
        .vertices
        .into_iter()
        .find(|v| v.kind == OverlayVertexKind::WarpGrid)
        .unwrap();
    let before: Vec<_> = editor
        .overlay()
        .vertices
        .iter()
        .map(|v| (v.id, v.position))
        .collect();
    stroke(&mut editor, &[vertex.position], false, false)?;
    let chosen: Vec<_> = editor
        .overlay()
        .vertices
        .into_iter()
        .filter(|v| v.selected)
        .collect();
    assert!(chosen.iter().any(|v| v.id == vertex.id));
    assert!(chosen.iter().all(|v| v.kind == OverlayVertexKind::WarpGrid));
    let after: Vec<_> = editor
        .overlay()
        .vertices
        .iter()
        .map(|v| (v.id, v.position))
        .collect();
    assert_eq!(after, before);
    editor.dispatch(editor.command(CommandId::SelectAll))?;
    assert!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .all(|v| v.kind == OverlayVertexKind::WarpGrid)
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn brush_select_never_falls_back_to_painting_objects_or_locked_hidden_points() -> anyhow::Result<()>
{
    for excluded in 0..3 {
        let (mut editor, _dir) = setup()?;
        let mesh = editor
            .document()
            .unwrap()
            .context()
            .selection
            .clone()
            .unwrap();
        let point = editor.overlay().vertices[0].position;
        match excluded {
            0 => editor.execute(EditorCommand::SelectObjects(Vec::new()))?,
            1 => editor.execute(EditorCommand::SetObjectsLocked {
                objects: vec![mesh.clone()],
                locked: true,
            })?,
            _ => editor.execute(EditorCommand::SetObjectVisible {
                object: mesh,
                visible: false,
            })?,
        }
        shown(&mut editor)?;
        let objects = editor.document().unwrap().context().selections.clone();
        let history = editor.document().unwrap().history_lengths();
        editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
        stroke(
            &mut editor,
            &[point, p(point.x + 30., point.y)],
            false,
            false,
        )?;
        assert!(selected(&editor).is_empty());
        assert_eq!(editor.document().unwrap().context().selections, objects);
        assert_eq!(editor.document().unwrap().history_lengths(), history);
    }
    Ok(())
}

#[test]
fn brush_select_size_gesture_is_selection_neutral_and_cancellable() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let vertex = editor.overlay().vertices[0].clone();
    click(&mut editor, vertex.position, false)?;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    let original = selected(&editor);
    let size = editor.brush_selection_settings().diameter;
    let binding = editor.primary_viewport_binding().unwrap();
    for cancel in [true, false] {
        let token = editor
            .begin_brush_size_gesture(binding, vertex.position)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(
            token,
            CanvasInput::Move {
                point: p(vertex.position.x + 20., vertex.position.y),
                left_down: true,
            },
        )?;
        assert_eq!(editor.brush_selection_settings().diameter, size + 40.);
        assert_eq!(selected(&editor), original);
        if cancel {
            editor.cancel_viewport_interaction(token);
            assert_eq!(editor.brush_selection_settings().diameter, size);
        } else {
            editor.update_canvas_gesture(
                token,
                CanvasInput::Up {
                    point: p(vertex.position.x + 20., vertex.position.y),
                },
            )?;
        }
        assert!(!editor.gesture_active());
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn brush_select_radius_tracks_screen_units_across_zoom_and_dpi() -> anyhow::Result<()> {
    for (zoom, scale_factor) in [(0.5, 1.), (1., 2.), (2., 1.5)] {
        let (mut editor, _dir) = setup()?;
        let binding = editor.primary_viewport_binding().unwrap();
        editor.set_canvas_metrics(ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor,
        });
        editor.set_viewport_zoom(binding, zoom, None)?;
        shown(&mut editor)?;
        let vertex = editor.overlay().vertices[0].clone();
        editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
        editor.execute(EditorCommand::SetBrushSelectionSize(20.))?;
        stroke(
            &mut editor,
            &[p(vertex.position.x + 11., vertex.position.y)],
            false,
            false,
        )?;
        assert!(!selected(&editor).contains(&vertex.id));
        stroke(
            &mut editor,
            &[p(vertex.position.x + 9., vertex.position.y)],
            false,
            false,
        )?;
        assert!(selected(&editor).contains(&vertex.id));
        assert_eq!(editor.overlay().selection_brush.unwrap().radius, 10.);
    }
    Ok(())
}

#[test]
fn brush_select_peer_viewport_cannot_finish_or_display_the_active_brush() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let first = editor.primary_viewport_binding().unwrap();
    let point = editor.overlay().vertices[0].position;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(other, editor.canvas_metrics());
    let other = editor.viewport_binding(other).unwrap();
    let frame = editor.preview_for(other)?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    crate::support::pointer_input_for(
        &mut editor,
        first,
        CanvasInput::Down {
            point,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    assert!(editor.overlay_for_viewport(first).selection_brush.is_some());
    assert!(editor.overlay_for_viewport(other).selection_brush.is_none());
    crate::support::pointer_input_for(&mut editor, other, CanvasInput::Up { point })?;
    assert!(editor.gesture_active());
    crate::support::pointer_input_for(&mut editor, first, CanvasInput::Up { point })?;
    assert!(!editor.gesture_active());
    assert!(
        !editor
            .overlay_for_viewport(first)
            .vertices
            .iter()
            .filter(|v| v.selected)
            .collect::<Vec<_>>()
            .is_empty()
    );
    Ok(())
}

fn weight_at(editor: &Editor, point: ViewportPoint) -> f32 {
    editor
        .overlay()
        .selection_weights
        .as_ref()
        .and_then(|w| {
            w.points
                .iter()
                .find(|(p, _, _)| ((p.x - point.x).powi(2) + (p.y - point.y).powi(2)) < 0.01)
                .map(|(_, w, _)| *w)
        })
        .unwrap_or(0.)
}

#[test]
fn weighted_brush_cmd_drag_respects_each_weight_and_undo_cancel_keep_the_selection()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let vertices = editor.overlay().vertices;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    for (i, weight) in [(0, 0.25), (1, 0.75)] {
        editor.execute(EditorCommand::SetBrushSelectionSettings(
            BrushSelectionSettings {
                diameter: 8.,
                weight,
                hardness: 1.,
                ..Default::default()
            },
        ))?;
        for pass in 0..2 {
            stroke(
                &mut editor,
                &[vertices[i].position],
                i != 0 || pass != 0,
                false,
            )?;
        }
    }
    assert_eq!(weight_at(&editor, vertices[0].position), 0.4375);
    assert_eq!(weight_at(&editor, vertices[1].position), 0.9375);
    // Changing future paint strength must not rescale the stored selection.
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            weight: 0.1,
            ..editor.brush_selection_settings()
        },
    ))?;
    let before = editor.document().unwrap().selected_vertex_positions();
    let start = vertices[0].position;
    let end = p(start.x + 40., start.y + 16.);
    for cancel in [true, false] {
        shown(&mut editor)?;
        input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: false,
                primary: true,
            },
        )?;
        assert!(editor.gesture_active());
        assert!(input(
            &mut editor,
            CanvasInput::Move {
                point: end,
                left_down: true
            }
        )?);
        let after = editor.document().unwrap().selected_vertex_positions();
        for (i, w) in [(0, 0.4375), (1, 0.9375), (2, 0.), (3, 0.)] {
            assert!(
                (after[i].1.x - before[i].1.x - 40. * w).abs() < 0.001,
                "point {i}: {:?}",
                after[i]
            );
            assert!((after[i].1.y - before[i].1.y + 16. * w).abs() < 0.001);
        }
        if cancel {
            input(&mut editor, CanvasInput::Cancel)?;
            assert_eq!(
                editor.document().unwrap().selected_vertex_positions(),
                before
            );
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            assert_eq!(weight_at(&editor, vertices[0].position), 0.4375);
            assert_eq!(weight_at(&editor, vertices[1].position), 0.9375);
        } else {
            input(&mut editor, CanvasInput::Up { point: end })?;
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
    }
    Ok(())
}

#[test]
fn weighted_brush_plain_press_restarts_selection_and_cancel_restores_previous_weights()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let vertices = editor.overlay().vertices;
    let a = vertices[0].position;
    let b = vertices[1].position;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 8.,
            weight: 0.2,
            hardness: 1.,
            ..Default::default()
        },
    ))?;
    stroke(&mut editor, &[a], false, false)?;
    stroke(&mut editor, &[a], true, false)?;
    stroke(&mut editor, &[b], true, false)?;
    assert!((weight_at(&editor, a) - 0.36).abs() < 1e-6);
    assert!((weight_at(&editor, b) - 0.2).abs() < 1e-6);
    input(
        &mut editor,
        CanvasInput::Down {
            point: a,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    assert!((weight_at(&editor, a) - 0.2).abs() < 1e-6);
    assert_eq!(weight_at(&editor, b), 0.);
    assert_eq!(selected(&editor), vec![vertices[0].id]);
    input(&mut editor, CanvasInput::Cancel)?;
    assert!((weight_at(&editor, a) - 0.36).abs() < 1e-6);
    assert!((weight_at(&editor, b) - 0.2).abs() < 1e-6);
    stroke(&mut editor, &[a], false, false)?;
    assert!((weight_at(&editor, a) - 0.2).abs() < 1e-6);
    assert_eq!(weight_at(&editor, b), 0.);
    stroke(&mut editor, &[p(a.x + 20., a.y)], false, false)?;
    assert!(
        selected(&editor).is_empty(),
        "plain blank press starts an empty selection"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn weighted_brush_repeated_dabs_accumulate_and_partial_erase_and_cancel_preserve_weights()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let v = editor.overlay().vertices[0].position;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 40.,
            weight: 0.4,
            hardness: 1.,
            ..Default::default()
        },
    ))?;
    stroke(&mut editor, &vec![v; 30], false, false)?;
    assert!((weight_at(&editor, v) - 0.4).abs() < 1e-6);
    stroke(&mut editor, &[v], true, false)?;
    assert!((weight_at(&editor, v) - 0.64).abs() < 1e-6);
    stroke(&mut editor, &[v], true, false)?;
    assert!((weight_at(&editor, v) - 0.784).abs() < 1e-6);
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 40.,
            weight: 0.5,
            hardness: 1.,
            ..Default::default()
        },
    ))?;
    assert!((weight_at(&editor, v) - 0.784).abs() < 1e-6);
    stroke(&mut editor, &[v], false, true)?;
    assert!((weight_at(&editor, v) - 0.392).abs() < 1e-6);
    input(
        &mut editor,
        CanvasInput::Down {
            point: v,
            shift: true,
            alt: false,
            primary: false,
        },
    )?;
    assert!((weight_at(&editor, v) - 0.696).abs() < 1e-6);
    input(&mut editor, CanvasInput::Cancel)?;
    assert!((weight_at(&editor, v) - 0.392).abs() < 1e-6);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn weighted_brush_return_passes_accumulate_independently_of_pointer_event_density()
-> anyhow::Result<()> {
    for shape in [BrushShape::Circle, BrushShape::Square, BrushShape::Line] {
        for hardness in [0.25, 1.] {
            let mut results = Vec::new();
            for divisions in [1, 240] {
                let (mut editor, _dir) = setup()?;
                let v = editor.overlay().vertices[0].position;
                editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
                editor.execute(EditorCommand::SetBrushSelectionSettings(
                    BrushSelectionSettings {
                        diameter: 40.,
                        weight: 0.2,
                        hardness,
                        shape,
                        angle: 35.,
                        ..Default::default()
                    },
                ))?;
                let start = p(v.x - 60., v.y);
                let end = p(v.x + 60., v.y);
                input(
                    &mut editor,
                    CanvasInput::Down {
                        point: start,
                        shift: false,
                        alt: false,
                        primary: false,
                    },
                )?;
                let mut one_pass = 0.;
                for (from, to) in [(start, end), (end, start)] {
                    for i in 1..=divisions {
                        let point = p(from.x + (to.x - from.x) * i as f32 / divisions as f32, v.y);
                        for _ in 0..3 {
                            input(
                                &mut editor,
                                CanvasInput::Move {
                                    point,
                                    left_down: true,
                                },
                            )?;
                        }
                    }
                    let current = weight_at(&editor, v);
                    assert!(
                        current > one_pass,
                        "return pass must add weight: {shape:?}, {hardness}"
                    );
                    one_pass = current;
                }
                let before_up = weight_at(&editor, v);
                input(&mut editor, CanvasInput::Up { point: start })?;
                assert_eq!(
                    weight_at(&editor, v),
                    before_up,
                    "release must not add a dab"
                );
                results.push(before_up);
            }
            assert!(
                (results[0] - results[1]).abs() < 1e-5,
                "{shape:?}, hardness {hardness}: sparse/dense={results:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn weighted_brush_default_strength_reaches_full_weight_and_zero_strength_is_neutral()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let v = editor.overlay().vertices[0].position;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSize(8.))?;
    assert_eq!(editor.brush_selection_settings().weight, 0.2);
    for (pass, expected) in [0.2, 0.36, 0.488].into_iter().enumerate() {
        stroke(&mut editor, &[v], pass != 0, false)?;
        assert!((weight_at(&editor, v) - expected).abs() < 1e-6);
    }
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            weight: 0.,
            ..editor.brush_selection_settings()
        },
    ))?;
    for alt in [false, true] {
        stroke(&mut editor, &[v, p(v.x + 40., v.y), v], true, alt)?;
        assert!((weight_at(&editor, v) - 0.488).abs() < 1e-6);
    }
    stroke(&mut editor, &[v], false, false)?;
    assert!(
        selected(&editor).is_empty(),
        "zero strength still restarts a plain stroke"
    );
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            weight: 0.2,
            ..editor.brush_selection_settings()
        },
    ))?;
    let mut previous = weight_at(&editor, v);
    for _ in 0..45 {
        stroke(&mut editor, &[v], true, false)?;
        let current = weight_at(&editor, v);
        assert!(current >= previous && current <= 1.);
        previous = current;
    }
    assert_eq!(previous, 1.);
    for _ in 0..45 {
        stroke(&mut editor, &[v], false, true)?;
    }
    assert_eq!(weight_at(&editor, v), 0.);
    assert!(selected(&editor).is_empty());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn weighted_brush_warp_cmd_drag_ignores_hidden_frame_adjustment() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let binding = editor.primary_viewport_binding().unwrap();
    editor.change_warp_frame(binding, object.clone(), WarpFrameAction::ToggleAdjust)?;
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 8.,
            weight: 0.25,
            hardness: 1.,
            ..Default::default()
        },
    ))?;
    let vertex = editor
        .overlay()
        .vertices
        .into_iter()
        .find(|v| v.kind == OverlayVertexKind::WarpGrid)
        .unwrap();
    stroke(&mut editor, &[vertex.position], false, false)?;
    assert_eq!(selected(&editor), vec![vertex.id]);
    assert_eq!(weight_at(&editor, vertex.position), 0.25);
    assert!(editor.warp_frame_info(binding, &object).unwrap().adjusting);
    assert!(!editor.overlay().warp_frame_adjusting);
    assert!(editor.overlay().handles.is_empty());
    let before = control_points(&mut editor, &object)?;
    let end = p(vertex.position.x + 40., vertex.position.y + 16.);

    for cancel in [true, false] {
        let token = editor
            .begin_canvas_gesture(binding, vertex.position, false, false, true)?
            .capture
            .expect("a hidden Warp frame must not block the brush's Cmd drag");
        assert!(
            editor
                .update_canvas_gesture(
                    token,
                    CanvasInput::Move {
                        point: end,
                        left_down: true,
                    },
                )?
                .model_changed
        );
        let after = control_points(&mut editor, &object)?;
        for (id, (a, b)) in before.iter().zip(&after).enumerate() {
            if id == vertex.id as usize {
                assert!((b.x - a.x - 10.).abs() < 0.001);
                assert!((b.y - a.y + 4.).abs() < 0.001);
            } else {
                assert_eq!(b, a);
            }
        }
        if cancel {
            assert!(editor.cancel_viewport_interaction(token));
            assert_eq!(control_points(&mut editor, &object)?, before);
            assert_eq!(weight_at(&editor, vertex.position), 0.25);
            assert!(editor.warp_frame_info(binding, &object).unwrap().adjusting);
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        } else {
            editor.update_canvas_gesture(token, CanvasInput::Up { point: end })?;
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        }
        assert!(!editor.gesture_active());
    }
    Ok(())
}

#[test]
fn weighted_brush_warp_cmd_drag_leaves_controller_and_children_policy_unchanged()
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
    editor.execute(EditorCommand::SetTool(ToolId::BrushSelect))?;
    editor.execute(EditorCommand::SetBrushSelectionSettings(
        BrushSelectionSettings {
            diameter: 8.,
            weight: 0.25,
            hardness: 1.,
            ..Default::default()
        },
    ))?;
    let v = editor
        .overlay()
        .vertices
        .iter()
        .find(|v| v.kind == OverlayVertexKind::WarpGrid)
        .unwrap()
        .clone();
    stroke(&mut editor, &[v.position], false, false)?;
    stroke(&mut editor, &[v.position], true, false)?;
    let before = editor
        .document()
        .unwrap()
        .object_index()
        .get(&object)
        .unwrap()
        .properties["warp"]
        .clone();
    input(
        &mut editor,
        CanvasInput::Down {
            point: v.position,
            shift: false,
            alt: false,
            primary: true,
        },
    )?;
    input(
        &mut editor,
        CanvasInput::Move {
            point: p(v.position.x + 40., v.position.y),
            left_down: true,
        },
    )?;
    input(
        &mut editor,
        CanvasInput::Up {
            point: p(v.position.x + 40., v.position.y),
        },
    )?;
    let after = editor
        .document()
        .unwrap()
        .object_index()
        .get(&object)
        .unwrap()
        .properties["warp"]
        .clone();
    assert_eq!(after["bezier"], before["bezier"]);
    let a = &after["points"][v.id as usize];
    let b = &before["points"][v.id as usize];
    assert!((a["x"].as_f64().unwrap() - b["x"].as_f64().unwrap() - 17.5).abs() < 0.001);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}
