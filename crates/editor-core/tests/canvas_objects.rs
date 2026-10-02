use editor_core::*;
use kasane_core::{
    Vec2,
    evaluation::{OffscreenFrame, RenderCommand},
};
use std::sync::Arc;
#[allow(dead_code)]
mod support;
fn shown(editor: &mut Editor) -> anyhow::Result<CanvasFrame> {
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    Ok(frame)
}
fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (doc, dir) = support::sample_document_with_two_layers()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    shown(&mut editor)?;
    Ok((editor, dir))
}
fn click(editor: &mut Editor, p: CanvasPoint, shift: bool) -> anyhow::Result<()> {
    editor.canvas_input(CanvasInput::Down { point: p, shift })?;
    editor.canvas_input(CanvasInput::Up { point: p })?;
    Ok(())
}
fn texture(id: &str, alpha: [u8; 2]) -> SceneTexture {
    SceneTexture {
        asset_id: id.into(),
        sha256: id.into(),
        width: 2,
        height: 1,
        rgba: vec![255, 255, 255, alpha[0], 255, 255, 255, alpha[1]].into(),
    }
}
fn pixel_scene(doc: &mut Document) -> anyhow::Result<SceneSnapshot> {
    let mut source = doc.snapshot()?;
    let f = Arc::make_mut(&mut source.frame);
    for d in &mut f.drawables {
        d.texture_asset_id = "solid".into();
    }
    source.textures = vec![texture("solid", [255, 255]), texture("cutout", [0, 255])].into();
    Ok(source)
}
#[test]
fn object_click_selects_top_surface_without_editing_and_clears_on_empty() -> anyhow::Result<()> {
    let (mut editor, _) = setup()?;
    let before = editor.evaluation_count();
    let top = editor
        .layers()
        .into_iter()
        .find(|l| l.name == "Second")
        .unwrap()
        .id;
    assert_eq!(editor.active_tool(), ToolId::Select);
    assert!(editor.overlay().vertices.is_empty());
    click(&mut editor, CanvasPoint { x: 256., y: 256. }, false)?;
    assert_eq!(editor.document().unwrap().selected_id(), Some(top.as_str()));
    assert_eq!(
        editor
            .document()
            .unwrap()
            .context()
            .inspected
            .as_ref()
            .unwrap()
            .id,
        top
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().has_pending_edit());
    assert!(editor.preview_if_needed()?.is_none());
    assert_eq!(editor.evaluation_count(), before);
    click(&mut editor, CanvasPoint { x: 2., y: 2. }, false)?;
    assert!(editor.document().unwrap().context().selections.is_empty());
    Ok(())
}
#[test]
fn alpha_holes_pass_through_and_mask_inversion_changes_candidates() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_two_layers()?;
    let mut source = pixel_scene(&mut doc)?;
    let bottom = source.frame.drawables[0].id.clone();
    let top = source.frame.drawables[1].id.clone();
    Arc::make_mut(&mut source.frame).drawables[1].texture_asset_id = "cutout".into();
    let scene = CanvasScene::new(&doc, source.clone());
    assert_eq!(
        scene.mesh_hits(ModelPoint { x: 70., y: 128. }),
        [bottom.clone()]
    );
    assert_eq!(
        scene.mesh_hits(ModelPoint { x: 190., y: 128. }),
        [top.clone(), bottom.clone()]
    );
    let f = Arc::make_mut(&mut source.frame);
    f.drawables[0].texture_asset_id = "cutout".into();
    f.drawables[1].texture_asset_id = "solid".into();
    f.drawables[1].masks = vec![bottom.clone()];
    assert!(
        CanvasScene::new(&doc, source.clone())
            .mesh_hits(ModelPoint { x: 70., y: 128. })
            .is_empty()
    );
    Arc::make_mut(&mut source.frame).drawables[1].inverted_mask = true;
    let scene = CanvasScene::new(&doc, source);
    assert_eq!(scene.mesh_hits(ModelPoint { x: 70., y: 128. }), [top]);
    assert_eq!(scene.mesh_hits(ModelPoint { x: 190., y: 128. }), [bottom]);
    Ok(())
}
#[test]
fn offscreen_plan_overrides_flat_rank_and_applies_group_alpha() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_two_layers()?;
    let mut source = pixel_scene(&mut doc)?;
    let bottom = source.frame.drawables[0].id.clone();
    let top = source.frame.drawables[1].id.clone();
    let f = Arc::make_mut(&mut source.frame);
    f.drawables[0].render_order = 999;
    f.drawables[1].render_order = 0;
    f.offscreens = vec![OffscreenFrame {
        id: "group".into(),
        enabled: true,
        opacity: 1.,
        ..Default::default()
    }];
    f.render_plan = vec![
        RenderCommand::DrawMesh {
            mesh_id: bottom.clone(),
        },
        RenderCommand::BeginOffscreen {
            offscreen_id: "group".into(),
        },
        RenderCommand::DrawMesh {
            mesh_id: top.clone(),
        },
        RenderCommand::EndOffscreen {
            offscreen_id: "group".into(),
        },
    ];
    let point = ModelPoint { x: 128., y: 128. };
    assert_eq!(
        CanvasScene::new(&doc, source.clone()).mesh_hits(point),
        [top, bottom.clone()]
    );
    Arc::make_mut(&mut source.frame).offscreens[0].opacity = 0.;
    assert_eq!(CanvasScene::new(&doc, source).mesh_hits(point), [bottom]);
    Ok(())
}
#[test]
fn shift_selection_and_marquee_share_document_selection_without_history() -> anyhow::Result<()> {
    let (mut editor, _) = setup()?;
    let first = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    click(&mut editor, CanvasPoint { x: 256., y: 256. }, true)?;
    assert_eq!(editor.document().unwrap().context().selections.len(), 2);
    click(&mut editor, CanvasPoint { x: 256., y: 256. }, true)?;
    assert_eq!(editor.document().unwrap().context().selections, [first]);
    let a = CanvasPoint { x: 5., y: 5. };
    let b = CanvasPoint { x: 505., y: 505. };
    editor.canvas_input(CanvasInput::Down {
        point: a,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: b,
        left_down: true,
    })?;
    assert!(editor.overlay().marquee.is_some());
    editor.canvas_input(CanvasInput::Up { point: b })?;
    assert_eq!(editor.document().unwrap().context().selections.len(), 2);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(editor.overlay().marquee.is_none());
    Ok(())
}
#[test]
fn tool_state_is_per_document_and_vertex_click_opens_no_candidate() -> anyhow::Result<()> {
    let (mut editor, _) = setup()?;
    let first = editor.active_id().unwrap();
    editor.dispatch(editor.command(CommandId::Vertices))?;
    let p = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    assert!(!editor.document().unwrap().has_pending_edit());
    editor.canvas_input(CanvasInput::Move {
        point: CanvasPoint {
            x: p.x + 1.,
            y: p.y,
        },
        left_down: true,
    })?;
    assert!(!editor.document().unwrap().has_pending_edit());
    editor.new_document()?;
    assert_eq!(editor.active_tool(), ToolId::Select);
    editor.activate(first)?;
    assert_eq!(editor.active_tool(), ToolId::Vertices);
    assert!(!editor.gesture_active());
    Ok(())
}
#[test]
fn bound_mesh_is_selectable_and_edits_only_a_stored_keyform() -> anyhow::Result<()> {
    let (doc, _dir) = support::sample_document_with_binding(true)?;
    let base = doc.selected_vertex_positions();
    let param = doc.parameters()[0].id.clone();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    shown(&mut editor)?;
    click(&mut editor, CanvasPoint { x: 256., y: 256. }, false)?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    let p = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: p.x + 12.,
            y: p.y,
        },
    })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        base,
        "base positions must remain untouched"
    );
    editor.execute(EditorCommand::Undo)?;
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: param,
        value: 0.5,
    })?;
    shown(&mut editor)?;
    assert!(
        editor
            .overlay()
            .unavailable
            .unwrap()
            .contains("stored keyform")
    );
    let p = editor.overlay().vertices[0].position;
    click(&mut editor, p, false)?; // read-only controls can still be selected
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    assert!(
        editor
            .canvas_input(CanvasInput::Up {
                point: CanvasPoint {
                    x: p.x + 20.,
                    y: p.y
                }
            })
            .is_err()
    );
    assert!(!editor.gesture_active());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    Ok(())
}
fn rich_document() -> anyhow::Result<(Document, tempfile::TempDir, String, String, String)> {
    rich_document_with_canvas(kasane_core::Canvas::new(256., 256., Vec2::new(0., 0.), 1.))
}
fn rich_document_with_canvas(
    canvas: kasane_core::Canvas,
) -> anyhow::Result<(Document, tempfile::TempDir, String, String, String)> {
    use kasane_core::{
        Mesh, Part, PreciseVec2, RotationPose, RotationTransform, Transform, TransformData,
        WarpTransform,
    };
    use kasane_sdk::{AuthoringSession, prepare_png_asset};
    let dir = tempfile::tempdir()?;
    let asset_path = dir.path().join("a.png");
    std::fs::write(&asset_path, include_bytes!("fixtures/accent.png"))?;
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    let mut s = AuthoringSession::new(&id(1), canvas)?;
    s.edit("fixture", None, |e| {
        e.create_asset(prepare_png_asset(&id(2), "a", &asset_path)?)?;
        e.create_part(Part {
            id: id(3),
            name: "Part".into(),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(4),
            name: "Warp".into(),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(32., 32.),
                    Vec2::new(224., 32.),
                    Vec2::new(32., 224.),
                    Vec2::new(224., 224.),
                ],
            }),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(5),
            name: "Rotation".into(),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 0.,
                pose: RotationPose {
                    origin: PreciseVec2::new(128., 128.),
                    ..Default::default()
                },
            }),
            ..Default::default()
        })?;
        e.create_mesh(Mesh {
            id: id(6),
            name: "Mesh".into(),
            texture_asset_id: id(2),
            part_id: id(3),
            deformer_id: id(4),
            vertex_ids: vec![10, 11, 12, 13],
            base_positions: vec![
                Vec2::new(0.1, 0.1),
                Vec2::new(0.9, 0.1),
                Vec2::new(0.9, 0.9),
                Vec2::new(0.1, 0.9),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[10, 11, 12], [10, 12, 13]],
            ..Default::default()
        })?;
        Ok(())
    })?;
    let receipt = s.save_project(&dir.path().join("fixture"), None)?;
    Ok((Document::open(&receipt.manifest)?, dir, id(4), id(5), id(3)))
}
#[test]
fn deformer_lines_are_pickable_but_empty_interior_is_not_and_groups_are_candidates()
-> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, part) = rich_document()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let f = shown(&mut editor)?;
    let p = |x, y| f.transform.to_canvas(ModelPoint { x, y });
    let scene = &f.geometry;
    let selection = &editor.document().unwrap().context().selections;
    assert!(
        scene
            .hit(
                p(32., 90.),
                f.transform,
                selection,
                ToolId::Select,
                DeformerVisibility::default()
            )
            .is_none()
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
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Part,
        part,
    )))?;
    assert!(!editor.overlay().accent_lines.is_empty());
    assert!(editor.overlay().vertices.is_empty());

    let mut scene = (*f.geometry).clone();
    let glue = ObjectRef::new(ObjectKind::Glue, "readonly-connection");
    scene.controls.push(ControlGeometry {
        object: glue.clone(),
        kind: ControlKind::Glue,
        points: vec![ModelPoint { x: 10., y: 10. }, ModelPoint { x: 20., y: 10. }],
        edges: vec![(0, 1)].into(),
        enabled: true,
    });
    assert_eq!(
        scene.hit(
            p(10., 10.),
            f.transform,
            std::slice::from_ref(&glue),
            ToolId::Deformers,
            DeformerVisibility::default(),
        ),
        Some(HitTarget::Object(glue))
    );
    Ok(())
}
#[test]
fn mesh_selection_and_tool_changes_do_not_reveal_deformer_helpers() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = shown(&mut editor)?;
    for tool in [ToolId::Select, ToolId::Vertices, ToolId::Deformers] {
        editor.execute(EditorCommand::SetTool(tool))?;
        let overlay = editor.overlay();
        assert_eq!(
            overlay.lines.len(),
            if tool == ToolId::Vertices { 5 } else { 0 },
            "only the mesh's own edges should be shown"
        );
        assert_eq!(
            overlay.vertices.len(),
            if tool == ToolId::Vertices { 4 } else { 0 }
        );
        assert_eq!(
            overlay.accent_lines.len(),
            4,
            "only the selected mesh outline"
        );
        let empty = frame.transform.to_canvas(ModelPoint { x: 32., y: 90. });
        assert!(
            editor
                .pick_candidates(empty, Some(frame.version))
                .is_empty(),
            "hidden ancestor lines must not intercept clicks"
        );
    }
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        warp,
    )))?;
    for tool in [ToolId::Select, ToolId::Deformers] {
        editor.execute(EditorCommand::SetTool(tool))?;
        let overlay = editor.overlay();
        assert!(overlay.lines.is_empty(), "unselected deformers stay hidden");
        assert_eq!(
            overlay.accent_lines.len(),
            4,
            "the selected warp remains visible"
        );
        assert_eq!(
            overlay.vertices.len(),
            if tool == ToolId::Deformers { 4 } else { 0 }
        );
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
fn deformer_helpers_are_opt_in_per_document_and_share_the_picking_filter() -> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, _) = rich_document()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
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
    assert!(visible(&editor).is_empty());
    editor.dispatch(editor.command(CommandId::ToggleAncestorDeformers))?;
    assert_eq!(visible(&editor), [warp.clone()]);
    let point = frame.transform.to_canvas(ModelPoint { x: 32., y: 90. });
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
    assert_eq!(visible(&editor), [warp]);
    editor.dispatch(editor.command(CommandId::ToggleAncestorDeformers))?;
    assert!(visible(&editor).is_empty());
    assert!(
        editor
            .pick_candidates(point, Some(frame.version))
            .is_empty()
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
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    shown(&mut editor)?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    let p = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: p.x + 38.4,
            y: p.y,
        },
    })?; // 19.2px = 0.1 parent units
    let local = editor.document().unwrap().selected_vertex_positions()[0].1;
    assert!((local.x - 0.2).abs() < 1e-4, "{local:?}");
    editor.execute(EditorCommand::Undo)?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        warp,
    )))?;
    editor.execute(EditorCommand::SetTool(ToolId::Deformers))?;
    let p = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: p.x + 20.,
            y: p.y,
        },
    })?;
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
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    shown(&mut editor)?;
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Deformer,
        &rotation,
    )))?;
    editor.execute(EditorCommand::SetTool(ToolId::Deformers))?;
    let frame = editor.preview()?.canvas.unwrap();
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let points = frame.geometry.control_points(c, frame.transform);
    let center = points[0];
    let start = points[1];
    let end = CanvasPoint {
        x: center.x,
        y: center.y - 54.,
    };
    assert!(editor.present_frame(frame));
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: end,
        left_down: true,
    })?;
    assert!(editor.document().unwrap().has_pending_edit());
    editor.dispatch(editor.command(CommandId::CancelGesture))?;
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    shown(&mut editor)?;
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up { point: end })?;
    let index = editor.document().unwrap().object_index();
    let properties = &index
        .get(&ObjectRef::new(ObjectKind::Deformer, rotation))
        .unwrap()
        .properties;
    assert!((properties["rotation"]["pose"]["angle"].as_f64().unwrap() - 90.).abs() < 0.001);
    Ok(())
}

