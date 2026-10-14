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
fn zoom_uses_actual_pixels_independent_of_viewport_and_display_scale() {
    for (width, height) in [(640., 480.), (1600., 1200.)] {
        let canvas = kasane_core::Canvas::new(
            width,
            height,
            kasane_core::Vec2::new(width / 2., height / 2.),
            100.,
        );
        for (viewport_width, viewport_height) in [(813., 517.), (300., 200.), (1800., 1400.)] {
            for dpi in [1., 1.5, 2., 3.] {
                for zoom in [0.25, 1., 2.75] {
                    let transform = ViewportTransform::new(
                        canvas,
                        CanvasMetrics {
                            width: viewport_width,
                            height: viewport_height,
                            scale_factor: dpi,
                        },
                        ViewState {
                            zoom,
                            pan: CanvasPoint { x: 13., y: -27. },
                        },
                    )
                    .unwrap();
                    let origin = transform.to_canvas(ModelPoint::default());
                    let corner = transform.to_canvas(ModelPoint {
                        x: width,
                        y: height,
                    });
                    assert!(((corner.x - origin.x) * dpi - width * zoom).abs() < 0.001);
                    assert!(((corner.y - origin.y) * dpi - height * zoom).abs() < 0.001);
                    let center = transform.to_canvas(ModelPoint {
                        x: width / 2.,
                        y: height / 2.,
                    });
                    assert!((center.x - (viewport_width / 2. + 13.)).abs() < 0.001);
                    assert!((center.y - (viewport_height / 2. - 27.)).abs() < 0.001);
                }
            }
        }
    }
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
    // The movement assertions below explicitly exercise 100% on a 2x display.
    editor.reset_view();
    present(&mut editor)?;
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
    // At 100% on a 2x display, 24 logical pixels move 48 source pixels.
    assert_eq!(positions[0].1, ModelPoint { x: 176., y: 56. });
    assert_eq!(positions[1].1, ModelPoint { x: 280., y: 160. });
    let scene = editor.preview()?.scene;
    assert_eq!(
        scene.viewport_frame.drawables[0].uvs,
        original.viewport_frame.drawables[0].uvs
    );
    assert_eq!(
        scene.viewport_frame.drawables[0].indices,
        original.viewport_frame.drawables[0].indices
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
    editor.execute(EditorCommand::SetObjectVisible {
        object: editor
            .document()
            .unwrap()
            .context()
            .selection
            .clone()
            .unwrap(),
        visible: false,
    })?;
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
                editor.dispatch(editor.command(CommandId::GlueTool))?;
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
fn completed_views_progress_monotonically_with_matching_geometry() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let old = editor.preview()?.canvas.unwrap();
    editor.zoom_by(0.25);
    let latest = editor.preview()?.canvas.unwrap();
    assert!(
        editor.present_frame(old.clone()),
        "continuous input must not starve presentation"
    );
    assert!(editor.present_frame(latest.clone()));
    assert!(
        !editor.present_frame(old.clone()),
        "completed frames cannot move backwards"
    );
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
fn continuous_control_drag_presents_completed_poses_without_changing_its_target()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let before_drag = editor.preview()?.canvas.unwrap();
    let start = editor.overlay().vertices[0].position;
    let at = |x| CanvasPoint {
        x: start.x + x,
        y: start.y,
    };
    down(&mut editor, start, false)?;
    let mut completed = Vec::new();
    for x in [10., 20., 30.] {
        assert!(editor.canvas_input(CanvasInput::Move {
            point: at(x),
            left_down: true,
        })?);
        completed.push(editor.preview()?.canvas.unwrap());
    }
    assert!(
        !editor.present_frame(before_drag),
        "a frame requested before this control edit must not be admitted"
    );
    let mut future = completed[0].clone();
    future.version.scene = completed[2].version.scene + 1;
    assert!(!editor.present_frame(future));
    assert!(editor.present_frame(completed[0].clone()));
    assert_eq!(editor.overlay().vertices[0].position, at(10.));
    assert!(
        down(&mut editor, at(10.), false).is_err(),
        "new pointer presses still require the current model frame"
    );
    assert!(editor.present_frame(completed[1].clone()));
    assert_eq!(editor.overlay().vertices[0].position, at(20.));
    assert!(!editor.present_frame(completed[0].clone()));
    // Continue from the original press, even though the displayed pose is behind.
    assert!(editor.canvas_input(CanvasInput::Move {
        point: at(40.),
        left_down: true,
    })?);
    let current = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(completed[2].clone()));
    assert_eq!(editor.overlay().vertices[0].position, at(30.));
    up(&mut editor, at(40.))?;
    assert!(!editor.present_frame(completed[2].clone()));
    assert!(editor.present_frame(current));
    assert_eq!(editor.overlay().vertices[0].position, at(40.));
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    present(&mut editor)?;
    assert_eq!(editor.overlay().vertices[0].position, start);
    Ok(())
}

