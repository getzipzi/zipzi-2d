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
    let dir = tempfile::tempdir()?;
    let mut session = fixture::create(
        dir.path(),
        include_bytes!("../fixtures/alpha-asymmetric.png"),
    )?;
    if glue {
        session.edit("Glue", None, |edit| {
            edit.create_glue(zipzi_core::Glue {
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
            })
        })?;
    }
    let saved = session.save_project(&dir.path().join("multi.zipzi"), None)?;
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
    let runtime = host.read();
    let model = runtime.document(document.runtime_session_id()).unwrap();
    objects()
        .iter()
        .map(|o| model.get_mesh(&o.id).unwrap().clone())
        .collect()
}
fn screen(editor: &mut Editor, x: f32, y: f32) -> anyhow::Result<ViewportPoint> {
    Ok(support::present_canvas(editor)?
        .transform
        .to_viewport(ModelPoint { x, y }))
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
    editor
        .overlay_for_viewport(editor.primary_viewport_binding().unwrap())
        .vertices
        .iter()
        .filter(|v| v.selected)
        .count()
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
    for (index, p) in points.iter().enumerate() {
        click(&mut editor, p.x, p.y, index > 0)?;
    }
    assert_eq!(selected(&editor), 2);
    assert!(
        editor
            .dispatch(editor.command(CommandId::ManualRemoveSelection))
            .is_err()
    );
    assert_eq!(meshes(&editor), original);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    let frame = support::present_canvas(&mut editor)?;
    let start = frame.transform.to_viewport(points[1]);
    let end = frame.transform.to_viewport(ModelPoint {
        x: points[1].x + 4.,
        y: points[1].y + 3.,
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
