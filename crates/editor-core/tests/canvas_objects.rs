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
        cache_key: id.into(),
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
    assert_eq!(editor.overlay().vertices.len(), 4);
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
fn hit_test_miss_clears_objects_and_components_even_with_shift() -> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = shown(&mut editor)?;
    let miss = CanvasPoint { x: -100., y: -100. };
    for object in [
        mesh,
        ObjectRef::new(ObjectKind::Deformer, warp),
        ObjectRef::new(ObjectKind::Deformer, rotation),
    ] {
        for shift in [false, true] {
            editor.execute(EditorCommand::SelectObject(object.clone()))?;
            let point = editor.overlay().vertices[0].position;
            click(&mut editor, point, false)?;
            assert!(editor.overlay().vertices.iter().any(|v| v.selected));
            assert!(
                frame
                    .geometry
                    .hit(
                        miss,
                        frame.transform,
                        &[object.clone()],
                        ToolId::Select,
                        DeformerVisibility::default(),
                    )
                    .is_none()
            );
            click(&mut editor, miss, shift)?;
            let doc = editor.document().unwrap();
            assert!(doc.context().selections.is_empty());
            assert!(doc.context().selection.is_none());
            assert!(doc.context().inspected.is_none());
            assert_eq!(doc.history_lengths(), (0, 0));
            assert!(editor.overlay().vertices.is_empty());
            editor.execute(EditorCommand::SelectObject(object.clone()))?;
            assert!(editor.overlay().vertices.iter().all(|v| !v.selected));
        }
    }
    Ok(())
}
#[test]
fn texture_alpha_picking_matches_renderer_v_orientation_for_surfaces_and_masks()
-> anyhow::Result<()> {
    let (mut doc, _dir) = support::sample_document_with_two_layers()?;
    let mut source = pixel_scene(&mut doc)?;
    // Decoded pixels are top-down; Drawable UVs are bottom-up. A horizontally
    // asymmetric texture alone cannot expose a missing V conversion.
    let mut textures = source.textures.to_vec();
    textures.push(SceneTexture {
        asset_id: "vertical-cutout".into(),
        cache_key: "vertical-cutout".into(),
        width: 1,
        height: 2,
        rgba: vec![255, 255, 255, 255, 255, 255, 255, 0].into(),
    });
    source.textures = textures.into();
    let bottom = source.frame.drawables[0].id.clone();
    let top = source.frame.drawables[1].id.clone();
    let frame = Arc::make_mut(&mut source.frame);
    // Set UVs explicitly in the renderer's convention for this diamond fixture.
    for d in &mut frame.drawables {
        d.uvs = d
            .positions
            .iter()
            .map(|p| {
                let p = ViewportTransform::evaluated_to_source(frame.canvas, *p);
                Vec2::new((p.x - 24.) / 208., 1. - (p.y - 24.) / 208.)
            })
            .collect();
    }
    frame.drawables[1].texture_asset_id = "vertical-cutout".into();
    let upper = ModelPoint { x: 128., y: 64. };
    let lower = ModelPoint { x: 128., y: 192. };
    let scene = CanvasScene::new(&doc, source.clone());
    assert_eq!(scene.mesh_hits(upper), [top.clone(), bottom.clone()]);
    assert_eq!(scene.mesh_hits(lower), [bottom.clone()]);

    // The same conversion must apply to mask sources, including inverted masks.
    let frame = Arc::make_mut(&mut source.frame);
    frame.drawables[0].texture_asset_id = "vertical-cutout".into();
    frame.drawables[1].texture_asset_id = "solid".into();
    frame.drawables[1].masks = vec![bottom.clone()];
    let scene = CanvasScene::new(&doc, source.clone());
    assert_eq!(scene.mesh_hits(upper), [top.clone(), bottom.clone()]);
    assert!(scene.mesh_hits(lower).is_empty());
    Arc::make_mut(&mut source.frame).drawables[1].inverted_mask = true;
    let scene = CanvasScene::new(&doc, source);
    assert_eq!(scene.mesh_hits(upper), [bottom]);
    assert_eq!(scene.mesh_hits(lower), [top]);
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
    click(&mut editor, CanvasPoint { x: 256., y: 256. }, true)?;
    assert_eq!(editor.document().unwrap().context().selections.len(), 2);
    click(&mut editor, CanvasPoint { x: 256., y: 256. }, true)?;
    assert_eq!(editor.document().unwrap().context().selections.len(), 2);
    click(&mut editor, CanvasPoint { x: 2., y: 2. }, false)?;
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
    editor.dispatch(editor.command(CommandId::GlueTool))?;
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
    assert_eq!(editor.active_tool(), ToolId::Glue);
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
                bezier: None,
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
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
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
    for tool in [ToolId::Select, ToolId::Glue, ToolId::Hand] {
        editor.execute(EditorCommand::SetTool(tool))?;
        let overlay = editor.overlay();
        assert!(overlay.lines.is_empty(), "unselected deformers stay hidden");
        assert_eq!(
            overlay
                .deformers
                .iter()
                .map(|d| d.lines.len())
                .sum::<usize>(),
            64,
            "the selected warp Bezier curves remain visible"
        );
        assert_eq!(overlay.vertices.len(), 4);
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
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
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
    let start = points[1];
    let end = CanvasPoint {
        x: center.x,
        y: center.y - (start.x - center.x).hypot(start.y - center.y),
    };
    assert!(editor.present_frame(frame));
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    assert!(
        editor
            .overlay()
            .vertices
            .iter()
            .any(|v| v.kind == OverlayVertexKind::RotationTip && v.selected),
        "pressed rotation tip is visually active"
    );
    editor.canvas_input(CanvasInput::Move {
        point: end,
        left_down: true,
    })?;
    assert!(editor.document().unwrap().has_pending_edit());
    editor.dispatch(editor.command(CommandId::CancelGesture))?;
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    shown(&mut editor)?;
    assert!(
        editor.overlay().vertices.iter().all(|v| !v.selected),
        "cancel clears the active rotation tip"
    );
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
    editor.execute(EditorCommand::SetVisible(false))?;
    let frame = shown(&mut editor)?;
    assert!(editor.overlay().vertices.is_empty());
    let scene = &frame.geometry;
    assert!(!matches!(
        scene.hit(
            CanvasPoint { x: 0., y: 0. },
            frame.transform,
            &editor.document().unwrap().context().selections,
            ToolId::Select,
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
        editor.execute(EditorCommand::SetTool(ToolId::Select))?;
        let overlay = editor.overlay();
        for (vertex, expected) in overlay.vertices.iter().zip(&control.points) {
            let expected = frame.transform.to_canvas(*expected);
            assert!((vertex.position.x - expected.x).hypot(vertex.position.y - expected.y) < 0.001);
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
        y: points[0].y + (points[1].x - points[0].x).hypot(points[1].y - points[0].y),
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
fn mesh_drag_reuses_unchanged_meshes_without_changing_old_frames() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original = shown(&mut editor)?;
    let selected = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    let unchanged = original
        .geometry
        .meshes
        .iter()
        .find(|mesh| mesh.id != selected.id)
        .unwrap();
    let positions = original
        .geometry
        .mesh(&selected.id)
        .unwrap()
        .vertices
        .clone();
    let point = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    for dx in [20., 40.] {
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint {
                x: point.x + dx,
                y: point.y,
            },
            left_down: true,
        })?;
        let edited = shown(&mut editor)?;
        let same = edited
            .geometry
            .meshes
            .iter()
            .find(|mesh| mesh.id == unchanged.id)
            .unwrap();
        assert!(Arc::ptr_eq(unchanged, same));
        assert_ne!(
            edited.geometry.mesh(&selected.id).unwrap().vertices,
            positions
        );
        assert_eq!(
            original.geometry.mesh(&selected.id).unwrap().vertices,
            positions
        );
        let current = &edited.geometry.mesh(&selected.id).unwrap().vertices[0];
        assert_eq!(
            edited.geometry.hit(
                edited.transform.to_canvas(current.1),
                edited.transform,
                std::slice::from_ref(&selected),
                ToolId::Select,
                DeformerVisibility::default(),
            ),
            Some(HitTarget::Component(ComponentRef {
                object: selected.clone(),
                id: current.0
            })),
        );
    }
    editor.canvas_input(CanvasInput::Cancel)?;
    assert_eq!(
        shown(&mut editor)?
            .geometry
            .mesh(&selected.id)
            .unwrap()
            .vertices,
        positions
    );
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

fn deformer_editor(rotation: bool) -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef)> {
    let (doc, dir, warp, rotate, _) = rich_document()?;
    let object = ObjectRef::new(ObjectKind::Deformer, if rotation { rotate } else { warp });
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    shown(&mut editor)?;
    Ok((editor, dir, object))
}
fn drag(
    editor: &mut Editor,
    start: CanvasPoint,
    end: CanvasPoint,
    shift: bool,
) -> anyhow::Result<()> {
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift,
    })?;
    editor.canvas_input(CanvasInput::Up { point: end })?;
    Ok(())
}
#[test]
fn warp_and_rotation_drags_present_intermediate_geometry_before_the_latest_pose()
-> anyhow::Result<()> {
    for rotation in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(rotation)?;
        let start = editor.overlay().vertices[0].position;
        editor.canvas_input(CanvasInput::Down {
            point: start,
            shift: false,
        })?;
        let mut frames = Vec::new();
        for x in [10., 20., 30.] {
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
        assert!(editor.has_presented_frame(Some(frames[0].version)));
        let displayed = frames[0]
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let anchor = frames[0].transform.to_canvas(displayed.points[0]);
        assert_eq!(editor.overlay().vertices[0].position, anchor);
        assert!((anchor.x - start.x - 10.).abs() < 0.01);
        assert!(editor.present_frame(frames[1].clone()));
        assert!(!editor.present_frame(frames[0].clone()));
        editor.canvas_input(CanvasInput::Up {
            point: CanvasPoint {
                x: start.x + 30.,
                y: start.y,
            },
        })?;
        assert!(editor.present_frame(frames[2].clone()));
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        shown(&mut editor)?;
        assert!((editor.overlay().vertices[0].position.x - start.x).abs() < 0.01);
    }
    Ok(())
}

fn control_points(editor: &mut Editor, object: &ObjectRef) -> anyhow::Result<Vec<ModelPoint>> {
    Ok(shown(editor)?
        .geometry
        .controls
        .iter()
        .find(|c| &c.object == object)
        .unwrap()
        .points
        .clone())
}
#[test]
fn warp_marquee_preserves_object_and_moves_selected_controls_only() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let p = editor.overlay().vertices[0].position;
    // Cross the grid's first row, starting outside the gizmo.
    drag(
        &mut editor,
        CanvasPoint {
            x: p.x - 35.,
            y: p.y - 10.,
        },
        CanvasPoint {
            x: p.x + 400.,
            y: p.y + 10.,
        },
        false,
    )?;
    assert_eq!(
        editor.document().unwrap().context().selections,
        [object.clone()]
    );
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        2
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    drag(
        &mut editor,
        p,
        CanvasPoint {
            x: p.x + 20.,
            y: p.y + 10.,
        },
        false,
    )?;
    let after = control_points(&mut editor, &object)?;
    for i in 0..2 {
        assert!((after[i].x - before[i].x - 10.).abs() < 0.001);
    }
    assert_eq!(&after[2..], &before[2..]);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    Ok(())
}
#[test]
fn warp_gizmo_scales_all_points_and_cancels_as_one_edit() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let start = gizmo
        .handles
        .iter()
        .find(|(h, _)| *h == DeformerHandle::Scale(4))
        .unwrap()
        .1;
    let end = CanvasPoint {
        x: start.x + 96.,
        y: start.y + 40.,
    };
    assert_eq!(
        frame.geometry.hit(
            start,
            frame.transform,
            &[object.clone()],
            ToolId::Select,
            DeformerVisibility::default()
        ),
        Some(HitTarget::DeformerHandle(
            object.clone(),
            DeformerHandle::Scale(4)
        ))
    );
    drag(&mut editor, start, end, true)?;
    let after = control_points(&mut editor, &object)?;
    assert_eq!(after[0], before[0]);
    assert!((after[3].x - before[0].x - 240.).abs() < 0.001);
    assert!((after[3].y - before[0].y - 240.).abs() < 0.001);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    shown(&mut editor)?;
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: end,
        left_down: true,
    })?;
    assert!(editor.overlay().interaction_label.is_some());
    editor.canvas_input(CanvasInput::Cancel)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    Ok(())
}
#[test]
fn warp_outer_edge_moves_without_selecting_descendant_mesh() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let (a, b) = gizmo.outline[0];
    let start = CanvasPoint {
        x: (a.x + b.x) / 2.,
        y: a.y,
    };
    drag(
        &mut editor,
        start,
        CanvasPoint {
            x: start.x + 20.,
            y: start.y - 16.,
        },
        false,
    )?;
    let after = control_points(&mut editor, &object)?;
    for (a, b) in before.iter().zip(&after) {
        assert!((b.x - a.x - 10.).abs() < 0.001);
        assert!((b.y - a.y + 8.).abs() < 0.001);
    }
    assert_eq!(editor.document().unwrap().context().selections, [object]);
    Ok(())
}
#[test]
fn rotation_scale_handle_and_numeric_adjustment_preserve_pivot_and_history() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let before = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let start = gizmo.handles[0].1;
    let end = CanvasPoint {
        x: gizmo.center.x + 2. * (start.x - gizmo.center.x),
        y: gizmo.center.y + 2. * (start.y - gizmo.center.y),
    };
    drag(&mut editor, start, end, false)?;
    let after = control_points(&mut editor, &object)?;
    assert_eq!(before[0], after[0]);
    let index = editor.document().unwrap().object_index();
    assert_eq!(
        index.get(&object).unwrap().properties["rotation"]["pose"]["scale"],
        2.
    );
    editor.execute(EditorCommand::Undo)?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelPoint { x: 10., y: 20. },
            angle: 90.,
            scale: 1.5,
        },
    })?;
    let after = control_points(&mut editor, &object)?;
    assert!((after[0].x - before[0].x - 10.).abs() < 0.001);
    assert!((after[0].y - before[0].y - 20.).abs() < 0.001);
    assert!(
        after[1].y < after[0].y,
        "positive numeric angle turns counterclockwise"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelPoint::default(),
            angle: 0.,
            scale: 1.,
        },
    })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    assert!(
        editor
            .execute(EditorCommand::AdjustDeformer {
                object: object.clone(),
                adjustment: DeformerAdjustment {
                    translation: ModelPoint { x: 10., y: 0. },
                    angle: 0.,
                    scale: 101.
                }
            })
            .is_err()
    );
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert!(!editor.document().unwrap().has_pending_edit());
    Ok(())
}
#[test]
fn delayed_bezier_frames_keep_their_own_subdivision_grid() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    editor.execute(EditorCommand::ToggleDeformerGrid)?;
    shown(&mut editor)?;
    let original_grid = editor.overlay().guide_lines;
    assert!(!original_grid.is_empty());
    let start = editor
        .overlay()
        .handles
        .iter()
        .find(|h| h.round)
        .unwrap()
        .position;
    let bent = CanvasPoint {
        x: start.x,
        y: start.y - 40.,
    };
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: bent,
        left_down: true,
    })?;
    let refined = shown(&mut editor)?;
    assert_eq!(
        refined
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .points
            .len(),
        25
    );
    let refined_grid = editor.overlay().guide_lines;
    assert!(refined_grid.len() > original_grid.len());

    // Queue another refined pose, then restore the legacy lattice before it arrives.
    let delayed = editor.preview()?.canvas.unwrap();
    editor.canvas_input(CanvasInput::Move {
        point: start,
        left_down: true,
    })?;
    let restored = editor.preview()?.canvas.unwrap();
    assert_eq!(
        restored
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .points
            .len(),
        4
    );
    assert!(editor.overlay().guide_lines == refined_grid);
    assert!(editor.present_frame(delayed));
    assert!(editor.overlay().guide_lines == refined_grid);
    assert!(editor.present_frame(restored));
    assert!(editor.overlay().guide_lines == original_grid);
    editor.canvas_input(CanvasInput::Up { point: start })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn subdivisions_are_opt_in_and_never_become_hit_targets() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let frame = shown(&mut editor)?;
    assert!(editor.overlay().guide_lines.is_empty());
    editor.execute(EditorCommand::ToggleDeformerGrid)?;
    assert!(!editor.overlay().guide_lines.is_empty());
    let overlay = editor.overlay();
    let p = overlay
        .guide_lines
        .iter()
        .map(|(a, b)| CanvasPoint {
            x: (a.x + b.x) / 2.,
            y: (a.y + b.y) / 2.,
        })
        .find(|p| {
            overlay
                .vertices
                .iter()
                .map(|v| v.position)
                .chain(overlay.handles.iter().map(|h| h.position))
                .all(|q| (q.x - p.x).hypot(q.y - p.y) > 8.)
        })
        .unwrap();
    let hit = frame.geometry.hit(
        p,
        frame.transform,
        &[object],
        ToolId::Select,
        DeformerVisibility::default(),
    );
    assert!(!matches!(hit, Some(HitTarget::Component(_))));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