#[test]
fn marquee_selected_vertices_across_meshes_move_in_one_transaction() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let selected = editor
        .layers()
        .iter()
        .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
        .collect();
    editor.execute(EditorCommand::SelectObjects(selected))?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    editor.canvas_input(CanvasInput::Down {
        point: CanvasPoint { x: 1., y: 1. },
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint { x: 510., y: 510. },
    })?;
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        8
    );
    let original = editor.preview()?.scene;
    let p = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: p.x + 20.,
            y: p.y + 10.,
        },
    })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let moved = editor.preview()?.scene;
    for (before, after) in original.frame.drawables.iter().zip(&moved.frame.drawables) {
        for (a, b) in before.positions.iter().zip(&after.positions) {
            assert!((b.x - a.x - 10.).abs() < 1e-4);
            assert!((b.y - a.y + 5.).abs() < 1e-4);
        }
    }
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.preview()?.scene.frame.drawables,
        original.frame.drawables
    );
    Ok(())
}

#[test]
fn hidden_meshes_have_no_invisible_component_hit_targets() -> anyhow::Result<()> {
    let (mut editor, _) = setup()?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    editor.execute(EditorCommand::SetVisible(false))?;
    let frame = shown(&mut editor)?;
    assert!(editor.overlay().vertices.is_empty());
    let scene = &frame.geometry;
    assert!(!matches!(
        scene.hit(
            CanvasPoint { x: 0., y: 0. },
            frame.transform,
            &editor.document().unwrap().context().selections,
            ToolId::Vertices,
            DeformerVisibility::default()
        ),
        Some(HitTarget::Component(_))
    ));
    Ok(())
}

