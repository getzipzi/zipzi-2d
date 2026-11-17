use super::support;
use zipzi_editor_core::*;
#[path = "../../../../fixtures/multi-mesh-edit.rs"]
mod fixture;
fn objects() -> Vec<ObjectRef> {
    [2, 4]
        .map(|n| ObjectRef::new(ObjectKind::Mesh, fixture::id(n)))
        .into()
}
fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    setup_with_glue(false)
}
fn setup_with_glue(glue: bool) -> anyhow::Result<(Editor, tempfile::TempDir)> {
    setup_meshes(glue, false)
}
fn setup_meshes(glue: bool, overlapping: bool) -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let mut session = fixture::create(
        dir.path(),
        include_bytes!("../fixtures/alpha-asymmetric.png"),
    )?;
    if overlapping {
        let b = session.snapshot().document().mesh(&fixture::id(4)).unwrap();
        let positions = b
            .vertex_ids
            .iter()
            .zip(&b.base_positions)
            .map(|(&id, p)| (id, zipzi_core::Vec2::new(p.x - 100., p.y - 100.)))
            .collect::<Vec<_>>();
        crate::support::publish_fixture(&mut session, "Overlap meshes", |edit| {
            edit.set_vertex_positions(&fixture::id(4), &positions)
        })?;
    }
    if glue {
        crate::support::publish_fixture(&mut session, "Glue", |edit| {
            crate::support::insert_glue(
                edit,
                zipzi_core::Glue {
                    id: fixture::id(5),
                    mesh_a_id: fixture::id(2).into(),
                    mesh_b_id: fixture::id(4).into(),
                    intensity: 0.5,
                    pairs: vec![zipzi_core::GlueVertexPair {
                        id: fixture::id(6),
                        vertex_a: Some(0),
                        vertex_b: Some(0),
                        weight_a: 0.5,
                        weight_b: 0.5,
                    }],
                    ..Default::default()
                },
            )
        })?;
    }
    let saved = crate::support::save_fixture(&mut session, &dir.path().join("multi.zipzi"))?;
    let mut document = Document::open(&saved.path)?;
    document.select_objects(objects())?;
    let mut editor = crate::support::editor_from_document(document);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    assert!(editor.command_available(editor.command(CommandId::ManualMesh)));
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    support::present_canvas(&mut editor)?;
    Ok((editor, dir))
}
fn meshes(editor: &Editor) -> Vec<zipzi_core::Mesh> {
    let document = editor.document().unwrap();
    let host = document.runtime_host();
    let captured =
        crate::support::published_snapshot(&host, document.runtime_session_id()).unwrap();
    let model = captured.document();
    objects()
        .iter()
        .map(|o| model.mesh(&o.id).unwrap().clone())
        .collect()
}
fn screen(editor: &mut Editor, x: f32, y: f32) -> anyhow::Result<ViewportPoint> {
    Ok(support::present_canvas(editor)?
        .transform
        .to_viewport(ModelPoint { x, y }))
}

#[test]
fn mesh_edit_ownership_colors_survive_target_switches_and_reordered_selection() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let colors = |editor: &Editor| {
        let mut colors: Vec<_> = editor
            .overlay()
            .mesh_styles
            .into_iter()
            .map(|s| (s.object.id, s.color_index))
            .collect();
        colors.sort();
        colors
    };
    let original = colors(&editor);
    assert_eq!(original.len(), 2);
    assert_ne!(original[0].1, original[1].1);
    click(&mut editor, 20., 20., false)?;
    click(&mut editor, 140., 140., true)?;
    assert_eq!(colors(&editor), original);
    let overlay = editor.overlay();
    let frame = support::present_canvas(&mut editor)?;
    for style in &overlay.mesh_styles {
        let geometry = frame.geometry.mesh(&style.object.id).unwrap();
        let vertices = &overlay.vertices[style.vertices.clone()];
        assert_eq!(vertices.len(), geometry.vertices.len());
        assert_eq!(vertices.iter().filter(|v| v.selected).count(), 1);
        for v in vertices {
            assert!(
                geometry
                    .vertices
                    .iter()
                    .any(|(id, p)| *id == v.id && frame.transform.to_viewport(*p) == v.position)
            );
        }
    }
    editor.exit_manual_mesh();
    editor.execute(EditorCommand::SelectObjects(
        objects().into_iter().rev().collect(),
    ))?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    support::present_canvas(&mut editor)?;
    assert_eq!(colors(&editor), original);
    Ok(())
}

#[test]
fn double_click_entry_retains_multi_selection_and_binds_the_clicked_viewport() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let original = meshes(&editor);
    editor.exit_manual_mesh();
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(
        other,
        ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        },
    );
    let binding = editor.viewport_binding(other).unwrap();
    let frame = editor.preview_for(binding)?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    let point = frame.transform.to_viewport(ModelPoint { x: 140., y: 140. });
    assert!(editor.try_enter_mesh_edit_at(binding, point)?);
    assert_eq!(editor.manual_targets().len(), 2);
    assert_eq!(editor.manual_target().unwrap().object, objects()[1]);
    let frame = editor.preview_for(binding)?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    assert!(!editor.overlay_for_viewport(binding).vertices.is_empty());
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.gesture_active());
    assert!(!editor.try_enter_mesh_edit_at(binding, point)?);
    Ok(())
}
fn click(editor: &mut Editor, x: f32, y: f32, shift: bool) -> anyhow::Result<()> {
    let point = screen(editor, x, y)?;
    support::pointer_input(
        editor,
        CanvasInput::Down {
            point,
            shift,
            alt: false,
            primary: false,
        },
    )?;
    support::pointer_input(editor, CanvasInput::Up { point })?;
    Ok(())
}
fn selected(editor: &Editor) -> usize {
    let overlay = editor.overlay_for_viewport(editor.primary_viewport_binding().unwrap());
    overlay.vertices.iter().filter(|v| v.selected).count()
        + overlay
            .glues
            .iter()
            .flat_map(|g| &g.points)
            .filter(|p| p.state == GluePointState::Binding && p.selected)
            .count()
}

fn glues(editor: &Editor) -> Vec<zipzi_core::Glue> {
    let document = editor.document().unwrap();
    let snapshot =
        support::published_snapshot(&document.runtime_host(), document.runtime_session_id())
            .unwrap();
    let model = snapshot.document();
    model
        .glue_order()
        .iter()
        .map(|id| model.glue(id).unwrap().clone())
        .collect()
}

