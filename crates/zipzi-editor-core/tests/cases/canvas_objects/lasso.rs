use super::super::support::present_canvas as shown;
use super::common::*;
use zipzi_editor_core::*;

fn p(x: f32, y: f32) -> ViewportPoint {
    ViewportPoint { x, y }
}

fn around(center: ViewportPoint) -> [ViewportPoint; 4] {
    [
        p(center.x - 12., center.y - 12.),
        p(center.x + 12., center.y - 12.),
        p(center.x + 12., center.y + 12.),
        p(center.x - 12., center.y + 12.),
    ]
}

fn lasso(
    editor: &mut Editor,
    path: &[ViewportPoint],
    shift: bool,
    alt: bool,
) -> anyhow::Result<()> {
    crate::support::pointer_input(
        editor,
        CanvasInput::Down {
            point: path[0],
            shift,
            alt,
        },
    )?;
    for &point in &path[1..path.len() - 1] {
        assert!(!crate::support::pointer_input(
            editor,
            CanvasInput::Move {
                point,
                left_down: true
            }
        )?);
        assert!(!editor.document().unwrap().has_pending_edit());
    }
    assert!(!crate::support::pointer_input(
        editor,
        CanvasInput::Up {
            point: *path.last().unwrap()
        }
    )?);
    assert!(!editor.gesture_active());
    assert!(editor.overlay().lasso.is_empty());
    Ok(())
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

#[test]
fn lasso_replace_add_remove_then_select_tool_moves_only_the_remaining_vertex() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let vertices = editor.overlay().vertices;
    let original = editor.document().unwrap().selected_vertex_positions();
    let objects = editor.document().unwrap().context().selections.clone();
    let evaluations = editor.evaluation_count();
    let modified = editor.document().unwrap().modified();
    editor.dispatch(editor.command(CommandId::LassoTool))?;
    assert!(
        editor
            .command_state(editor.command(CommandId::LassoTool))
            .checked
    );
    lasso(&mut editor, &around(vertices[0].position), false, false)?;
    assert_eq!(selected(&editor), vec![vertices[0].id]);
    lasso(&mut editor, &around(vertices[1].position), true, false)?;
    assert_eq!(selected(&editor).len(), 2);
    // Alt takes precedence over Shift, and removing the same vertex twice is harmless.
    for _ in 0..2 {
        lasso(&mut editor, &around(vertices[0].position), true, true)?;
        assert_eq!(selected(&editor), vec![vertices[1].id]);
    }
    assert_eq!(editor.document().unwrap().context().selections, objects);
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert_eq!(editor.document().unwrap().modified(), modified);
    assert_eq!(editor.evaluation_count(), evaluations);
    assert!(editor.preview_if_needed()?.is_none());
    editor.dispatch(editor.command(CommandId::SelectTool))?;
    assert_eq!(selected(&editor).len(), 1);
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
            .zip(&original)
            .filter(|(a, b)| a != b)
            .count(),
        1
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn lasso_vertex_hover_and_drag_match_select_and_cancel_restores_geometry() -> anyhow::Result<()> {
    for tool in [ToolId::Select, ToolId::Lasso] {
        for cancel in [false, true] {
            let (mut editor, _dir) = setup()?;
            let vertex = editor.overlay().vertices[0].clone();
            let positions = editor.document().unwrap().selected_vertex_positions();
            editor.execute(EditorCommand::SetTool(tool))?;
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: vertex.position,
                    left_down: false,
                },
            )?;
            let overlay = editor.overlay();
            assert!(
                overlay
                    .vertices
                    .iter()
                    .any(|v| v.id == vertex.id && v.hovered)
            );
            assert_eq!(
                overlay.active_component.as_ref().map(|c| c.id),
                Some(vertex.id)
            );
            assert!(!editor.document().unwrap().has_pending_edit());
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    point: vertex.position,
                    shift: false,
                    alt: false,
                },
            )?;
            assert!(
                !editor
                    .viewport_interaction_hints(editor.current_viewport_interaction().unwrap())
                    .contains(&InteractionHint::Drag(HintAction::LassoSelect))
            );
            assert!(!editor.document().unwrap().has_pending_edit());
            let end = p(vertex.position.x + 15., vertex.position.y + 8.);
            assert!(crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true
                }
            )?);
            let overlay = editor.overlay();
            assert!(overlay.lasso.is_empty());
            assert!(overlay.marquee.is_none());
            assert_eq!(
                overlay.active_component.as_ref().map(|c| c.id),
                Some(vertex.id)
            );
            assert!(editor.document().unwrap().has_pending_edit());
            let changed = editor.document().unwrap().selected_vertex_positions();
            assert_eq!(
                changed
                    .iter()
                    .zip(&positions)
                    .filter(|(a, b)| a != b)
                    .count(),
                1
            );
            if cancel {
                crate::support::pointer_input(&mut editor, CanvasInput::Cancel)?;
                assert_eq!(
                    editor.document().unwrap().selected_vertex_positions(),
                    positions
                );
                assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            } else {
                crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
                assert_eq!(
                    editor.document().unwrap().selected_vertex_positions(),
                    changed
                );
                assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            }
            assert_eq!(editor.active_tool(), tool);
            assert!(!editor.gesture_active());
        }
    }
    Ok(())
}

