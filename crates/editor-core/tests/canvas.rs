use editor_core::*;
mod support;
fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (doc, dir) = support::sample_document()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    present(&mut editor)?;
    Ok((editor, dir))
}
fn present(editor: &mut Editor) -> anyhow::Result<()> {
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    Ok(())
}
fn down(editor: &mut Editor, p: CanvasPoint, shift: bool) -> anyhow::Result<()> {
    editor.canvas_input(CanvasInput::Down { point: p, shift })?;
    Ok(())
}
fn up(editor: &mut Editor, p: CanvasPoint) -> anyhow::Result<()> {
    editor.canvas_input(CanvasInput::Up { point: p })?;
    Ok(())
}
#[test]
fn coordinate_spaces_round_trip_at_multiple_display_scales() {
    let canvas = kasane_core::Canvas::new(640., 480., kasane_core::Vec2::new(320., 240.), 100.);
    for dpi in [1., 1.5, 2., 3.] {
        for zoom in [0.25, 1., 2.75] {
            let t = ViewportTransform::new(
                canvas,
                CanvasMetrics {
                    width: 813.,
                    height: 517.,
                    scale_factor: dpi,
                },
                ViewState {
                    zoom,
                    pan: CanvasPoint { x: 13., y: -27. },
                },
            )
            .unwrap();
            let source =
                ViewportTransform::evaluated_to_source(canvas, kasane_core::Vec2::new(-2., 1.));
            assert_eq!(source, ModelPoint { x: 120., y: 140. });
            let round = t.to_model(t.to_canvas(source));
            assert!((round.x - source.x).abs() < 0.0001 && (round.y - source.y).abs() < 0.0001);
            assert_eq!(
                t.metrics.physical_size().unwrap(),
                ((813. * dpi).round() as u32, (517. * dpi).round() as u32)
            );
        }
    }
    assert!(!CanvasMetrics::default().visible());
}
#[test]
fn hover_selection_and_viewport_reuse_the_evaluated_scene() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let before = editor.evaluation_count();
    let first = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Move {
        point: first,
        left_down: false,
    })?;
    assert!(editor.preview_if_needed()?.is_none());
    let mesh = editor.document().unwrap().selected_id().unwrap().to_owned();
    editor.execute(EditorCommand::Select(mesh))?;
    assert!(editor.preview_if_needed()?.is_none());
    editor.zoom_by(0.25);
    present(&mut editor)?;
    editor.pan_by(CanvasPoint { x: 10., y: 15. });
    present(&mut editor)?;
    assert_eq!(editor.evaluation_count(), before);
    editor.set_canvas_metrics(CanvasMetrics::default());
    assert!(editor.preview_if_needed()?.is_none());
    editor.set_canvas_metrics(CanvasMetrics {
        width: 700.,
        height: 400.,
        scale_factor: 1.,
    });
    present(&mut editor)?;
    assert_eq!(editor.evaluation_count(), before);
    Ok(())
}
#[test]
fn multi_vertex_drag_is_one_history_step_and_preserves_uv_topology() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original = editor.preview()?.scene;
    let vertices = editor.overlay().vertices;
    down(&mut editor, vertices[0].position, false)?;
    up(&mut editor, vertices[0].position)?;
    present(&mut editor)?;
    down(&mut editor, vertices[1].position, true)?;
    up(&mut editor, vertices[1].position)?;
    present(&mut editor)?;
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        2
    );
    let start = vertices[0].position;
    down(&mut editor, start, false)?;
    for dx in [4., 12., 20.] {
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint {
                x: start.x + dx,
                y: start.y + 10.,
            },
            left_down: true,
        })?;
    }
    let final_point = CanvasPoint {
        x: start.x + 24.,
        y: start.y + 16.,
    };
    up(&mut editor, final_point)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let positions = editor.document().unwrap().selected_vertex_positions();
    assert_eq!(positions[0].1, ModelPoint { x: 140., y: 32. });
    assert_eq!(positions[1].1, ModelPoint { x: 244., y: 136. });
    let scene = editor.preview()?.scene;
    assert_eq!(
        scene.frame.drawables[0].uvs,
        original.frame.drawables[0].uvs
    );
    assert_eq!(
        scene.frame.drawables[0].indices,
        original.frame.drawables[0].indices
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions()[0].1,
        ModelPoint { x: 128., y: 24. }
    );
    Ok(())
}
#[test]
fn returning_to_anchor_or_cancelling_keeps_redo_and_original_positions() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    editor.execute(EditorCommand::SetVisible(false))?;
    editor.execute(EditorCommand::Undo)?;
    present(&mut editor)?;
    let start = editor.overlay().vertices[0].position;
    down(&mut editor, start, false)?;
    editor.canvas_input(CanvasInput::Move {
        point: CanvasPoint {
            x: start.x + 27.25,
            y: start.y - 8.,
        },
        left_down: true,
    })?;
    up(&mut editor, start)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    present(&mut editor)?;
    down(&mut editor, start, false)?;
    editor.canvas_input(CanvasInput::Move {
        point: CanvasPoint {
            x: start.x + 40.,
            y: start.y,
        },
        left_down: true,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: start,
        left_down: false,
    })?;
    assert!(!editor.gesture_active());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions()[0].1,
        ModelPoint { x: 128., y: 24. }
    );
    Ok(())
}
#[test]
fn switch_save_close_and_tool_commands_cancel_canvas_candidates() -> anyhow::Result<()> {
    for finish in 0..4 {
        let (mut editor, dir) = setup()?;
        let id = editor.active_id().unwrap();
        let p = editor.overlay().vertices[0].position;
        down(&mut editor, p, false)?;
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint {
                x: p.x + 20.,
                y: p.y,
            },
            left_down: true,
        })?;
        match finish {
            0 => {
                editor.new_document()?;
                editor.activate(id)?;
            }
            1 => {
                let task = editor.prepare_save(id, &dir.path().join("new"), false)?;
                editor.finish_save(&mut task.run())?;
            }
            2 => {
                assert_eq!(editor.prepare_close(id)?, CloseDisposition::Ready);
            }
            _ => {
                editor.dispatch(editor.command(CommandId::Vertices))?;
            }
        }
        assert!(!editor.gesture_active());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions()[0].1,
            ModelPoint { x: 128., y: 24. }
        );
    }
    Ok(())
}
#[test]
fn delayed_frames_cannot_mix_documents_geometry_or_viewports() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let old = editor.preview()?.canvas.unwrap();
    editor.zoom_by(0.25);
    let latest = editor.preview()?.canvas.unwrap();
    assert!(!editor.present_frame(old.clone()));
    assert!(editor.present_frame(latest.clone()));
    let shown = editor.overlay().vertices[0].position;
    editor.pan_by(CanvasPoint { x: 30., y: 0. });
    let pending = editor.preview()?.canvas.unwrap();
    assert_eq!(
        editor.overlay().vertices[0].position,
        shown,
        "old geometry stays with old image until presentation"
    );
    assert!(editor.present_frame(pending));
    assert_ne!(editor.overlay().vertices[0].position, shown);
    editor.new_document()?;
    assert!(!editor.present_frame(latest));
    assert!(!editor.present_frame(old));
    Ok(())
}
#[test]
fn hit_ties_are_stable_and_hit_radius_is_in_logical_pixels() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let mut frame = editor.preview()?.canvas.unwrap();
    let mut geometry = frame.geometry.to_vec();
    let p = geometry[0].vertices[0].1;
    geometry[0].vertices[1].1 = p;
    geometry[0].vertices.reverse();
    frame.geometry = geometry.into();
    let hit = frame.transform.to_canvas(p);
    assert!(editor.present_frame(frame));
    down(
        &mut editor,
        CanvasPoint {
            x: hit.x + 7.,
            y: hit.y,
        },
        false,
    )?;
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .find(|v| v.selected)
            .unwrap()
            .id,
        0
    );
    editor.cancel_gesture();
    present(&mut editor)?;
    down(
        &mut editor,
        CanvasPoint {
            x: hit.x + 9.,
            y: hit.y,
        },
        false,
    )?;
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        0
    );
    Ok(())
}