#[test]
fn glue_tool_chooses_labels_before_painting_and_colors_only_the_selected_glue() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup_meshes(false, true)?;
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    click(&mut editor, 60., 65., false)?;
    let first = glues(&editor)[0].clone();
    let frame = support::present_canvas(&mut editor)?;
    editor.set_manual_glue_target(frame.version.binding, None)?;
    click(&mut editor, 70., 75., false)?;
    let glues = glues(&editor);
    assert_eq!(glues.len(), 2);
    let second = glues.iter().find(|g| g.id != first.id).unwrap().clone();
    editor.exit_manual_mesh();
    editor.execute(EditorCommand::SelectObjects(vec![]))?;
    support::present_canvas(&mut editor)?;
    assert!(editor.overlay().glue_labels.is_empty());
    editor.dispatch(editor.command(CommandId::GlueTool))?;
    let frame = support::present_canvas(&mut editor)?;
    let history = editor.document().unwrap().history_lengths();
    assert_eq!(editor.overlay().glue_labels.len(), 2);
    assert!(!editor.glue_editing_weights());
    assert!(editor.overlay().glue_weight_meshes.is_empty());
    let blank = frame.transform.to_viewport(ModelPoint { x: 100., y: 110. });
    editor.hover_canvas(frame.version.binding, Some(blank))?;
    assert!(editor.overlay().selection_brush.is_none());
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: blank,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: blank })?;
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert!(!editor.document().unwrap().has_pending_edit());

    for target in [&first, &second] {
        let overlay = editor.overlay();
        let label = overlay
            .glue_labels
            .iter()
            .find(|l| l.object.id == target.id)
            .unwrap();
        let at = ViewportPoint {
            x: label.position.x + 6.,
            y: label.position.y + 6.,
        };
        editor.hover_canvas(frame.version.binding, Some(at))?;
        assert!(
            editor.overlay().selection_brush.is_none(),
            "labels remain clickable without a brush cursor"
        );
        support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: at,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        support::pointer_input(&mut editor, CanvasInput::Up { point: at })?;
        let selected_frame = support::present_canvas(&mut editor)?;
        assert!(editor.glue_editing_weights());
        let overlay = editor.overlay();
        assert_eq!(overlay.glue_labels.len(), 2);
        assert_eq!(overlay.glue_weight_meshes.len(), 2);
        for mesh in &overlay.glue_weight_meshes {
            let side_a = mesh.object.id == target.mesh_a_id.as_str();
            let geometry = selected_frame.geometry.mesh(&mesh.object.id).unwrap();
            assert_eq!(mesh.triangles.len(), geometry.triangles.len());
            for (&ids, (points, red)) in geometry.triangles.iter().zip(&mesh.triangles) {
                for i in 0..3 {
                    let expected = if ids[i]
                        == if side_a {
                            target.pairs[0].vertex_a.unwrap()
                        } else {
                            target.pairs[0].vertex_b.unwrap()
                        } {
                        0.5
                    } else if side_a {
                        1.
                    } else {
                        0.
                    };
                    assert_eq!(
                        red[i], expected,
                        "only this Glue's vertices mix the peer's color"
                    );
                    let raw = geometry.vertices.iter().find(|v| v.0 == ids[i]).unwrap().1;
                    assert_eq!(points[i], selected_frame.transform.to_viewport(raw));
                }
            }
        }
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            history,
            "choosing a label never paints or creates an undo step"
        );
    }
    let frame = support::present_canvas(&mut editor)?;
    editor.hover_canvas(frame.version.binding, Some(blank))?;
    assert!(editor.overlay().selection_brush.is_some());
    let retained = editor.overlay().glue_weight_meshes;
    editor.execute(EditorCommand::SetGlueBrushSettings(
        BrushSelectionSettings {
            weight: 1.,
            diameter: 512.,
            ..Default::default()
        },
    ))?;
    editor.execute(EditorCommand::SetGlueBrushMode(GlueBrushMode::Release))?;
    let at = editor.overlay().glue_weights[0].a;
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: at,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    support::present_canvas(&mut editor)?;
    let released = editor.overlay().glue_weight_meshes;
    assert_ne!(released, retained);
    for mesh in &released {
        let expected = if mesh.object.id == second.mesh_a_id.as_str() {
            1.
        } else {
            0.
        };
        assert!(
            mesh.triangles
                .iter()
                .all(|(_, red)| red.iter().all(|v| *v == expected))
        );
    }
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    support::present_canvas(&mut editor)?;
    assert_eq!(editor.overlay().glue_weight_meshes, retained);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    editor.dispatch(editor.command(CommandId::SelectTool))?;
    support::present_canvas(&mut editor)?;
    assert!(editor.overlay().glue_weight_meshes.is_empty());
    Ok(())
}

#[test]
fn glue_create_reuse_bind_paint_cancel_and_reopen() -> anyhow::Result<()> {
    let (mut editor, dir) = setup_meshes(false, true)?;
    let original = meshes(&editor);
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    click(&mut editor, 60., 65., false)?;
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (1, 0),
        "{:?}",
        editor.canvas_notice()
    );
    support::present_canvas(&mut editor)?;
    assert_eq!(selected(&editor), 1);
    let inserted = meshes(&editor);
    for (before, after) in original.iter().zip(&inserted) {
        assert_eq!(after.vertex_ids.len(), before.vertex_ids.len() + 1);
    }
    click(&mut editor, 60., 65., false)?;
    assert_eq!(selected(&editor), 1);
    assert_eq!(meshes(&editor), inserted);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));

    support::present_canvas(&mut editor)?;
    let created = glues(&editor).pop().unwrap();
    assert_eq!(created.mesh_a_id.as_str(), objects()[0].id);
    assert_eq!(created.mesh_b_id.as_str(), objects()[1].id);
    assert_eq!(created.pairs.len(), 1);
    assert_eq!(
        [created.pairs[0].weight_a, created.pairs[0].weight_b],
        [0.5, 0.5]
    );
    assert_eq!(created.intensity, 1.);
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueBind))?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));

    editor.exit_manual_mesh();
    support::present_canvas(&mut editor)?;
    let label = editor.overlay().glue_labels[0].position;
    let at = ViewportPoint {
        x: label.x + 4.,
        y: label.y + 4.,
    };
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: at,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    assert_eq!(editor.active_tool(), ToolId::Glue);
    assert_eq!(
        editor
            .document()
            .unwrap()
            .context()
            .selection
            .as_ref()
            .unwrap()
            .id,
        created.id
    );
    editor.execute(EditorCommand::SetGlueBrushMode(GlueBrushMode::A))?;
    editor.execute(EditorCommand::SetGlueBrushSettings(
        BrushSelectionSettings {
            weight: 1.,
            ..Default::default()
        },
    ))?;
    let at = screen(&mut editor, 60., 65.)?;
    let down = CanvasInput::Down {
        point: at,
        shift: false,
        alt: false,
        primary: false,
    };
    assert!(support::pointer_input(&mut editor, down)?);
    assert_eq!(glues(&editor)[0], created);
    assert_eq!(editor.overlay().glue_weights[0].weights, [0., 0.5]);
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    support::present_canvas(&mut editor)?;
    assert!(support::pointer_input(&mut editor, down)?);
    support::pointer_input(&mut editor, CanvasInput::Up { point: at })?;
    let painted = glues(&editor).pop().unwrap();
    assert_eq!(
        [painted.pairs[0].weight_a, painted.pairs[0].weight_b],
        [0., 0.5]
    );
    assert_eq!(painted.pairs[0].id, created.pairs[0].id);
    assert_eq!(painted.intensity, created.intensity);
    assert_eq!(meshes(&editor), inserted);
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(glues(&editor), vec![created]);
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(glues(&editor), vec![painted.clone()]);

    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("glue.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let reopened = support::editor_from_document(Document::open(&saved.path)?);
    assert_eq!(glues(&reopened), vec![painted.clone()]);
    assert_eq!(meshes(&reopened), inserted);

    editor.execute(EditorCommand::SelectObjects(objects()))?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    click(&mut editor, 60., 65., false)?;
    assert_eq!(
        selected(&editor),
        1,
        "the bound vertices are one visible Glue point"
    );
    let frame = support::present_canvas(&mut editor)?;
    editor.set_manual_glue_target(frame.version.binding, Some(painted.id.clone()))?;
    assert_eq!(
        editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .unbind_count,
        1
    );
    editor.dispatch(editor.command(CommandId::GlueUnbind))?;
    let empty = glues(&editor).pop().unwrap();
    assert_eq!(empty.id, painted.id);
    assert!(empty.pairs.is_empty());
    assert_eq!(empty.intensity, painted.intensity);
    assert_eq!(meshes(&editor), inserted);
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(glues(&editor), vec![painted]);
    Ok(())
}