#[test]
fn delayed_control_poses_are_rejected_when_the_gesture_ends_or_is_replaced() -> anyhow::Result<()> {
    for finish in 0..6 {
        let (mut editor, dir) = setup()?;
        let start = editor.overlay().vertices[0].position;
        down(&mut editor, start, false)?;
        let mut frames = Vec::new();
        for x in [10., 20.] {
            editor.canvas_input(CanvasInput::Move {
                point: CanvasPoint {
                    x: start.x + x,
                    y: start.y,
                },
                left_down: true,
            })?;
            frames.push(editor.preview()?.canvas.unwrap());
        }
        assert!(editor.present_frame(frames[0].clone()));
        match finish {
            0 => editor.cancel_gesture(),
            1 => up(
                &mut editor,
                CanvasPoint {
                    x: start.x + 30.,
                    y: start.y,
                },
            )?,
            2 => editor.execute(EditorCommand::SetTool(ToolId::Hand))?,
            3 => {
                let id = editor.active_id().unwrap();
                editor.new_document()?;
                editor.activate(id)?;
            }
            4 => {
                let mut metrics = editor.canvas_metrics();
                metrics.width += 10.;
                editor.set_canvas_metrics(metrics);
            }
            _ => {
                let id = editor.active_id().unwrap();
                let task = editor.prepare_save(id, &dir.path().join("saved"), false)?;
                editor.finish_save(&mut task.run())?;
            }
        }
        assert!(!editor.gesture_active());
        assert!(!editor.present_frame(frames[1].clone()));
        present(&mut editor)?;
        if finish == 0 {
            // A later control gesture must not reopen the previous pose's lifetime.
            down(&mut editor, start, false)?;
            editor.canvas_input(CanvasInput::Move {
                point: CanvasPoint {
                    x: start.x + 5.,
                    y: start.y,
                },
                left_down: true,
            })?;
            editor.preview()?;
            assert!(!editor.present_frame(frames[1].clone()));
        }
    }
    Ok(())
}

#[test]
fn hit_ties_are_stable_and_hit_radius_is_in_logical_pixels() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let mut frame = editor.preview()?.canvas.unwrap();
    let mut geometry = (*frame.geometry).clone();
    let p = geometry.meshes[0].vertices[0].1;
    let meshes = std::sync::Arc::make_mut(&mut geometry.meshes);
    let mesh = std::sync::Arc::make_mut(&mut meshes[0]);
    mesh.vertices[1].1 = p;
    mesh.vertices.reverse();
    frame.geometry = std::sync::Arc::new(geometry);
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
    up(
        &mut editor,
        CanvasPoint {
            x: hit.x + 9.,
            y: hit.y,
        },
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
        editor.execute(EditorCommand::SetObjectVisible {
            object: editor
                .document()
                .unwrap()
                .context()
                .selection
                .clone()
                .unwrap(),
            visible: false,
        })?;
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

#[test]
fn delayed_views_cannot_cross_edits_resizes_or_activation_epochs() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original_id = editor.active_id().unwrap();
    let before_edit = editor.preview()?.canvas.unwrap();
    editor.execute(EditorCommand::PreviewOpacity(0.4))?;
    let edited = editor.preview()?.canvas.unwrap();
    assert!(!editor.present_frame(before_edit));
    assert!(editor.present_frame(edited));
    let before_resize = editor.preview()?.canvas.unwrap();
    let mut metrics = editor.canvas_metrics();
    metrics.width += 10.;
    editor.set_canvas_metrics(metrics);
    let _resized = editor.preview()?;
    assert!(!editor.present_frame(before_resize));
    let previous_visit = editor.preview()?.canvas.unwrap();
    editor.new_document()?;
    editor.activate(original_id)?;
    let current_visit = editor.preview()?.canvas.unwrap();
    assert!(!editor.present_frame(previous_visit));
    assert!(editor.present_frame(current_visit));
    Ok(())
}