#[test]
fn vertex_marquee_can_start_inside_selected_meshes_without_changing_objects() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let objects: Vec<_> = editor
        .layers()
        .iter()
        .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
        .collect();
    editor.execute(EditorCommand::SelectObjects(objects.clone()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    let frame = shown(&mut editor)?;
    let start = frame.transform.to_canvas(ModelPoint { x: 120., y: 120. });
    let end = frame.transform.to_canvas(ModelPoint { x: 240., y: 240. });
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: end,
        left_down: true,
    })?;
    assert!(
        editor.overlay().marquee.is_some(),
        "a mesh interior must allow component marquee"
    );
    editor.canvas_input(CanvasInput::Up { point: end })?;
    assert_eq!(editor.document().unwrap().context().selections, objects);
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        4
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn invisible_offscreen_members_have_no_handles_and_are_excluded_from_marquee() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let mut frame = editor.preview()?.canvas.unwrap();
    let mut source = frame.geometry.source.clone();
    let evaluated = Arc::make_mut(&mut source.frame);
    let hidden = evaluated.drawables[0].id.clone();
    let visible = evaluated.drawables[1].id.clone();
    evaluated.offscreens = vec![OffscreenFrame {
        id: "hidden-group".into(),
        enabled: true,
        opacity: 0.,
        ..Default::default()
    }];
    evaluated.render_plan = vec![
        RenderCommand::BeginOffscreen {
            offscreen_id: "hidden-group".into(),
        },
        RenderCommand::DrawMesh {
            mesh_id: hidden.clone(),
        },
        RenderCommand::EndOffscreen {
            offscreen_id: "hidden-group".into(),
        },
        RenderCommand::DrawMesh {
            mesh_id: visible.clone(),
        },
    ];
    frame.geometry = Arc::new(CanvasScene::new(editor.document().unwrap(), source));
    assert!(editor.present_frame(frame));
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Mesh,
        hidden,
    )))?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    assert!(
        editor.overlay().vertices.is_empty(),
        "zero-opacity groups must not leave ghost controls"
    );
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    editor.canvas_input(CanvasInput::Down {
        point: CanvasPoint { x: 1., y: 1. },
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint { x: 510., y: 510. },
    })?;
    assert_eq!(
        editor.document().unwrap().context().selections,
        [ObjectRef::new(ObjectKind::Mesh, visible)]
    );
    Ok(())
}