#[test]
fn glue_add_point_rolls_back_outside_peer_source() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original = meshes(&editor);
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    click(&mut editor, 60., 65., false)?;
    assert_eq!(meshes(&editor), original);
    assert!(glues(&editor).is_empty());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(editor.canvas_notice().is_some());
    Ok(())
}

#[test]
fn glue_external_publication_retires_paint_capture() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup_with_glue(true)?;
    editor.exit_manual_mesh();
    let original = glues(&editor).pop().unwrap();
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Glue,
        &original.id,
    )))?;
    editor.execute(EditorCommand::SetGlueBrushMode(GlueBrushMode::A))?;
    let document = editor.document().unwrap();
    let host = document.runtime_host();
    let session = document.runtime_session_id();
    let version = host
        .read()
        .session_read(host.client_id(), session)?
        .version();
    let remote = host.write().register_client()?;
    let candidate = {
        let mut runtime = host.write();
        runtime.attach(remote, session)?;
        let draft = runtime.begin_draft(remote, version)?;
        runtime
            .batch(remote, draft, |commands| {
                let mut changed = original.clone();
                changed.intensity = 0.875;
                commands.replace_glue(changed)
            })?
            .0
    };

    let frame = support::present_canvas(&mut editor)?;
    let point = editor.overlay().glue_weights[0].a;
    let stroke = editor.begin_canvas_gesture(frame.version.binding, point, false, false, false)?;
    assert!(stroke.model_changed);
    let token = stroke.capture.unwrap();
    host.write()
        .publish(remote, candidate, "Change Glue strength")?;
    assert!(editor.sync_runtime()?);
    assert!(!editor.viewport_interaction_valid(token));
    assert!(
        editor
            .update_canvas_gesture(token, CanvasInput::Up { point })
            .is_err()
    );
    let current = glues(&editor).pop().unwrap();
    assert_eq!(current.intensity, 0.875);
    assert_eq!(current.pairs, original.pairs);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn manual_cursor_gestures_keep_the_pressed_operation_and_scope() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(
        other,
        ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        },
    );
    let secondary = editor.viewport_binding(other).unwrap();
    let frame = editor.preview_for(secondary)?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    let point = screen(&mut editor, 60., 60.)?;
    // Plain Select clicks create points; crossing the threshold becomes a region.
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    assert_eq!(
        editor.overlay().active_gesture,
        Some(CanvasGestureKind::MeshAdd)
    );
    let end = ViewportPoint {
        x: point.x + 12.,
        y: point.y + 12.,
    };
    support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    assert_eq!(
        editor.overlay().active_gesture,
        Some(CanvasGestureKind::BoxSelect)
    );
    let other_overlay = editor.overlay_for_viewport(secondary);
    assert_eq!(other_overlay.active_gesture, None);
    assert_eq!(other_overlay.marquee, None);
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(editor.overlay().active_gesture, None);
    click(&mut editor, 60., 60., false)?;
    for (shift, kind) in [
        (false, CanvasGestureKind::MeshConnect),
        (true, CanvasGestureKind::MeshDelete),
    ] {
        let point = screen(&mut editor, 60., 60.)?;
        support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point,
                shift,
                alt: false,
                primary: true,
            },
        )?;
        assert_eq!(editor.overlay().active_gesture, Some(kind));
        support::pointer_input(
            &mut editor,
            CanvasInput::ModifiersChanged {
                shift: false,
                alt: false,
            },
        )?;
        assert_eq!(editor.overlay().active_gesture, Some(kind));
        support::pointer_input(&mut editor, CanvasInput::Cancel)?;
        assert_eq!(editor.overlay().active_gesture, None);
    }
    // Selecting points on two meshes promotes dragging into the shared gesture.
    click(&mut editor, 160., 180., false)?;
    click(&mut editor, 60., 60., false)?;
    click(&mut editor, 160., 180., true)?;
    let point = screen(&mut editor, 60., 60.)?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    assert_eq!(
        editor.overlay().active_gesture,
        Some(CanvasGestureKind::Move)
    );
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    Ok(())
}
fn region(editor: &mut Editor, lasso: bool, shift: bool, alt: bool) -> anyhow::Result<()> {
    editor.dispatch(editor.command(if lasso {
        CommandId::ManualLasso
    } else {
        CommandId::ManualSelect
    }))?;
    let from = screen(editor, 10., 10.)?;
    let to = screen(editor, 150., 150.)?;
    support::pointer_input(
        editor,
        CanvasInput::Down {
            point: from,
            shift,
            alt,
            primary: false,
        },
    )?;
    if lasso {
        let corner = ViewportPoint { x: to.x, y: from.y };
        support::pointer_input(
            editor,
            CanvasInput::Move {
                point: corner,
                left_down: true,
            },
        )?;
        support::pointer_input(
            editor,
            CanvasInput::Move {
                point: to,
                left_down: true,
            },
        )?;
        support::pointer_input(
            editor,
            CanvasInput::Up {
                point: ViewportPoint { x: from.x, y: to.y },
            },
        )?;
    } else {
        support::pointer_input(editor, CanvasInput::Up { point: to })?;
    }
    Ok(())
}
#[test]
fn multi_mesh_regions_share_selection_with_mesh_scoped_vertex_ids() -> anyhow::Result<()> {
    for lasso in [false, true] {
        let (mut editor, _dir) = setup()?;
        let original = meshes(&editor);
        assert_eq!(editor.manual_targets().len(), 2);
        assert_eq!(editor.document().unwrap().context().selections, objects());
        assert_eq!(
            editor
                .overlay_for_viewport(editor.primary_viewport_binding().unwrap())
                .vertices
                .len(),
            8
        );
        assert!(!editor.command_available(editor.command(CommandId::ManualSourceView)));
        for (x, y, name) in [
            (40., 40., Some("Mesh 2")),
            (140., 140., Some("Mesh 4")),
            (153., 166., Some("Mesh 4")),
            (250., 250., None),
        ] {
            let point = screen(&mut editor, x, y)?;
            let binding = editor.primary_viewport_binding().unwrap();
            editor.hover_canvas(binding, Some(point))?;
            assert_eq!(editor.overlay().hovered_object_label.as_deref(), name);
        }
        region(&mut editor, lasso, false, false)?;
        assert_eq!(selected(&editor), 5);
        region(&mut editor, lasso, false, true)?;
        assert_eq!(selected(&editor), 0);
        region(&mut editor, lasso, true, false)?;
        assert_eq!(selected(&editor), 5);
        editor.dispatch(editor.command(CommandId::SelectAll))?;
        assert_eq!(selected(&editor), 8);
        assert_eq!(meshes(&editor), original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        editor.exit_manual_mesh();
        editor.execute(EditorCommand::SelectObject(objects()[0].clone()))?;
        editor.enter_manual_mesh(editor.active_id().unwrap())?;
        let point = screen(&mut editor, 40., 40.)?;
        let binding = editor.primary_viewport_binding().unwrap();
        editor.hover_canvas(binding, Some(point))?;
        assert_eq!(
            editor.overlay().hovered_object_label.as_deref(),
            Some("Mesh 2")
        );
        editor.hover_canvas(binding, None)?;
        assert!(editor.overlay().hovered_object_label.is_none());
    }
    Ok(())
}
#[test]
fn multi_mesh_drag_is_atomic_recovers_and_has_one_history_step() -> anyhow::Result<()> {
    for lasso in [false, true] {
        let (mut editor, _dir) = setup()?;
        if lasso {
            editor.dispatch(editor.command(CommandId::ManualLasso))?;
        }
        let original = meshes(&editor);
        click(&mut editor, 20., 20., false)?;
        click(&mut editor, 140., 140., true)?;
        assert_eq!(selected(&editor), 2);
        let from = screen(&mut editor, 140., 140.)?;
        let transform = support::present_canvas(&mut editor)?.transform;
        support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: from,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        let valid = transform.to_viewport(ModelPoint { x: 146., y: 144. });
        assert!(support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: valid,
                left_down: true
            }
        )?);
        let accepted_frame = editor.preview()?.canvas.unwrap();
        for before in &original {
            let after = accepted_frame.geometry.mesh(&before.id).unwrap();
            assert!((after.vertices[0].1.x - before.base_positions[0].x - 6.).abs() < 1e-4);
            assert!((after.vertices[0].1.y - before.base_positions[0].y - 4.).abs() < 1e-4);
        }
        assert_eq!(
            meshes(&editor),
            original,
            "Preview must leave the formal document unchanged"
        );
        let rejected = transform.to_viewport(ModelPoint { x: 190., y: 144. });
        assert!(!support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: rejected,
                left_down: true
            }
        )?);
        let rejected_frame = editor.preview()?.canvas.unwrap();
        for before in &original {
            assert_eq!(
                rejected_frame.geometry.mesh(&before.id).unwrap().vertices,
                accepted_frame.geometry.mesh(&before.id).unwrap().vertices,
                "One invalid mesh must reject the entire increment"
            );
        }
        support::pointer_input(&mut editor, CanvasInput::Up { point: valid })?;
        let accepted = meshes(&editor);
        for after in &accepted {
            assert_ne!(
                after.base_positions,
                original
                    .iter()
                    .find(|m| m.id == after.id)
                    .unwrap()
                    .base_positions
            );
        }
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.dispatch(editor.command(CommandId::Undo))?;
        assert_eq!(meshes(&editor), original);
        assert_eq!(editor.manual_targets().len(), 2);
        editor.dispatch(editor.command(CommandId::Redo))?;
        assert_eq!(meshes(&editor), accepted);
    }
    Ok(())
}
#[test]
fn multi_mesh_cancel_and_lost_button_restore_every_mesh() -> anyhow::Result<()> {
    for cancel in [false, true] {
        let (mut editor, _dir) = setup()?;
        let original = meshes(&editor);
        click(&mut editor, 20., 20., false)?;
        click(&mut editor, 140., 140., true)?;
        let point = screen(&mut editor, 140., 140.)?;
        support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        let moved = ViewportPoint {
            x: point.x + 6.,
            y: point.y - 4.,
        };
        support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: moved,
                left_down: true,
            },
        )?;
        assert!(editor.document().unwrap().has_pending_edit());
        if cancel {
            editor.dispatch(editor.command(CommandId::CancelGesture))?;
        } else {
            support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: moved,
                    left_down: false,
                },
            )?;
        }
        assert_eq!(meshes(&editor), original);
        assert_eq!(selected(&editor), 2);
        assert!(!editor.document().unwrap().has_pending_edit());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        assert!(editor.current_viewport_interaction().is_none());
    }
    Ok(())
}
#[test]
fn multi_mesh_insert_connect_and_delete_keep_topologies_independent() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original = meshes(&editor);
    click(&mut editor, 60., 60., false)?;
    let after_first = meshes(&editor);
    assert_eq!(after_first[0].vertex_ids.len(), 5);
    assert_eq!(after_first[1], original[1]);
    click(&mut editor, 160., 180., false)?;
    let after_both = meshes(&editor);
    assert_eq!(after_both[1].vertex_ids.len(), 5);
    let a = screen(&mut editor, 60., 60.)?;
    let b = screen(&mut editor, 160., 180.)?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: a,
            shift: false,
            alt: false,
            primary: true,
        },
    )?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: b })?;
    assert_eq!(meshes(&editor), after_both);
    click(&mut editor, 60., 60., false)?;
    click(&mut editor, 160., 180., true)?;
    editor.dispatch(editor.command(CommandId::ManualRemoveSelection))?;
    for (mut after, mut before) in meshes(&editor).into_iter().zip(original) {
        after.meshing = None;
        before.meshing = None;
        assert_eq!(after, before);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (3, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(meshes(&editor), after_both);
    Ok(())
}

#[test]
fn multi_mesh_glue_endpoints_move_jointly_and_refuse_deletion_atomically() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup_with_glue(true)?;
    let original = meshes(&editor);
    let before = support::present_canvas(&mut editor)?;
    let ids = objects();
    let points = ids
        .iter()
        .map(|o| before.geometry.mesh(&o.id).unwrap().vertices[0].1)
        .collect::<Vec<_>>();
    let middle = ModelPoint {
        x: (points[0].x + points[1].x) * 0.5,
        y: (points[0].y + points[1].y) * 0.5,
    };
    click(&mut editor, middle.x, middle.y, false)?;
    assert_eq!(selected(&editor), 1);
    assert!(
        editor
            .dispatch(editor.command(CommandId::ManualRemoveSelection))
            .is_err()
    );
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    let frame = support::present_canvas(&mut editor)?;
    let start = frame.transform.to_viewport(middle);
    editor.hover_canvas(frame.version.binding, Some(start))?;
    assert!(editor.overlay().manual_mesh_delete_unavailable);
    let end = frame.transform.to_viewport(ModelPoint {
        x: middle.x + 4.,
        y: middle.y + 3.,
    });
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    let after = support::present_canvas(&mut editor)?;
    for (index, o) in ids.iter().enumerate() {
        let actual = after.geometry.mesh(&o.id).unwrap().vertices[0].1;
        assert!((actual.x - points[index].x - 4.).abs() < 1e-3);
        assert!((actual.y - points[index].y - 3.).abs() < 1e-3);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(meshes(&editor), original);
    Ok(())
}

#[test]
fn multi_mesh_save_settles_a_shared_drag_and_roundtrips_both_meshes() -> anyhow::Result<()> {
    let (mut editor, dir) = setup()?;
    let original = meshes(&editor);
    click(&mut editor, 20., 20., false)?;
    click(&mut editor, 140., 140., true)?;
    let from = screen(&mut editor, 140., 140.)?;
    let to = screen(&mut editor, 146., 144.)?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: from,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: to,
            left_down: true,
        },
    )?;
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("grouped.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let moved = meshes(&editor);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(!editor.document().unwrap().has_pending_edit());
    assert!(editor.current_viewport_interaction().is_none());
    let reopened = crate::support::editor_from_document(Document::open(&saved.path)?);
    assert_eq!(meshes(&reopened), moved);
    for (before, after) in original.iter().zip(moved) {
        assert_ne!(before.base_positions, after.base_positions);
    }
    Ok(())
}