#[test]
fn lasso_gesture_keeps_selecting_when_it_crosses_a_vertex() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let vertex = editor.overlay().vertices[0].clone();
    let positions = editor.document().unwrap().selected_vertex_positions();
    editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
    let path = around(vertex.position);
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: path[0],
            shift: false,
            alt: false,
        },
    )?;
    assert!(
        editor
            .viewport_interaction_hints(editor.current_viewport_interaction().unwrap())
            .contains(&InteractionHint::Drag(HintAction::LassoSelect))
    );
    for point in [path[1], vertex.position, path[2]] {
        assert!(!crate::support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point,
                left_down: true
            }
        )?);
        assert!(
            editor
                .viewport_interaction_hints(editor.current_viewport_interaction().unwrap())
                .contains(&InteractionHint::Drag(HintAction::LassoSelect))
        );
        assert!(editor.overlay().active_component.is_none());
        assert!(!editor.document().unwrap().has_pending_edit());
    }
    // Up completes the polygon without another Move event.
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: path[3] })?;
    assert_eq!(selected(&editor), vec![vertex.id]);
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        positions
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn lasso_cancel_lost_button_tool_switch_and_resize_preserve_selection() -> anyhow::Result<()> {
    for cancellation in 0..4 {
        let (mut editor, _dir) = setup()?;
        let v = editor.overlay().vertices[0].position;
        click(&mut editor, v, false)?;
        let original = selected(&editor);
        let objects = editor.document().unwrap().context().selections.clone();
        editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: p(5., 5.),
                shift: false,
                alt: false,
            },
        )?;
        for point in [p(500., 5.), p(500., 500.)] {
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point,
                    left_down: true,
                },
            )?;
        }
        assert!(!editor.overlay().lasso.is_empty());
        match cancellation {
            0 => {
                editor.dispatch(editor.command(CommandId::CancelGesture))?;
            }
            1 => {
                crate::support::pointer_input(
                    &mut editor,
                    CanvasInput::Move {
                        point: v,
                        left_down: false,
                    },
                )?;
            }
            2 => {
                editor.execute(EditorCommand::SetTool(ToolId::Select))?;
            }
            _ => editor.set_canvas_metrics(ViewportMetrics {
                width: 600.,
                height: 600.,
                scale_factor: 2.,
            }),
        }
        crate::support::pointer_input(&mut editor, CanvasInput::Up { point: p(5., 500.) })?;
        assert!(!editor.gesture_active());
        assert_eq!(selected(&editor), original, "cancellation {cancellation}");
        assert_eq!(editor.document().unwrap().context().selections, objects);
        assert!(editor.overlay().lasso.is_empty());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn lasso_degenerate_paths_preserve_selection_and_surface_click_matches_select() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let v = editor.overlay().vertices[0].position;
    click(&mut editor, v, false)?;
    editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
    let original = selected(&editor);
    let objects = editor.document().unwrap().context().selections.clone();
    for path in [
        vec![v, v],
        vec![p(1., 1.), p(200., 200.), p(400., 400.)],
        vec![v, p(v.x + 0.5, v.y), p(v.x, v.y + 0.5)],
    ] {
        lasso(&mut editor, &path, false, false)?;
        assert_eq!(selected(&editor), original);
        assert_eq!(editor.document().unwrap().context().selections, objects);
    }
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: v,
            shift: false,
            alt: false,
        },
    )?;
    assert!(
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: p(f32::NAN, 0.),
                left_down: true
            }
        )
        .is_err()
    );
    assert!(!editor.gesture_active());
    assert_eq!(selected(&editor), original);
    shown(&mut editor)?;
    click(&mut editor, p(256., 256.), false)?;
    assert!(selected(&editor).is_empty());
    let top = editor
        .layers()
        .into_iter()
        .find(|l| l.name == "Second")
        .unwrap()
        .id;
    assert_eq!(editor.document().unwrap().selected_id(), Some(top.as_str()));
    Ok(())
}