fn bound_deformer_editor(
    rotation: bool,
    reversed: bool,
) -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef, String)> {
    use kasane_core::{
        BindingAxis, Parameter, RotationKeyform, RotationPose, RotationTransform, SceneBinding,
        SceneTrack, Transform, TransformData, WarpKeyform, WarpTransform,
    };
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    let dir = tempfile::tempdir()?;
    let mut canvas = kasane_core::Canvas::new(256., 256., Vec2::new(0., 0.), 1.);
    if reversed {
        canvas.flag = 0;
    } else {
        canvas.flag = 1;
    }
    let mut session = kasane_sdk::AuthoringSession::new(&id(21), canvas)?;
    let positions = vec![
        Vec2::new(0.1, 0.1),
        Vec2::new(0.9, 0.1),
        Vec2::new(0.1, 0.9),
        Vec2::new(0.9, 0.9),
    ];
    let pose = RotationPose {
        origin: kasane_core::PreciseVec2::new(0.5123456789, 0.4123456789),
        ..Default::default()
    };
    session.edit("fixture", None, |edit| {
        edit.create_transform(Transform {
            id: id(22),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(20., 30.),
                    Vec2::new(220., 30.),
                    Vec2::new(20., 230.),
                    Vec2::new(220., 230.),
                ],
            }),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: id(23),
            parent_id: Some(id(22).into()),
            data: if rotation {
                TransformData::Rotation(RotationTransform {
                    pose,
                    ..Default::default()
                })
            } else {
                TransformData::Warp(WarpTransform {
                    bezier: None,
                    rows: 1,
                    columns: 1,
                    quad: true,
                    points: positions.clone(),
                })
            },
            ..Default::default()
        })?;
        edit.create_parameter(Parameter {
            id: id(24),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
        edit.create_scene_binding(SceneBinding {
            id: id(25),
            axes: vec![BindingAxis {
                parameter_id: id(24),
                keys: vec![0., 1.],
            }],
            track: if rotation {
                SceneTrack::Rotation {
                    target_id: id(23).into(),
                    keyforms: vec![0., 1.]
                        .into_iter()
                        .map(|v| RotationKeyform {
                            keys: vec![v],
                            rotation: pose,
                            ..Default::default()
                        })
                        .collect(),
                }
            } else {
                SceneTrack::Warp {
                    target_id: id(23).into(),
                    keyforms: vec![0., 1.]
                        .into_iter()
                        .map(|v| WarpKeyform {
                            bezier: None,
                            keys: vec![v],
                            positions: positions.clone(),
                            ..Default::default()
                        })
                        .collect(),
                }
            },
        })?;
        Ok(())
    })?;
    let saved = session.save_project(&dir.path().join("bound"), None)?;
    let mut editor = Editor::from_document(Document::open(&saved.manifest)?);
    let object = ObjectRef::new(ObjectKind::Deformer, id(23));
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    shown(&mut editor)?;
    Ok((editor, dir, object, id(24)))
}