#[test]
fn glue_stitch_entry_rejects_other_selections_for_every_command_source() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup_with_glue(true)?;
    editor.exit_manual_mesh();
    let plan = editor.document().unwrap().plan_scene_object_creation(
        SceneObjectType::Mesh,
        "Third mesh".into(),
        &[],
    )?;
    let third = editor
        .execute_scene_object_creation(editor.active_id().unwrap(), &plan)?
        .suggested_selection[0]
        .clone();
    let glue = ObjectRef::new(ObjectKind::Glue, fixture::id(5));
    let mut three_meshes = objects();
    three_meshes.push(third);
    let history = editor.document().unwrap().history_lengths();
    for selection in [
        vec![],
        vec![objects()[0].clone()],
        three_meshes,
        vec![objects()[0].clone(), glue.clone()],
        vec![glue],
    ] {
        editor.execute(EditorCommand::SelectObjects(selection.clone()))?;
        for source in [
            CommandSource::Direct,
            CommandSource::Toolbar,
            CommandSource::NativeMenu,
            CommandSource::Shortcut,
        ] {
            let command = editor.command(CommandId::GlueStitch).with_source(source);
            let state = editor.command_state(command);
            assert!(!state.enabled);
            assert_eq!(
                state.disabled_reason,
                Some(CommandDisabledReason::RequiresTwoMeshes)
            );
            assert!(editor.dispatch(command).is_err());
            assert!(!editor.manual_active());
            assert_eq!(editor.document().unwrap().context().selections, selection);
        }
    }
    editor.execute(EditorCommand::SelectObjects(objects()))?;
    assert!(editor.command_available(editor.command(CommandId::GlueStitch)));
    let command = editor.command(CommandId::GlueStitch);
    editor.execute(EditorCommand::SelectObjects(vec![objects()[0].clone()]))?;
    assert!(
        editor.dispatch(command).is_err(),
        "dispatch rechecks the selection"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    Ok(())
}