#[test]
fn opacity_preview_refreshes_picking_and_preserves_undo() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original = editor.preview()?;
    editor.execute(EditorCommand::PreviewOpacity(0.))?;
    let hidden = editor.preview()?;
    assert!(!std::sync::Arc::ptr_eq(
        &original.canvas.unwrap().geometry,
        &hidden.canvas.unwrap().geometry
    ));
    assert_eq!(hidden.scene.viewport_frame.drawables[0].opacity, 0.);
    editor.execute(EditorCommand::PreviewOpacity(0.5))?;
    let half = editor.preview()?;
    assert_eq!(half.scene.viewport_frame.drawables[0].opacity, 0.5);
    editor.execute(EditorCommand::FinishEdit)?;
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.preview()?.scene.viewport_frame.drawables[0].opacity,
        original.scene.viewport_frame.drawables[0].opacity
    );
    Ok(())
}

#[test]
fn zoom_at_preserves_model_point_under_mouse() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let t1 = editor.preview()?.canvas.unwrap().transform;
    let mouse = CanvasPoint { x: 380., y: 190. };
    let model_point = t1.to_model(mouse);

    // Zoom in at mouse
    editor.zoom_at(0.5, Some(mouse));
    let t2 = editor.preview()?.canvas.unwrap().transform;
    let mouse_after = t2.to_canvas(model_point);
    assert!(
        (mouse_after.x - mouse.x).abs() < 1e-3,
        "x drifted: {} vs {}",
        mouse_after.x,
        mouse.x
    );
    assert!(
        (mouse_after.y - mouse.y).abs() < 1e-3,
        "y drifted: {} vs {}",
        mouse_after.y,
        mouse.y
    );

    // Zoom out at another mouse position
    let mouse2 = CanvasPoint { x: 120., y: 450. };
    let model_point2 = t2.to_model(mouse2);
    editor.zoom_at(-0.75, Some(mouse2));
    let t3 = editor.preview()?.canvas.unwrap().transform;
    let mouse2_after = t3.to_canvas(model_point2);
    assert!(
        (mouse2_after.x - mouse2.x).abs() < 1e-3,
        "x drifted: {} vs {}",
        mouse2_after.x,
        mouse2.x
    );
    assert!(
        (mouse2_after.y - mouse2.y).abs() < 1e-3,
        "y drifted: {} vs {}",
        mouse2_after.y,
        mouse2.y
    );

    Ok(())
}

#[test]
fn zoom_by_preserves_viewport_center() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    // Pan first so pan != 0
    editor.pan_by(CanvasPoint { x: 50., y: -30. });
    let t1 = editor.preview()?.canvas.unwrap().transform;
    let metrics = editor.canvas_metrics();
    let center = CanvasPoint {
        x: metrics.width / 2.,
        y: metrics.height / 2.,
    };
    let model_center = t1.to_model(center);

    editor.zoom_by(0.4);
    let t2 = editor.preview()?.canvas.unwrap().transform;
    let center_after = t2.to_canvas(model_center);
    assert!(
        (center_after.x - center.x).abs() < 1e-3,
        "center x drifted: {} vs {}",
        center_after.x,
        center.x
    );
    assert!(
        (center_after.y - center.y).abs() < 1e-3,
        "center y drifted: {} vs {}",
        center_after.y,
        center.y
    );

    Ok(())
}

#[test]
fn absolute_zoom_clamps_and_preserves_anchor_without_changing_history() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    editor.pan_by(CanvasPoint { x: 37., y: -12. });
    let anchor = CanvasPoint { x: 120., y: 90. };
    let model = editor.preview()?.canvas.unwrap().transform.to_model(anchor);
    for (requested, expected) in [(2., 2.), (100., MAX_ZOOM), (0., MIN_ZOOM)] {
        editor.set_zoom_at(requested, Some(anchor));
        assert_eq!(editor.view().zoom, expected);
        let after = editor.preview()?.canvas.unwrap().transform.to_canvas(model);
        assert!((after.x - anchor.x).abs() < 1e-3);
        assert!((after.y - anchor.y).abs() < 1e-3);
    }
    let before = editor.view();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        editor.set_zoom_at(invalid, Some(anchor));
        assert_eq!(editor.view().zoom, before.zoom);
        assert_eq!(editor.view().pan, before.pan);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