#[test]
fn unqueried_warp_snapshot_retains_keyform_and_parent_space_after_edits() -> anyhow::Result<()> {
    for reversed in [false, true] {
        let (mut editor, _dir, object, parameter_id) = bound_deformer_editor(false, reversed)?;
        // No overlay or hit query has synthesized the old frame's controller yet.
        let old = shown(&mut editor)?;
        let parent = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(22).to_string());
        let before = old
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .points[0];
        for target in [&parent, &object] {
            editor.execute(EditorCommand::AdjustDeformer {
                object: target.clone(),
                adjustment: DeformerAdjustment {
                    translation: ModelPoint { x: 12., y: 8. },
                    angle: 0.,
                    scale: 1.,
                },
            })?;
        }
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id,
            value: 0.5,
        })?;
        let current = shown(&mut editor)?;
        assert_ne!(
            current
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap()
                .points[0],
            before
        );
        // Late selection of the retained frame must still hit its original anchor.
        assert_eq!(
            old.geometry.hit(
                old.transform.to_canvas(before),
                old.transform,
                std::slice::from_ref(&object),
                ToolId::Select,
                DeformerVisibility::default(),
            ),
            Some(HitTarget::Component(ComponentRef {
                object: object.clone(),
                id: 1 << 31,
            }))
        );
        // At an interpolated parameter value the current frame uses lattice controls.
        let first = editor.overlay().vertices[0].clone();
        assert_eq!(first.id, 0);
    }
    Ok(())
}