#[test]
fn warp_controls_align_with_mesh_after_canvas_y_reversal() -> anyhow::Result<()> {
    for flag in [0, 1] {
        let mut canvas = kasane_core::Canvas::new(320., 280., Vec2::new(110., 150.), 100.);
        canvas.flag = flag;
        let (doc, _dir, warp, _, _) = rich_document_with_canvas(canvas)?;
        let mesh = doc.context().selection.clone().unwrap();
        let mut editor = Editor::from_document(doc);
        editor.set_canvas_metrics(CanvasMetrics {
            width: 700.,
            height: 510.,
            scale_factor: 2.,
        });
        editor.zoom_by(0.5);
        editor.pan_by(CanvasPoint { x: 17., y: -23. });
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object.id == warp)
            .unwrap();
        assert!((control.points[0].x - 32.).abs() < 1e-3);
        let expected_y = if flag == 0 {
            2. * canvas.origin.y - 32.
        } else {
            32.
        };
        assert!((control.points[0].y - expected_y).abs() < 1e-3);
        // Vertices occupy known local (u,v) positions inside the root warp.
        for ((_, p), (u, v)) in frame.geometry.mesh(&mesh.id).unwrap().vertices.iter().zip([
            (0.1, 0.1),
            (0.9, 0.1),
            (0.9, 0.9),
            (0.1, 0.9),
        ]) {
            let weights = [(1. - u) * (1. - v), u * (1. - v), (1. - u) * v, u * v];
            let expected =
                control
                    .points
                    .iter()
                    .zip(weights)
                    .fold(ModelPoint::default(), |p, (c, w)| ModelPoint {
                        x: p.x + c.x * w,
                        y: p.y + c.y * w,
                    });
            let a = frame.transform.to_canvas(*p);
            let b = frame.transform.to_canvas(expected);
            assert!(
                (a.x - b.x).hypot(a.y - b.y) < 0.01,
                "canvas flag {flag}: {a:?} != {b:?}"
            );
        }
        editor.execute(EditorCommand::SelectObject(control.object.clone()))?;
        editor.execute(EditorCommand::SetTool(ToolId::Deformers))?;
        let overlay = editor.overlay();
        for (vertex, expected) in overlay.vertices.iter().zip(&control.points) {
            assert_eq!(vertex.position, frame.transform.to_canvas(*expected));
        }
    }
    Ok(())
}