#[test]
fn invalid_drag_rolls_back_candidate_without_losing_redo() -> anyhow::Result<()> {
    for release in [false, true] {
        let (mut editor, _dir) = setup()?;
        editor.execute(EditorCommand::SetVisible(false))?;
        editor.execute(EditorCommand::Undo)?;
        present(&mut editor)?;
        let original = editor.document().unwrap().selected_vertex_positions();
        let start = editor.overlay().vertices[0].position;
        down(&mut editor, start, false)?;
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint {
                x: start.x + 20.,
                y: start.y,
            },
            left_down: true,
        })?;
        let point = CanvasPoint { x: f32::NAN, y: 0. };
        let event = if release {
            CanvasInput::Up { point }
        } else {
            CanvasInput::Move {
                point,
                left_down: true,
            }
        };
        assert!(editor.canvas_input(event).is_err());
        assert!(!editor.gesture_active());
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            original
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
        present(&mut editor)?;
    }
    Ok(())
}

#[test]
fn bound_mesh_stays_visible_but_cannot_start_base_vertex_edit() -> anyhow::Result<()> {
    let (document, _dir) = support::sample_document_with_binding(true)?;
    let mut editor = Editor::from_document(document);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    present(&mut editor)?;
    let overlay = editor.overlay();
    assert!(!overlay.vertices.is_empty());
    assert!(overlay.unavailable.unwrap().contains("parameter"));
    assert!(down(&mut editor, overlay.vertices[0].position, false).is_err());
    assert!(!editor.gesture_active());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn saving_across_resize_requires_a_new_frame_before_hit_testing() -> anyhow::Result<()> {
    let (mut editor, dir) = setup()?;
    let old = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(old.clone()));
    let point = editor.overlay().vertices[0].position;
    let id = editor.active_id().unwrap();
    let task = editor.prepare_save(id, &dir.path().join("saved"), false)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 800.,
        height: 400.,
        scale_factor: 1.,
    });
    editor.finish_save(&mut task.run())?;
    assert!(down(&mut editor, point, false).is_err());
    let next = editor.preview_if_needed()?.unwrap().canvas.unwrap();
    assert_ne!(next.version.viewport, old.version.viewport);
    assert!(editor.present_frame(next));
    Ok(())
}