#[test]
fn nested_deformer_adjustments_write_only_the_exact_keyform_in_both_canvas_directions()
-> anyhow::Result<()> {
    for rotation in [false, true] {
        for reversed in [false, true] {
            let (mut editor, _dir, object, parameter_id) =
                bound_deformer_editor(rotation, reversed)?;
            let original = control_points(&mut editor, &object)?;
            editor.execute(EditorCommand::AdjustDeformer {
                object: object.clone(),
                adjustment: DeformerAdjustment {
                    translation: ModelPoint { x: 12., y: 8. },
                    angle: 0.,
                    scale: 1.,
                },
            })?;
            let changed = control_points(&mut editor, &object)?;
            for (a, b) in original.iter().zip(&changed) {
                assert!((b.x - a.x - 12.).abs() < 0.01);
                assert!((b.y - a.y - 8.).abs() < 0.01);
            }
            editor.execute(EditorCommand::SetPreviewParameter {
                parameter_id: parameter_id.clone(),
                value: 1.,
            })?;
            assert_eq!(control_points(&mut editor, &object)?, original);
            editor.execute(EditorCommand::SetPreviewParameter {
                parameter_id: parameter_id.clone(),
                value: 0.5,
            })?;
            let interpolation = control_points(&mut editor, &object)?;
            assert!(editor.overlay().unavailable.is_some());
            assert!(
                editor
                    .execute(EditorCommand::AdjustDeformer {
                        object: object.clone(),
                        adjustment: DeformerAdjustment {
                            translation: ModelPoint::default(),
                            angle: 25.,
                            scale: 1.2
                        }
                    })
                    .is_err()
            );
            assert_eq!(control_points(&mut editor, &object)?, interpolation);
            assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
            assert!(!editor.document().unwrap().has_pending_edit());
            editor.execute(EditorCommand::SetPreviewParameter {
                parameter_id,
                value: 0.,
            })?;
            shown(&mut editor)?;
            editor.execute(EditorCommand::Undo)?;
            assert_eq!(control_points(&mut editor, &object)?, original);
        }
    }
    Ok(())
}
#[test]
fn numeric_warp_rotation_and_scale_use_displayed_center_under_parent() -> anyhow::Result<()> {
    let (mut editor, _dir, object, _) = bound_deformer_editor(false, true)?;
    let before = control_points(&mut editor, &object)?;
    let center = ModelPoint {
        x: (before[0].x + before[3].x) / 2.,
        y: (before[0].y + before[3].y) / 2.,
    };
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelPoint::default(),
            angle: 90.,
            scale: 0.5,
        },
    })?;
    let after = control_points(&mut editor, &object)?;
    for (a, b) in before.iter().zip(&after) {
        assert!((b.x - (center.x + (a.y - center.y) * 0.5)).abs() < 0.01);
        assert!((b.y - (center.y - (a.x - center.x) * 0.5)).abs() < 0.01);
    }
    Ok(())
}