#[test]
fn glue_mesh_edit_prepares_binds_reopens_and_unbinds_connections() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup_meshes(false, true)?;
    let original = meshes(&editor);
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    click(&mut editor, 60., 65., false)?;
    let inserted = meshes(&editor);
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let frame = support::present_canvas(&mut editor)?;
    assert!(
        !editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .add_glue_point
    );
    assert_eq!(selected(&editor), 1);
    assert_eq!(
        glues(&editor).len(),
        1,
        "adding a Glue point binds immediately"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    for (before, after) in original.iter().zip(&inserted) {
        assert_eq!(after.vertex_ids.len(), before.vertex_ids.len() + 1);
    }
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueBind))?;
    let created = glues(&editor)[0].clone();
    assert_eq!(created.pairs.len(), 1);
    assert!(editor.manual_active());
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueWeights))?;
    assert!(!editor.manual_active());
    assert_eq!(editor.active_tool(), ToolId::Glue);
    assert_eq!(
        editor
            .document()
            .unwrap()
            .context()
            .selection
            .as_ref()
            .unwrap()
            .id,
        created.id
    );
    let history = editor.document().unwrap().history_lengths();
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueEditStitches))?;
    assert!(editor.manual_active());
    let frame = support::present_canvas(&mut editor)?;
    assert_eq!(
        editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .target,
        Some(created.id.clone())
    );
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    click(&mut editor, 60., 65., false)?;
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueUnbind))?;
    assert!(glues(&editor)[0].pairs.is_empty());
    assert_eq!(meshes(&editor), inserted);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(glues(&editor), vec![created]);
    Ok(())
}

