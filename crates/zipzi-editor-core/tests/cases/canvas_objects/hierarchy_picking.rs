use super::super::support::present_canvas as shown;
use super::common::*;
use zipzi_editor_core::*;

#[test]
fn deformer_lines_are_pickable_but_empty_interior_is_not_and_groups_are_candidates()
-> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, part) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let f = shown(&mut editor)?;
    let p = |x, y| f.transform.to_viewport(ModelPoint { x, y });
    let scene = &f.geometry;
    let selection = &editor.document().unwrap().context().selections;
    assert_eq!(
        scene.hit(
            p(32., 90.),
            f.transform,
            selection,
            ToolId::Select,
            DeformerVisibility::default()
        ),
        Some(HitTarget::Object(ObjectRef::new(
            ObjectKind::Deformer,
            &warp
        )))
    );
    assert_eq!(
        scene.hit(
            p(32., 90.),
            f.transform,
            selection,
            ToolId::Select,
            DeformerVisibility {
                ancestors: true,
                ..Default::default()
            }
        ),
        Some(HitTarget::Object(ObjectRef::new(
            ObjectKind::Deformer,
            &warp
        )))
    );
    assert!(
        scene
            .hit(
                p(40., 40.),
                f.transform,
                selection,
                ToolId::Select,
                DeformerVisibility::default()
            )
            .is_none()
    );
    assert!(matches!(
        scene.hit(
            p(90., 90.),
            f.transform,
            selection,
            ToolId::Select,
            DeformerVisibility::default()
        ),
        Some(HitTarget::Object(ObjectRef {
            kind: ObjectKind::Mesh,
            ..
        }))
    ));
    let candidates = editor.pick_candidates(p(90., 90.), Some(f.version));
    assert!(
        candidates
            .iter()
            .any(|c| c.object == ObjectRef::new(ObjectKind::Part, &part))
    );
    assert!(
        candidates
            .iter()
            .any(|c| c.object == ObjectRef::new(ObjectKind::Deformer, &warp))
    );
    assert!(!candidates.iter().any(|c| c.object.id == rotation));
    editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
    let rotation_control = scene
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let rotation_points = scene.control_points(rotation_control, f.transform);
    let rotation_body = ViewportPoint {
        x: (rotation_points[0].x + rotation_points[1].x) / 2.,
        y: (rotation_points[0].y + rotation_points[1].y) / 2.,
    };
    for (point, name) in [
        (p(32., 90.), "Warp"),
        (p(90., 90.), "Mesh"),
        (rotation_body, "Rotation"),
    ] {
        editor.hover_canvas(f.version.binding, Some(point))?;
        assert_eq!(editor.overlay().hovered_object_label.as_deref(), Some(name));
    }
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Part,
        part,
    )))?;
    assert!(!editor.overlay().accent_lines.is_empty());
    assert!(editor.overlay().vertices.is_empty());
    let handle = editor.overlay().handles[0].position;
    editor.hover_canvas(f.version.binding, Some(handle))?;
    assert_eq!(
        editor.overlay().hovered_object_label.as_deref(),
        Some("Part")
    );
    editor.hover_canvas(f.version.binding, None)?;
    assert!(editor.overlay().hovered_object_label.is_none());

    let mut scene = (*f.geometry).clone();
    let glue = ObjectRef::new(ObjectKind::Glue, "readonly-connection");
    scene.controls.push(ControlGeometry {
        object: glue.clone(),
        kind: ControlKind::Glue,
        rotation_axis: None,
        rotation_bone_length: 128.,
        rotation_bone_reference_scale: 1.,
        points: vec![ModelPoint { x: 10., y: 10. }, ModelPoint { x: 20., y: 10. }],
        edges: vec![(0, 1)].into(),
        enabled: true,
        visible: true,
    });
    assert_eq!(
        scene.hit(
            p(10., 10.),
            f.transform,
            std::slice::from_ref(&glue),
            ToolId::Select,
            DeformerVisibility::default(),
        ),
        Some(HitTarget::Object(glue))
    );
    Ok(())
}
#[test]
fn mesh_selection_and_tool_changes_do_not_reveal_deformer_helpers() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = shown(&mut editor)?;
    for tool in [ToolId::Select, ToolId::Glue, ToolId::Hand] {
        editor.execute(EditorCommand::SetTool(tool))?;
        let overlay = editor.overlay();
        assert_eq!(
            overlay.lines.len(),
            5,
            "only the mesh's own edges should be shown"
        );
        assert_eq!(overlay.vertices.len(), 4);
        assert_eq!(
            overlay.accent_lines.len(),
            4,
            "only the selected mesh outline"
        );
        let empty = frame.transform.to_viewport(ModelPoint { x: 32., y: 90. });
        let hits = editor.pick_candidates(empty, Some(frame.version));
        assert_eq!(
            hits.iter().any(|c| c.object.id == warp),
            tool != ToolId::Hand
        );
        assert!(
            overlay
                .deformers
                .iter()
                .filter(|d| d.kind == ControlKind::Warp)
                .all(|d| d.outline_only)
        );
    }
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        warp,
    )))?;
    for tool in [ToolId::Select, ToolId::Glue, ToolId::Hand] {
        editor.execute(EditorCommand::SetTool(tool))?;
        let overlay = editor.overlay();
        assert!(overlay.lines.is_empty(), "unselected deformers stay hidden");
        assert_eq!(
            overlay.bezier_lines.len(),
            64,
            "the selected warp Bezier curves remain visible"
        );
        assert_eq!(overlay.vertices.len(), 8);
    }
    editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
    let overlay = editor.overlay();
    assert!(overlay.lines.is_empty(), "no persistent canvas border");
    assert!(overlay.accent_lines.is_empty());
    assert!(overlay.vertices.is_empty());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn deformer_detail_is_opt_in_per_document_while_outlines_remain_pickable() -> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, _) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = shown(&mut editor)?;
    let original = editor.active_id().unwrap();
    let evaluations = editor.evaluation_count();
    let visible = |editor: &Editor| {
        frame
            .geometry
            .controls
            .iter()
            .filter(|c| {
                frame.geometry.control_visible(
                    c,
                    &editor.document().unwrap().context().selections,
                    editor.active_tool(),
                    editor.deformer_visibility(),
                )
            })
            .map(|c| c.object.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(visible(&editor), [warp.clone(), rotation.clone()]);
    editor.dispatch(editor.command(CommandId::ToggleAncestorDeformers))?;
    assert_eq!(visible(&editor), [warp.clone(), rotation.clone()]);
    let point = frame.transform.to_viewport(ModelPoint { x: 32., y: 90. });
    assert!(
        editor
            .pick_candidates(point, Some(frame.version))
            .iter()
            .any(|c| c.object.id == warp)
    );
    editor.dispatch(editor.command(CommandId::ToggleDeformers))?;
    let all = visible(&editor);
    assert_eq!(all.len(), 2);
    assert!(all.contains(&warp) && all.contains(&rotation));
    editor.dispatch(editor.command(CommandId::ToggleDeformers))?;
    assert_eq!(visible(&editor), [warp.clone(), rotation.clone()]);
    editor.dispatch(editor.command(CommandId::ToggleAncestorDeformers))?;
    assert_eq!(visible(&editor), [warp.clone(), rotation]);
    assert!(
        editor
            .pick_candidates(point, Some(frame.version))
            .iter()
            .any(|c| c.object.id == warp)
    );
    assert!(
        editor
            .overlay()
            .deformers
            .iter()
            .filter(|d| d.kind == ControlKind::Warp)
            .all(|d| d.outline_only)
    );
    assert_eq!(
        editor.evaluation_count(),
        evaluations,
        "display toggles do not re-evaluate the model"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.dispatch(editor.command(CommandId::ToggleAncestorDeformers))?;
    editor.new_document()?;
    assert_eq!(editor.deformer_visibility(), DeformerVisibility::default());
    editor.activate(original)?;
    assert_eq!(
        editor.deformer_visibility(),
        DeformerVisibility {
            hide_unselected_warps: false,
            hide_unselected_rotations: false,
            ancestors: true,
            all: false
        }
    );
    Ok(())
}

#[test]
fn root_warp_and_parent_local_mesh_controls_write_correct_spaces() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = crate::support::editor_from_document(doc);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    shown(&mut editor)?;
    let p = editor.overlay().vertices[0].position;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: p,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Up {
            point: ViewportPoint {
                x: p.x + 19.2,
                y: p.y,
            },
        },
    )?; // 19.2px = 0.1 parent units
    let local = editor.document().unwrap().selected_vertex_positions()[0].1;
    assert!((local.x - 0.2).abs() < 1e-4, "{local:?}");
    editor.execute(EditorCommand::Undo)?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        warp,
    )))?;
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    let p = editor.overlay().vertices[0].position;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: p,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Up {
            point: ViewportPoint {
                x: p.x + 10.,
                y: p.y,
            },
        },
    )?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let f = shown(&mut editor)?;
    assert!((f.geometry.controls[0].points[0].x - 42.).abs() < 1e-3);
    editor.execute(EditorCommand::Undo)?;
    editor.execute(EditorCommand::SelectObject(mesh))?;
    assert!(
        (editor.document().unwrap().selected_vertex_positions()[0]
            .1
            .x
            - 0.1)
            .abs()
            < 1e-5
    );
    Ok(())
}
#[test]
fn rotation_handle_edits_angle_and_escape_restores_pose() -> anyhow::Result<()> {
    let (doc, _dir, _, rotation, _) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    shown(&mut editor)?;
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        &rotation,
    )))?;
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    let frame = editor.preview()?.canvas.unwrap();
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let points = frame.geometry.control_points(c, frame.transform);
    let center = points[0];
    let start = rotation_body_pointer(center, points[1]);
    let end = ViewportPoint {
        x: center.x,
        y: center.y - (start.x - center.x).hypot(start.y - center.y),
    };
    assert!(editor.present_frame(frame));
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    assert_eq!(editor.overlay().active_handle, Some(DeformerHandle::Rotate));
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    assert!(editor.document().unwrap().has_pending_edit());
    editor.dispatch(editor.command(CommandId::CancelGesture))?;
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    shown(&mut editor)?;
    assert!(
        editor.overlay().vertices.iter().all(|v| !v.selected),
        "cancel clears the active rotation tip"
    );
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    let index = editor.document().unwrap().object_index();
    let properties = &index
        .get(&ObjectRef::new(ObjectKind::Deformer, rotation))
        .unwrap()
        .properties;
    assert!((properties["rotation"]["pose"]["angle"].as_f64().unwrap() - 90.).abs() < 0.001);
    Ok(())
}