#[test]
fn rotation_gizmo_follows_canvas_scale_with_matching_pick_geometry() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let original = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let scene = &frame.geometry;
    let control = scene.controls.iter().find(|c| c.object == object).unwrap();
    for width in [256., 512.] {
        for zoom in [0.5, 1., 2.] {
            let mut logical_positions = None;
            for dpi in [1., 2.] {
                let transform = ViewportTransform::new(
                    scene.source.frame.canvas,
                    CanvasMetrics {
                        width,
                        height: width,
                        scale_factor: dpi,
                    },
                    ViewState {
                        zoom,
                        pan: CanvasPoint { x: 31., y: -19. },
                    },
                )
                .unwrap();
                let points = scene.control_points(control, transform);
                let gizmo = scene.deformer_gizmo(control, transform).unwrap();
                let radius = |p: CanvasPoint| (p.x - gizmo.center.x).hypot(p.y - gizmo.center.y);
                assert!((radius(points[1]) - 540. * transform.scale).abs() < 0.001);
                assert!((radius(gizmo.outline[0].0) - 280. * transform.scale).abs() < 0.001);
                assert!((radius(gizmo.handles[0].1) - 400. * transform.scale).abs() < 0.001);
                let selection = std::slice::from_ref(&object);
                let hit = |p| {
                    scene.hit(
                        p,
                        transform,
                        selection,
                        ToolId::Select,
                        DeformerVisibility::default(),
                    )
                };
                assert_eq!(
                    hit(points[1]),
                    Some(HitTarget::RotationHandle(object.clone()))
                );
                assert_eq!(
                    hit(gizmo.handles[0].1),
                    Some(HitTarget::DeformerHandle(
                        object.clone(),
                        DeformerHandle::RotationScale
                    ))
                );
                // The opposite quadrant of the scaled ring is clear of the axes.
                assert_eq!(
                    hit(gizmo.outline[30].0),
                    Some(HitTarget::DeformerHandle(
                        object.clone(),
                        DeformerHandle::Rotate
                    ))
                );
                let positions = (points, gizmo.outline, gizmo.handles);
                if let Some(previous) = logical_positions {
                    assert_eq!(positions, previous, "DPI must not apply the zoom twice");
                }
                logical_positions = Some(positions);
            }
        }
    }
    editor.zoom_by(1.);
    assert_eq!(control_points(&mut editor, &object)?, original);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn bezier_tangent_bends_lattice_persists_and_undo_restores_legacy_shape() -> anyhow::Result<()> {
    let (mut editor, dir, object) = deformer_editor(false)?;
    let original = control_points(&mut editor, &object)?;
    let original_mesh = shown(&mut editor)?.geometry.meshes[0].vertices.clone();
    let overlay = editor.overlay();
    assert_eq!(overlay.vertices.len(), 4);
    let handle = overlay.handles.iter().find(|h| h.round).unwrap().position;
    drag(
        &mut editor,
        handle,
        CanvasPoint {
            x: handle.x,
            y: handle.y - 40.,
        },
        false,
    )?;
    let bent = control_points(&mut editor, &object)?;
    assert_eq!(
        bent.len(),
        25,
        "a four-corner grid must be refined to express curvature"
    );
    for (i, j) in [0, 4, 20, 24].into_iter().enumerate() {
        assert!(
            (bent[j].x - original[i].x).hypot(bent[j].y - original[i].y) < 0.001,
            "tangent drag must keep anchors fixed"
        );
    }
    assert!(bent[1].y < original[0].y - 1.);
    assert_ne!(
        shown(&mut editor)?.geometry.meshes[0].vertices,
        original_mesh
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let index = editor.document().unwrap().object_index();
    let stored = index.get(&object).unwrap().properties["warp"]["bezier"].clone();
    assert!(stored.is_object());
    let id = editor.active_id().unwrap();
    let task = editor.prepare_save(id, &dir.path().join("bezier-saved"), false)?;
    let receipt = editor.finish_save(&mut task.run())?;
    let reopened = Document::open(&receipt.manifest)?;
    assert_eq!(
        reopened.object_index().get(&object).unwrap().properties["warp"]["bezier"],
        stored
    );
    let mut reopened = Editor::from_document(reopened);
    reopened.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    reopened.execute(EditorCommand::SelectObject(object.clone()))?;
    assert_eq!(control_points(&mut reopened, &object)?, bent);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, original);
    assert!(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]
            .get("bezier")
            .is_none()
    );
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(control_points(&mut editor, &object)?, bent);
    editor.execute(EditorCommand::ToggleWarpFine)?;
    assert_eq!(editor.overlay().vertices.len(), 25);
    assert_eq!(
        editor.overlay().handles.iter().filter(|h| h.round).count(),
        0
    );
    let point = editor.overlay().vertices[12].position;
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: CanvasPoint {
            x: point.x + 10.,
            y: point.y,
        },
        left_down: true,
    })?;
    editor.canvas_input(CanvasInput::Up { point })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert_eq!(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]["bezier"],
        stored
    );
    Ok(())
}