#[test]
fn reversed_canvas_control_drags_follow_the_displayed_pointer() -> anyhow::Result<()> {
    let mut canvas = kasane_core::Canvas::new(320., 280., Vec2::new(110., 150.), 100.);
    canvas.flag = 0;
    let (doc, _dir, warp, rotation, _) = rich_document_with_canvas(canvas)?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 640.,
        height: 560.,
        scale_factor: 2.,
    });
    for object in [
        mesh,
        ObjectRef::new(ObjectKind::Deformer, &warp),
        ObjectRef::new(ObjectKind::Deformer, &rotation),
    ] {
        editor.execute(EditorCommand::SelectObject(object.clone()))?;
        editor.execute(EditorCommand::SetTool(if object.kind == ObjectKind::Mesh {
            ToolId::Vertices
        } else {
            ToolId::Deformers
        }))?;
        let position = |f: &CanvasFrame| {
            if object.kind == ObjectKind::Mesh {
                f.transform
                    .to_canvas(f.geometry.mesh(&object.id).unwrap().vertices[0].1)
            } else {
                let c = f
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == object)
                    .unwrap();
                f.geometry.control_points(c, f.transform)[0]
            }
        };
        let frame = shown(&mut editor)?;
        let start = position(&frame);
        let end = CanvasPoint {
            x: start.x + 8.,
            y: start.y + 12.,
        };
        editor.canvas_input(CanvasInput::Down {
            point: start,
            shift: false,
        })?;
        editor.canvas_input(CanvasInput::Up { point: end })?;
        let after = position(&shown(&mut editor)?);
        assert!(
            (after.x - end.x).hypot(after.y - end.y) < 0.01,
            "{}: {after:?} != {end:?}",
            object.id
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        let restored = position(&shown(&mut editor)?);
        assert!((restored.x - start.x).hypot(restored.y - start.y) < 0.01);
    }
    // The visible rotation axis must follow the cursor in the same final space.
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let points = frame.geometry.control_points(c, frame.transform);
    let end = CanvasPoint {
        x: points[0].x,
        y: points[0].y + 54.,
    };
    editor.canvas_input(CanvasInput::Down {
        point: points[1],
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up { point: end })?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let after = frame.geometry.control_points(c, frame.transform)[1];
    assert!(
        (after.x - end.x).hypot(after.y - end.y) < 0.01,
        "{after:?} != {end:?}"
    );
    Ok(())
}