#[test]
fn lasso_blank_click_clears_objects_and_components_even_with_modifiers() -> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    shown(&mut editor)?;
    for object in [
        mesh,
        ObjectRef::new(ObjectKind::Deformer, warp),
        ObjectRef::new(ObjectKind::Deformer, rotation),
    ] {
        for (shift, alt) in [(false, false), (true, false), (false, true), (true, true)] {
            editor.execute(EditorCommand::SetTool(ToolId::Select))?;
            editor.execute(EditorCommand::SelectObject(object.clone()))?;
            let vertex = editor.overlay().vertices[0].position;
            click(&mut editor, vertex, false)?;
            assert!(!selected(&editor).is_empty());
            editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
            let evaluations = editor.evaluation_count();
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    point: p(-100., -100.),
                    shift,
                    alt,
                },
            )?;
            // Pressing blank space is still cancellable until release.
            assert!(!selected(&editor).is_empty());
            assert_eq!(
                editor.document().unwrap().context().selections,
                vec![object.clone()]
            );
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Up {
                    point: p(-99.5, -100.),
                },
            )?;
            let doc = editor.document().unwrap();
            assert!(doc.context().selections.is_empty());
            assert!(doc.context().selection.is_none());
            assert!(doc.context().inspected.is_none());
            assert_eq!(doc.history_lengths(), (0, 0));
            assert_eq!(editor.evaluation_count(), evaluations);
            assert!(!editor.gesture_active());
            editor.execute(EditorCommand::SelectObject(object.clone()))?;
            assert!(selected(&editor).is_empty());
        }
    }
    Ok(())
}

#[test]
fn lasso_object_selection_requires_all_vertices_and_filters_hidden_and_locked_meshes()
-> anyhow::Result<()> {
    for excluded in 0..3 {
        let (mut editor, _dir) = setup()?;
        let mesh = editor
            .document()
            .unwrap()
            .context()
            .selection
            .clone()
            .unwrap();
        match excluded {
            1 => editor.execute(EditorCommand::SetObjectVisible {
                object: mesh.clone(),
                visible: false,
            })?,
            2 => editor.execute(EditorCommand::SetObjectsLocked {
                objects: vec![mesh.clone()],
                locked: true,
            })?,
            _ => {}
        }
        editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
        editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
        shown(&mut editor)?;
        let history = editor.document().unwrap().history_lengths();
        let evaluations = editor.evaluation_count();
        lasso(
            &mut editor,
            &[p(5., 5.), p(505., 5.), p(5., 505.)],
            false,
            false,
        )?;
        assert!(editor.document().unwrap().context().selections.is_empty());
        lasso(
            &mut editor,
            &[p(5., 5.), p(505., 5.), p(505., 505.), p(5., 505.)],
            false,
            false,
        )?;
        let selection = &editor.document().unwrap().context().selections;
        assert_eq!(selection.len(), if excluded == 0 { 2 } else { 1 });
        if excluded != 0 {
            assert!(!selection.contains(&mesh));
        }
        assert_eq!(editor.document().unwrap().history_lengths(), history);
        assert_eq!(editor.evaluation_count(), evaluations);
    }
    Ok(())
}