#[test]
fn bezier_keyform_edit_preserves_other_shapes_and_cancels_refinement() -> anyhow::Result<()> {
    for reversed in [false, true] {
        let (mut editor, dir, object, param) = bound_deformer_editor(false, reversed)?;
        let original = control_points(&mut editor, &object)?;
        let handle = editor
            .overlay()
            .handles
            .iter()
            .find(|h| h.round)
            .unwrap()
            .position;
        let end = CanvasPoint {
            x: handle.x + 24.,
            y: handle.y - 20.,
        };
        editor.canvas_input(CanvasInput::Down {
            point: handle,
            shift: false,
        })?;
        editor.canvas_input(CanvasInput::Move {
            point: end,
            left_down: true,
        })?;
        assert_eq!(control_points(&mut editor, &object)?.len(), 25);
        editor.canvas_input(CanvasInput::Cancel)?;
        assert_eq!(control_points(&mut editor, &object)?, original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        // Dragging back restores both grid topology and absent controller metadata.
        editor.canvas_input(CanvasInput::Down {
            point: handle,
            shift: false,
        })?;
        editor.canvas_input(CanvasInput::Move {
            point: end,
            left_down: true,
        })?;
        editor.canvas_input(CanvasInput::Up { point: handle })?;
        assert_eq!(control_points(&mut editor, &object)?, original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        drag(&mut editor, handle, end, false)?;
        let edited = control_points(&mut editor, &object)?;
        let saved_handle = editor
            .overlay()
            .handles
            .iter()
            .find(|h| h.round)
            .unwrap()
            .position;
        assert!(
            (saved_handle.x - end.x).hypot(saved_handle.y - end.y) < 0.01,
            "handle must follow pointer through parent deformation"
        );
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: param.clone(),
            value: 1.,
        })?;
        let other = control_points(&mut editor, &object)?;
        for (i, j) in [0, 4, 20, 24].into_iter().enumerate() {
            assert!((other[j].x - original[i].x).hypot(other[j].y - original[i].y) < 0.001);
        }
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: param.clone(),
            value: 0.,
        })?;
        assert_eq!(control_points(&mut editor, &object)?, edited);
        let id = editor.active_id().unwrap();
        let task = editor.prepare_save(id, &dir.path().join("bezier-bound"), false)?;
        let receipt = editor.finish_save(&mut task.run())?;
        let mut reopened = Editor::from_document(Document::open(&receipt.manifest)?);
        reopened.set_canvas_metrics(CanvasMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 2.,
        });
        reopened.execute(EditorCommand::SelectObject(object.clone()))?;
        reopened.execute(EditorCommand::SetPreviewParameter {
            parameter_id: param,
            value: 0.,
        })?;
        assert_eq!(control_points(&mut reopened, &object)?, edited);
        let handle = reopened
            .overlay()
            .handles
            .iter()
            .find(|h| h.round)
            .unwrap()
            .position;
        assert!((handle.x - saved_handle.x).hypot(handle.y - saved_handle.y) < 0.001);
    }
    Ok(())
}