#[test]
fn glue_separated_selected_endpoints_meet_at_midpoint_and_undo_restores_them() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let original = meshes(&editor);
    click(&mut editor, 20., 20., false)?;
    click(&mut editor, 140., 140., true)?;
    assert_eq!(selected(&editor), 2);
    let frame = support::present_canvas(&mut editor)?;
    assert_eq!(
        editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .bind_count,
        1
    );
    editor.dispatch(editor.command(CommandId::GlueBind))?;
    let glue = glues(&editor)[0].clone();
    assert_eq!(glue.pairs.len(), 1);
    assert_eq!([glue.pairs[0].weight_a, glue.pairs[0].weight_b], [0.5, 0.5]);
    assert_eq!(glue.intensity, 1.);
    assert_eq!(
        meshes(&editor),
        original,
        "binding must not bake positions or insert vertices"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.exit_manual_mesh();
    let frame = support::present_canvas(&mut editor)?;
    for (mesh, vertex) in [
        (&glue.mesh_a_id, glue.pairs[0].vertex_a),
        (&glue.mesh_b_id, glue.pairs[0].vertex_b),
    ] {
        let p = frame
            .geometry
            .mesh(mesh.as_str())
            .unwrap()
            .vertices
            .iter()
            .find(|(id, _)| Some(*id) == vertex)
            .unwrap()
            .1;
        assert!(
            (p.x - 80.).abs() < 1e-5 && (p.y - 80.).abs() < 1e-5,
            "{p:?}"
        );
    }
    editor.execute(EditorCommand::Undo)?;
    assert!(glues(&editor).is_empty());
    assert_eq!(meshes(&editor), original);
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(glues(&editor), vec![glue.clone()]);
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Glue,
        glue.id,
    )))?;
    editor.dispatch(editor.command(CommandId::GlueEditStitches))?;
    click(&mut editor, 80., 80., false)?;
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueUnbind))?;
    assert!(glues(&editor)[0].pairs.is_empty());
    assert_eq!(meshes(&editor), original);
    Ok(())
}

#[test]
fn glue_mesh_edit_separates_artwork_handles_and_drag_from_runtime_attachment() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let assert_edges = |editor: &Editor, frame: &CanvasFrame, joint: Option<ModelPoint>| {
        let overlay = editor.overlay();
        for style in &overlay.mesh_styles {
            let mesh = frame.geometry.mesh(&style.object.id).unwrap();
            let lines: Vec<_> = overlay.lines[style.lines.clone()]
                .iter()
                .chain(&overlay.automatic_lines[style.automatic_lines.clone()])
                .copied()
                .collect();
            assert_eq!(lines.len(), mesh.edges.len(), "all topology edges remain");
            let project = |slot: usize| {
                frame.transform.to_viewport(if slot == 0 {
                    joint.unwrap_or(mesh.vertices[slot].1)
                } else {
                    mesh.vertices[slot].1
                })
            };
            let close = |a: ViewportPoint, b: ViewportPoint| (a.x - b.x).hypot(a.y - b.y) < 1e-4;
            for &(a, b) in mesh.edges.iter() {
                let (a, b) = (project(a), project(b));
                assert!(
                    lines.iter().any(|&(p, q)| (close(a, p) && close(b, q))
                        || (close(a, q) && close(b, p))),
                    "edge {a:?}–{b:?} must follow the merged Glue point on {}",
                    style.object.id
                );
            }
        }
    };
    let initial = support::present_canvas(&mut editor)?;
    assert_edges(&editor, &initial, None);
    click(&mut editor, 20., 20., false)?;
    click(&mut editor, 140., 140., true)?;
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueBind))?;
    let original = meshes(&editor);
    let glue = glues(&editor)[0].clone();
    let point = |frame: &CanvasFrame, mesh: &str| frame.geometry.mesh(mesh).unwrap().vertices[0].1;
    let draw_point = |scene: &SceneSnapshot, runtime: bool, mesh: &str| {
        let frame = if runtime {
            &scene.runtime_frame
        } else {
            &scene.viewport_frame
        };
        let p = frame
            .drawables
            .iter()
            .find(|d| d.id == mesh)
            .unwrap()
            .positions[0];
        ModelPoint { x: p.x, y: p.y }
    };
    let a = objects()[0].id.clone();
    let b = objects()[1].id.clone();
    let edit = support::present_canvas(&mut editor)?;
    assert_eq!(point(&edit, &a), ModelPoint { x: 20., y: 20. });
    assert_eq!(point(&edit, &b), ModelPoint { x: 140., y: 140. });
    assert_eq!(
        draw_point(&edit.geometry.source, false, &a),
        point(&edit, &a)
    );
    assert_eq!(
        draw_point(&edit.geometry.source, false, &b),
        point(&edit, &b)
    );
    assert_eq!(
        draw_point(&edit.geometry.source, true, &a),
        ModelPoint { x: 80., y: 80. }
    );
    let overlay = editor.overlay();
    let relationship = overlay
        .glues
        .iter()
        .find(|g| g.points.iter().any(|p| p.state == GluePointState::Binding))
        .unwrap();
    assert!(
        relationship.pairs.is_empty(),
        "a Glue point has no extra relationship line"
    );
    assert_eq!(relationship.points.len(), 1);
    assert_eq!(
        overlay.glues.iter().map(|g| g.points.len()).sum::<usize>(),
        1
    );
    assert_eq!(
        overlay.vertices.len(),
        6,
        "both endpoint editing markers are replaced by one Glue point"
    );
    let middle = edit.transform.to_viewport(ModelPoint { x: 80., y: 80. });
    let marker = relationship.points[0].position;
    assert!((marker.x - middle.x).hypot(marker.y - middle.y) < 1e-4);
    assert_edges(&editor, &edit, Some(ModelPoint { x: 80., y: 80. }));
    // Edge picking follows the visible joint, with no ghost edge at its old position.
    let edge = edit.transform.to_viewport(ModelPoint { x: 90., y: 50. });
    assert_eq!(
        editor.manual_target_at(&edit, edge).unwrap().object,
        objects()[0]
    );
    editor.hover_canvas(edit.version.binding, Some(edge))?;
    assert_eq!(
        editor.overlay().manual_mesh_feedback,
        Some(ManualMeshFeedback::FixedEdge)
    );
    let old_edge = edit.transform.to_viewport(ModelPoint { x: 50., y: 20. });
    editor.hover_canvas(edit.version.binding, Some(old_edge))?;
    assert_eq!(
        editor.overlay().manual_mesh_feedback,
        Some(ManualMeshFeedback::Empty)
    );
    editor.hover_canvas(edit.version.binding, None)?;
    // Other viewports and retained frames must keep the final Glue pose.
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(
        other,
        ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        },
    );
    let other_preview = editor.preview_for(editor.viewport_binding(other).unwrap())?;
    let other_frame = other_preview.canvas.unwrap();
    assert_eq!(point(&other_frame, &a), ModelPoint { x: 80., y: 80. });
    assert_eq!(
        draw_point(&other_preview.scene, false, &a),
        point(&other_frame, &a)
    );
    click(&mut editor, 80., 80., false)?;
    assert_eq!(selected(&editor), 1);
    let frame = support::present_canvas(&mut editor)?;
    let start = frame.transform.to_viewport(ModelPoint { x: 80., y: 80. });
    let end = frame.transform.to_viewport(ModelPoint { x: 84., y: 84. });
    let down = CanvasInput::Down {
        point: start,
        shift: false,
        alt: false,
        primary: false,
    };
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    let candidate = support::present_canvas(&mut editor)?;
    let moved = point(&candidate, &a);
    assert!((moved.x - 24.).abs() < 1e-4 && (moved.y - 24.).abs() < 1e-4);
    assert_eq!(point(&candidate, &b), ModelPoint { x: 144., y: 144. });
    assert_eq!(draw_point(&candidate.geometry.source, false, &a), moved);
    assert_edges(&editor, &candidate, Some(ModelPoint { x: 84., y: 84. }));
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let canceled = support::present_canvas(&mut editor)?;
    assert_edges(&editor, &canceled, Some(ModelPoint { x: 80., y: 80. }));
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    let changed = meshes(&editor);
    assert!((changed[1].base_positions[0].x - 144.).abs() < 1e-4);
    assert!((changed[0].base_positions[0].x - 24.).abs() < 1e-4);
    assert_eq!(glues(&editor), vec![glue.clone()]);
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(meshes(&editor), original);
    let undone = support::present_canvas(&mut editor)?;
    assert_edges(&editor, &undone, Some(ModelPoint { x: 80., y: 80. }));
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(meshes(&editor), changed);
    let redone = support::present_canvas(&mut editor)?;
    assert_edges(&editor, &redone, Some(ModelPoint { x: 84., y: 84. }));
    editor.dispatch(editor.command(CommandId::GlueWeights))?;
    let final_frame = support::present_canvas(&mut editor)?;
    for mesh in [&a, &b] {
        let p = point(&final_frame, mesh);
        assert!(
            (p.x - 84.).abs() < 1e-4 && (p.y - 84.).abs() < 1e-4,
            "{p:?}"
        );
    }
    assert_eq!(point(&edit, &a), ModelPoint { x: 20., y: 20. });
    assert_eq!(point(&other_frame, &a), ModelPoint { x: 80., y: 80. });
    editor.dispatch(editor.command(CommandId::GlueEditStitches))?;
    let reopened = support::present_canvas(&mut editor)?;
    assert_eq!(point(&reopened, &b), ModelPoint { x: 144., y: 144. });
    assert!((point(&reopened, &a).x - 24.).abs() < 1e-4);
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    assert_edges(&editor, &reopened, Some(ModelPoint { x: 84., y: 84. }));
    click(&mut editor, 84., 84., false)?;
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueUnbind))?;
    let unbound = support::present_canvas(&mut editor)?;
    assert_edges(&editor, &unbound, None);
    editor.execute(EditorCommand::Undo)?;
    let rebound = support::present_canvas(&mut editor)?;
    assert_edges(&editor, &rebound, Some(ModelPoint { x: 84., y: 84. }));
    Ok(())
}