#[test]
fn lasso_warp_anchors_and_fine_controls_are_selected_from_outside_handles() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    // Produce a refined lattice so ordinary and fine selection have different domains.
    let handle = editor
        .overlay()
        .handles
        .iter()
        .find(|h| h.round)
        .unwrap()
        .position;
    drag(&mut editor, handle, p(handle.x, handle.y - 40.), false)?;
    for fine in [false, true] {
        if fine {
            editor.execute(EditorCommand::ToggleWarpFine)?;
        }
        shown(&mut editor)?;
        let expected = editor.overlay().vertices.len();
        assert_eq!(expected, if fine { 25 } else { 4 });
        let positions = control_points(&mut editor, &object)?;
        let history = editor.document().unwrap().history_lengths();
        editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
        lasso(
            &mut editor,
            &[
                p(-500., -500.),
                p(1000., -500.),
                p(1000., 1000.),
                p(-500., 1000.),
                p(-500., -500.),
            ],
            false,
            false,
        )?;
        assert_eq!(selected(&editor).len(), expected);
        assert_eq!(
            editor.document().unwrap().context().selections,
            vec![object.clone()]
        );
        assert_eq!(control_points(&mut editor, &object)?, positions);
        assert_eq!(editor.document().unwrap().history_lengths(), history);
    }
    Ok(())
}

#[test]
fn lasso_deformer_handle_drags_match_select() -> anyhow::Result<()> {
    for rotation in [false, true] {
        let mut results = Vec::new();
        for tool in [ToolId::Select, ToolId::Lasso] {
            let (mut editor, _dir, object) = deformer_editor(rotation)?;
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
            let (kind, start) = gizmo.handles[0];
            editor.execute(EditorCommand::SetTool(tool))?;
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: start,
                    left_down: false,
                },
            )?;
            assert_eq!(editor.overlay().active_handle, Some(kind));
            let before = control_points(&mut editor, &object)?;
            let end = p(start.x + 20., start.y + 10.);
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Down {
                    point: start,
                    shift: false,
                    alt: false,
                },
            )?;
            assert!(
                !editor
                    .viewport_interaction_hints(editor.current_viewport_interaction().unwrap())
                    .contains(&InteractionHint::Drag(HintAction::LassoSelect))
            );
            assert!(crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true
                }
            )?);
            assert!(editor.overlay().lasso.is_empty());
            crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
            let after = control_points(&mut editor, &object)?;
            assert_ne!(before, after);
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            assert_eq!(editor.active_tool(), tool);
            results.push(after);
        }
        assert_eq!(results[0], results[1]);
    }
    Ok(())
}

#[test]
fn lasso_isolation_and_viewport_ownership_keep_other_geometry_out_of_the_gesture()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let first = editor.primary_viewport_binding().unwrap();
    let mesh = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    editor.toggle_viewport_isolation(first)?;
    editor.execute(EditorCommand::SelectCanvasObjects(Vec::new()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
    shown(&mut editor)?;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(other, editor.canvas_metrics());
    let other = editor.viewport_binding(other).unwrap();
    let frame = editor.preview_for(other)?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    let start = p(-500., -500.);
    crate::support::pointer_input_for(
        &mut editor,
        first,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input_for(
        &mut editor,
        first,
        CanvasInput::Move {
            point: p(1000., -500.),
            left_down: true,
        },
    )?;
    assert!(!editor.overlay_for_viewport(first).lasso.is_empty());
    assert!(editor.overlay_for_viewport(other).lasso.is_empty());
    crate::support::pointer_input_for(
        &mut editor,
        other,
        CanvasInput::Up {
            point: p(1000., 1000.),
        },
    )?;
    assert!(editor.gesture_active());
    crate::support::pointer_input_for(
        &mut editor,
        first,
        CanvasInput::Move {
            point: p(1000., 1000.),
            left_down: true,
        },
    )?;
    crate::support::pointer_input_for(
        &mut editor,
        first,
        CanvasInput::Up {
            point: p(-500., 1000.),
        },
    )?;
    assert_eq!(editor.document().unwrap().context().selections, vec![mesh]);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