#[test]
fn frame_must_belong_to_active_document_even_with_matching_request_number() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let mut old = editor.preview()?.canvas.unwrap();
    editor.new_document()?;
    let current = editor.preview()?.canvas.unwrap();
    old.version.request = current.version.request;
    assert!(!editor.present_frame(old));
    Ok(())
}

#[test]
fn retained_geometry_is_not_drawn_without_its_matching_image() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    assert!(
        !editor
            .overlay_for_frame(Some(frame.version))
            .vertices
            .is_empty()
    );
    assert!(editor.overlay_for_frame(None).vertices.is_empty());
    let mut wrong = frame.version;
    wrong.request += 1;
    assert!(editor.overlay_for_frame(Some(wrong)).vertices.is_empty());
    editor.new_document()?;
    assert!(
        editor
            .overlay_for_frame(Some(frame.version))
            .vertices
            .is_empty()
    );
    Ok(())
}

#[test]
fn click_without_moving_is_overlay_only() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let before = editor.evaluation_count();
    let point = editor.overlay().vertices[0].position;
    down(&mut editor, point, false)?;
    up(&mut editor, point)?;
    assert!(editor.overlay().vertices[0].selected);
    assert!(editor.preview_if_needed()?.is_none());
    assert_eq!(editor.evaluation_count(), before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