#[test]
fn glue_midpoint_control_selects_moves_cancels_and_unbinds_its_endpoint_pair() -> anyhow::Result<()>
{
    let (mut editor, dir) = setup()?;
    click(&mut editor, 20., 20., false)?;
    click(&mut editor, 140., 140., true)?;
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueBind))?;
    let original = meshes(&editor);
    let glue = glues(&editor)[0].clone();
    let middle = |editor: &Editor| {
        editor
            .overlay()
            .glues
            .into_iter()
            .flat_map(|g| g.points)
            .find(|p| p.state == GluePointState::Binding)
            .unwrap()
    };
    support::present_canvas(&mut editor)?;
    assert!(
        middle(&editor).selected,
        "binding produces a selected control immediately"
    );
    let start = middle(&editor).position;
    editor.hover_canvas(editor.primary_viewport_binding().unwrap(), Some(start))?;
    assert!(middle(&editor).hovered);
    assert_eq!(
        editor.overlay().manual_mesh_feedback,
        Some(ManualMeshFeedback::Vertex)
    );
    // Remove both endpoints via the center, then reselect it without adding mesh points.
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: true,
            primary: false,
        },
    )?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: start })?;
    assert_eq!(selected(&editor), 0);
    assert!(!middle(&editor).selected);
    // The old endpoint position is no longer a hidden selectable vertex.
    let old = support::present_canvas(&mut editor)?
        .transform
        .to_viewport(ModelPoint { x: 20., y: 20. });
    editor.hover_canvas(editor.primary_viewport_binding().unwrap(), Some(old))?;
    assert_ne!(
        editor.overlay().manual_mesh_feedback,
        Some(ManualMeshFeedback::Vertex)
    );
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: ViewportPoint {
                x: old.x - 8.,
                y: old.y - 8.,
            },
            shift: true,
            alt: false,
            primary: false,
        },
    )?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Up {
            point: ViewportPoint {
                x: old.x + 8.,
                y: old.y + 8.,
            },
        },
    )?;
    assert_eq!(selected(&editor), 0);
    assert_eq!(meshes(&editor), original);
    // The midpoint can be box-selected even when neither endpoint is in the box.
    let corner = ViewportPoint {
        x: start.x - 8.,
        y: start.y - 8.,
    };
    let opposite = ViewportPoint {
        x: start.x + 8.,
        y: start.y + 8.,
    };
    for (alt, count) in [(false, 1), (true, 0)] {
        support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: corner,
                shift: !alt,
                alt,
                primary: false,
            },
        )?;
        support::pointer_input(&mut editor, CanvasInput::Up { point: opposite })?;
        assert_eq!(selected(&editor), count);
    }
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    let down = CanvasInput::Down {
        point: start,
        shift: false,
        alt: false,
        primary: false,
    };
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: start })?;
    assert_eq!(selected(&editor), 1);
    assert!(middle(&editor).selected);
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let frame = support::present_canvas(&mut editor)?;
    assert!(
        editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .add_glue_point,
        "selecting a binding control preserves the add-point tool for continued insertion"
    );
    let end = frame.transform.to_viewport(ModelPoint { x: 84., y: 84. });
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    let preview = support::present_canvas(&mut editor)?;
    for (object, expected) in objects().iter().zip([24., 144.]) {
        let p = preview.geometry.mesh(&object.id).unwrap().vertices[0].1;
        assert!((p.x - expected).abs() < 1e-4 && (p.y - expected).abs() < 1e-4);
    }
    assert!((middle(&editor).position.x - end.x).hypot(middle(&editor).position.y - end.y) < 1e-4);
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    support::present_canvas(&mut editor)?;
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    let changed = meshes(&editor);
    for (mesh, expected) in changed.iter().zip([24., 144.]) {
        assert!((mesh.base_positions[0].x - expected).abs() < 1e-4);
        assert_eq!(
            mesh.vertex_ids.len(),
            4,
            "a binding control is not a third topology vertex"
        );
    }
    assert_eq!(glues(&editor), vec![glue.clone()]);
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(meshes(&editor), original);
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(meshes(&editor), changed);
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueUnbind))?;
    support::present_canvas(&mut editor)?;
    assert!(
        editor
            .overlay()
            .glues
            .iter()
            .flat_map(|g| &g.points)
            .all(|p| p.state != GluePointState::Binding)
    );
    assert_eq!(
        editor.overlay().vertices.len(),
        8,
        "unbinding restores both ordinary vertices"
    );
    assert_eq!(meshes(&editor), changed);
    editor.execute(EditorCommand::Undo)?;
    support::present_canvas(&mut editor)?;
    assert_eq!(glues(&editor), vec![glue]);
    assert!(middle(&editor).selected);
    let path = dir.path().join("midpoint.zipzi");
    let saved = editor.prepare_save(editor.active_id().unwrap(), &path, false)?;
    editor.finish_save(&mut saved.run())?;
    let mut document = Document::open(&path)?;
    document.select_objects(objects())?;
    let mut reopened = support::editor_from_document(document);
    support::attach_viewports(&mut reopened);
    reopened.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    reopened.enter_manual_mesh(reopened.active_id().unwrap())?;
    let frame = support::present_canvas(&mut reopened)?;
    let expected = frame.transform.to_viewport(ModelPoint { x: 84., y: 84. });
    let restored = middle(&reopened).position;
    assert!((restored.x - expected.x).hypot(restored.y - expected.y) < 1e-4);
    Ok(())
}