#[test]
fn overlay_keeps_control_identity_and_reference_deformers_quiet() -> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, _) = rich_document()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    shown(&mut editor)?;
    editor.dispatch(editor.command(CommandId::ToggleDeformers))?;
    let overlay = editor.overlay();
    assert!(
        overlay
            .vertices
            .iter()
            .all(|v| v.kind == OverlayVertexKind::Mesh)
    );
    assert_eq!(overlay.deformers.len(), 2);
    assert!(overlay.deformers.iter().all(|d| !d.selected));
    for (id, expected) in [
        (warp, OverlayVertexKind::Warp),
        (rotation, OverlayVertexKind::RotationCenter),
    ] {
        editor.execute(EditorCommand::SelectObject(ObjectRef::new(
            ObjectKind::Deformer,
            id,
        )))?;
        let overlay = editor.overlay();
        assert_eq!(overlay.vertices[0].kind, expected);
        assert!(
            overlay.accent_lines.is_empty(),
            "deformers must not inherit mesh styling"
        );
        assert_eq!(overlay.deformers.iter().filter(|d| d.selected).count(), 1);
        let point = overlay.vertices[0].position;
        click(&mut editor, point, false)?;
        assert!(editor.overlay().vertices.iter().any(|v| v.selected));
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn glue_overlay_retains_pairs_and_mesh_context_without_editable_vertices() -> anyhow::Result<()> {
    use kasane_core::{Canvas, Glue, GlueVertexPair};
    use kasane_sdk::AuthoringSession;
    for intensity in [0., 1.] {
        let (mut source, dir) = support::sample_document_with_two_layers()?;
        let receipt = source.save(&dir.path().join("glue-source"))?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut sdk = AuthoringSession::new(&id, Canvas::new(256., 256., Vec2::new(0., 0.), 1.))?;
        sdk.open_project(&receipt.manifest, None)?;
        let meshes = sdk.mesh_ids().to_vec();
        sdk.edit("Glue fixture", None, |edit| {
            edit.create_glue(Glue {
                id: id.clone(),
                runtime_id: id.clone(),
                name: "Seam".into(),
                mesh_a_id: meshes[0].clone(),
                mesh_b_id: meshes[1].clone(),
                intensity,
                pairs: vec![
                    GlueVertexPair {
                        vertex_a: 0,
                        vertex_b: 0,
                        weight_a: 0.5,
                        weight_b: 0.5,
                    },
                    GlueVertexPair {
                        vertex_a: 1,
                        vertex_b: 2,
                        weight_a: 0.5,
                        weight_b: 0.5,
                    },
                ],
                binding: None,
            })
        })?;
        sdk.save_project(&receipt.manifest, None)?;
        let mut editor = Editor::from_document(Document::open(&receipt.manifest)?);
        editor.set_canvas_metrics(CanvasMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 2.,
        });
        let frame = shown(&mut editor)?;
        editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
        assert!(editor.overlay().glues.is_empty());
        editor.execute(EditorCommand::SetTool(ToolId::Glue))?;
        let overlay = editor.overlay();
        assert_eq!(overlay.glues.len(), 1);
        assert!(!overlay.glues[0].selected);
        let object = ObjectRef::new(ObjectKind::Glue, &id);
        editor.execute(EditorCommand::SelectObject(object.clone()))?;
        let overlay = editor.overlay();
        assert!(overlay.vertices.is_empty() && overlay.handles.is_empty());
        assert_eq!(
            overlay.reference_lines.len(),
            8,
            "both connected mesh outlines are visible"
        );
        assert!(overlay.glues[0].selected);
        let pairs = &overlay.glues[0].pairs;
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            pairs[0].0, pairs[0].1,
            "coincident endpoints must not be discarded"
        );
        let (a, b) = pairs[1];
        assert_eq!((a.x - b.x).hypot(a.y - b.y) < 0.01, intensity == 1.);
        let point = pairs[0].0;
        assert_eq!(
            frame.geometry.hit(
                point,
                frame.transform,
                &[object.clone()],
                ToolId::Glue,
                DeformerVisibility::default()
            ),
            Some(HitTarget::Object(object))
        );
        editor.canvas_input(CanvasInput::Move {
            point,
            left_down: false,
        })?;
        assert!(editor.overlay().glues[0].hovered);
        click(&mut editor, point, false)?;
        assert!(editor.overlay().vertices.is_empty());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn mesh_drag_reuses_binding_index_without_mutating_retained_snapshots() -> anyhow::Result<()> {
    let (doc, _dir) = support::sample_document_with_binding(true)?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    shown(&mut editor)?;
    let index = editor.document().unwrap().object_index();
    let point = editor.overlay().vertices[0].position;
    let before = shown(&mut editor)?.geometry.meshes[0].vertices.clone();
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    for distance in [10., 20., 30.] {
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint {
                x: point.x + distance,
                y: point.y,
            },
            left_down: true,
        })?;
        assert!(Arc::ptr_eq(
            &index,
            &editor.document().unwrap().object_index()
        ));
        assert_ne!(shown(&mut editor)?.geometry.meshes[0].vertices, before);
    }
    editor.cancel_gesture();
    assert_eq!(shown(&mut editor)?.geometry.meshes[0].vertices, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn deformer_drag_refreshes_live_properties_and_preserves_old_index_snapshots() -> anyhow::Result<()>
{
    for rotation in [false, true] {
        let (mut editor, _dir, object) = deformer_editor(rotation)?;
        let original = editor.document().unwrap().object_index();
        let original_properties = original.get(&object).unwrap().properties.clone();
        let start = editor.overlay().vertices[0].position;
        editor.canvas_input(CanvasInput::Down {
            point: start,
            shift: false,
        })?;
        let mut previous = original.clone();
        for distance in [10., 20.] {
            let retained_properties = previous.get(&object).unwrap().properties.clone();
            editor.canvas_input(CanvasInput::Move {
                point: CanvasPoint {
                    x: start.x + distance,
                    y: start.y,
                },
                left_down: true,
            })?;
            let current = editor.document().unwrap().object_index();
            assert_ne!(
                current.get(&object).unwrap().properties,
                retained_properties
            );
            assert_eq!(
                previous.get(&object).unwrap().properties,
                retained_properties
            );
            assert_eq!(
                original.get(&object).unwrap().properties,
                original_properties
            );
            assert_eq!(current.objects.len(), original.objects.len());
            assert_eq!(current.relations.len(), original.relations.len());
            shown(&mut editor)?;
            previous = current;
        }
        editor.cancel_gesture();
        assert_eq!(
            editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties,
            original_properties
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}