#[test]
fn pose_updates_share_topology_but_publish_independent_positions() -> anyhow::Result<()> {
    let (doc, _dir) = support::sample_document_with_binding(true)?;
    let parameter_id = doc.parameters()[0].id.clone();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    let original = shown(&mut editor)?;
    let positions = original.geometry.meshes[0].vertices.clone();
    let point = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: point.x + 20.,
            y: point.y,
        },
    })?;
    let edited = shown(&mut editor)?;
    assert_ne!(edited.geometry.meshes[0].vertices, positions);
    assert_eq!(
        original.geometry.meshes[0].vertices, positions,
        "old displayed frames are immutable"
    );
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id,
        value: 1.,
    })?;
    let other_key = shown(&mut editor)?;
    assert_eq!(other_key.geometry.meshes[0].vertices, positions);
    for frame in [&edited, &other_key] {
        assert!(Arc::ptr_eq(&original.geometry.index, &frame.geometry.index));
        assert!(Arc::ptr_eq(
            &original.geometry.meshes[0].triangles,
            &frame.geometry.meshes[0].triangles
        ));
        assert!(Arc::ptr_eq(
            &original.geometry.meshes[0].edges,
            &frame.geometry.meshes[0].edges
        ));
        assert!(!Arc::ptr_eq(
            &original.geometry.meshes,
            &frame.geometry.meshes
        ));
    }
    Ok(())
}