#[test]
fn glue_locked_midpoint_refuses_drag_without_inserting_a_vertex() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    click(&mut editor, 20., 20., false)?;
    click(&mut editor, 140., 140., true)?;
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueBind))?;
    let id = glues(&editor)[0].id.clone();
    let original = meshes(&editor);
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![ObjectRef::new(ObjectKind::Glue, id)],
        locked: true,
    })?;
    editor.execute(EditorCommand::SelectObjects(objects()))?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let history = editor.document().unwrap().history_lengths();
    let frame = support::present_canvas(&mut editor)?;
    let start = frame.transform.to_viewport(ModelPoint { x: 80., y: 80. });
    let end = frame.transform.to_viewport(ModelPoint { x: 84., y: 84. });
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert!(editor.canvas_notice().is_some());
    Ok(())
}

#[test]
fn glue_add_point_creates_binding_atomically_and_keeps_adding_to_the_same_seam()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup_meshes(false, true)?;
    let original = meshes(&editor);
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    let start = screen(&mut editor, 60., 65.)?;
    let down = CanvasInput::Down {
        point: start,
        shift: false,
        alt: false,
        primary: false,
    };
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(meshes(&editor), original);
    assert!(glues(&editor).is_empty());
    support::present_canvas(&mut editor)?;
    support::pointer_input(&mut editor, down)?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Up {
            point: ViewportPoint {
                x: start.x + 40.,
                ..start
            },
        },
    )?;
    assert_eq!(
        meshes(&editor),
        original,
        "dragging empty space in the add tool must not edit meshes"
    );
    assert!(glues(&editor).is_empty());
    click(&mut editor, 60., 65., false)?;
    let first = glues(&editor)[0].clone();
    let inserted = meshes(&editor);
    assert_eq!(first.pairs.len(), 1);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    support::present_canvas(&mut editor)?;
    assert!(
        editor
            .overlay()
            .glues
            .iter()
            .flat_map(|g| &g.points)
            .any(|p| p.state == GluePointState::Binding)
    );
    click(&mut editor, 60., 65., false)?;
    assert_eq!(glues(&editor), vec![first.clone()]);
    assert_eq!(meshes(&editor), inserted);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    click(&mut editor, 70., 75., false)?;
    let two = glues(&editor)[0].clone();
    assert_eq!(glues(&editor).len(), 1);
    assert_eq!(two.id, first.id);
    assert_eq!(two.pairs.len(), 2);
    for (mesh, before) in meshes(&editor).iter().zip(&original) {
        assert_eq!(mesh.vertex_ids.len(), before.vertex_ids.len() + 2);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    let frame = support::present_canvas(&mut editor)?;
    assert!(
        editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .add_glue_point
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(glues(&editor), vec![first.clone()]);
    assert_eq!(meshes(&editor), inserted);
    editor.execute(EditorCommand::Undo)?;
    assert!(glues(&editor).is_empty());
    assert_eq!(
        meshes(&editor),
        original,
        "one undo removes both new vertices and their Glue"
    );
    editor.execute(EditorCommand::Redo)?;
    let frame = support::present_canvas(&mut editor)?;
    assert_eq!(
        editor
            .manual_glue_controls(frame.version.binding)
            .unwrap()
            .target,
        Some(first.id.clone())
    );
    assert_eq!(meshes(&editor), inserted);
    click(&mut editor, 70., 75., false)?;
    let points = meshes(&editor);
    assert_eq!(glues(&editor)[0].id, first.id);
    support::present_canvas(&mut editor)?;
    editor.dispatch(editor.command(CommandId::GlueUnbind))?;
    assert_eq!(glues(&editor)[0].pairs.len(), 1);
    click(&mut editor, 70., 75., false)?;
    assert_eq!(
        meshes(&editor),
        points,
        "reconnecting reuses both existing endpoints"
    );
    assert_eq!(glues(&editor)[0].pairs.len(), 2);
    assert_eq!(glues(&editor)[0].id, first.id);
    Ok(())
}

#[test]
fn glue_add_point_rolls_back_inserted_vertex_if_the_other_endpoint_is_occupied()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup_meshes(true, true)?;
    let original = meshes(&editor);
    let relationships = glues(&editor);
    editor.dispatch(editor.command(CommandId::GlueInsertPoint))?;
    // B has a bound vertex at (40,40); A would need a new vertex there.
    click(&mut editor, 40., 40., false)?;
    assert_eq!(meshes(&editor), original);
    assert_eq!(glues(&editor), relationships);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(editor.canvas_notice().is_some());
    Ok(())
}