#[test]
fn opacity_reuses_positions_with_fresh_picking_visibility_and_history() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let top = editor
        .layers()
        .into_iter()
        .find(|l| l.name == "Second")
        .unwrap()
        .id;
    editor.execute(EditorCommand::Select(top.clone()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Vertices))?;
    let original = shown(&mut editor)?;
    let point = ModelPoint { x: 128., y: 128. };
    assert_eq!(original.geometry.mesh_hits(point)[0], top);
    for opacity in [0., 0.5, 1., 0.] {
        editor.execute(EditorCommand::PreviewOpacity(opacity))?;
        let frame = shown(&mut editor)?;
        assert!(Arc::ptr_eq(
            &original.geometry.meshes,
            &frame.geometry.meshes
        ));
        assert!(Arc::ptr_eq(&original.geometry.index, &frame.geometry.index));
        assert_eq!(frame.geometry.mesh_visible(&top), opacity > 0.);
        assert_eq!(frame.geometry.mesh_hits(point).contains(&top), opacity > 0.);
        assert_eq!(editor.overlay().vertices.is_empty(), opacity == 0.);
        assert!(original.geometry.mesh_visible(&top));
        assert_eq!(original.geometry.mesh_hits(point)[0], top);
    }
    editor.execute(EditorCommand::FinishEdit)?;
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(shown(&mut editor)?.geometry.mesh_hits(point)[0], top);
    editor.execute(EditorCommand::Redo)?;
    assert!(!shown(&mut editor)?.geometry.mesh_hits(point).contains(&top));
    editor.execute(EditorCommand::PreviewOpacity(1.))?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::CancelEdit)?;
    assert!(!shown(&mut editor)?.geometry.mesh_visible(&top));
    Ok(())
}

#[test]
fn canvas_metadata_refreshes_on_rename_cancel_property_edit_and_history() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let original = shown(&mut editor)?;
    editor.execute(EditorCommand::PreviewName("Renamed".into()))?;
    let renamed = shown(&mut editor)?;
    assert_eq!(renamed.geometry.index.name(&mesh), "Renamed");
    assert_eq!(original.geometry.index.name(&mesh), "Mesh");
    editor.execute(EditorCommand::CancelEdit)?;
    assert_eq!(shown(&mut editor)?.geometry.index.name(&mesh), "Mesh");
    let warp = ObjectRef::new(ObjectKind::Deformer, warp);
    editor.execute(EditorCommand::SetObjectProperty {
        object: warp.clone(),
        field: "name".into(),
        value: "Updated Warp".into(),
    })?;
    assert_eq!(
        shown(&mut editor)?.geometry.index.name(&warp),
        "Updated Warp"
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(shown(&mut editor)?.geometry.index.name(&warp), "Warp");
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        shown(&mut editor)?.geometry.index.name(&warp),
        "Updated Warp"
    );
    assert_eq!(renamed.geometry.index.name(&mesh), "Renamed");
    Ok(())
}
