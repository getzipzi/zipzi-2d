use super::super::support::{self, present_canvas as shown};
use super::common::*;
use editor_core::*;
use kasane_core::{
    Vec2,
    evaluation::{OffscreenFrame, RenderCommand},
};
use std::sync::Arc;

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
    let point = CanvasPoint { x: 256., y: 256. };
    let selection = editor.document().unwrap().context().selections.clone();
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    assert_eq!(editor.document().unwrap().context().selections, selection);
    assert!(editor.overlay().marquee.is_none());
    // A small release displacement is still a click.
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint { x: 258., y: 256. },
    })?;
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
fn surface_press_cancellation_preserves_selection_and_components() -> anyhow::Result<()> {
    for lost_button in [false, true] {
        let (mut editor, _dir) = setup()?;
        let vertex = editor.overlay().vertices[0].position;
        click(&mut editor, vertex, false)?;
        let selection = editor.document().unwrap().context().selections.clone();
        let components = editor.document().unwrap().selected_vertex_positions();
        assert!(!components.is_empty());
        let point = CanvasPoint { x: 256., y: 256. };
        editor.canvas_input(CanvasInput::Down {
            point,
            shift: false,
        })?;
        if lost_button {
            editor.canvas_input(CanvasInput::Move {
                point,
                left_down: false,
            })?;
        } else {
            editor.canvas_input(CanvasInput::Cancel)?;
        }
        editor.canvas_input(CanvasInput::Up { point })?;
        assert_eq!(editor.document().unwrap().context().selections, selection);
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            components
        );
        assert!(!editor.gesture_active());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn object_marquee_can_start_on_an_unselected_surface() -> anyhow::Result<()> {
    for move_event in [false, true] {
        let (mut editor, _dir) = setup()?;
        editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
        let point = CanvasPoint { x: 256., y: 256. };
        let end = CanvasPoint { x: 505., y: 505. };
        editor.canvas_input(CanvasInput::Down {
            point,
            shift: false,
        })?;
        assert!(editor.document().unwrap().context().selections.is_empty());
        if move_event {
            editor.canvas_input(CanvasInput::Move {
                point: end,
                left_down: true,
            })?;
            assert_eq!(editor.overlay().marquee, Some((point, end)));
            assert!(editor.document().unwrap().context().selections.is_empty());
        }
        editor.canvas_input(CanvasInput::Up { point: end })?;
        // This rectangle contains only part of each mesh, so it selects neither.
        assert!(editor.document().unwrap().context().selections.is_empty());
        assert!(editor.overlay().marquee.is_none());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        assert!(!editor.document().unwrap().has_pending_edit());
    }
    Ok(())
}

#[test]
fn surface_marquee_uses_original_component_selection_and_shift() -> anyhow::Result<()> {
    for shift in [false, true] {
        let (mut editor, _dir) = setup()?;
        let vertex = editor.overlay().vertices[0].position;
        click(&mut editor, vertex, false)?;
        let selection = editor.document().unwrap().context().selections.clone();
        let point = CanvasPoint { x: 256., y: 256. };
        editor.canvas_input(CanvasInput::Down { point, shift })?;
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint { x: 280., y: 280. },
            left_down: true,
        })?;
        assert!(editor.overlay().marquee.is_some());
        // Returning to the press position cannot turn the gesture into a click.
        editor.canvas_input(CanvasInput::Move {
            point,
            left_down: true,
        })?;
        assert_eq!(editor.overlay().marquee, Some((point, point)));
        editor.canvas_input(CanvasInput::Up { point })?;
        assert_eq!(editor.document().unwrap().context().selections, selection);
        assert_eq!(
            editor
                .overlay()
                .vertices
                .iter()
                .filter(|v| v.selected)
                .count(),
            usize::from(shift)
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}
#[test]
fn hit_test_miss_clears_objects_and_components_even_with_shift() -> anyhow::Result<()> {
    let (doc, _dir, warp, rotation, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = Editor::from_document(doc);
    editor.reset_view();
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
    let bottom = source.viewport_frame.drawables[0].id.clone();
    let top = source.viewport_frame.drawables[1].id.clone();
    let frame = Arc::make_mut(&mut source.viewport_frame);
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
    let frame = Arc::make_mut(&mut source.viewport_frame);
    frame.drawables[0].texture_asset_id = "vertical-cutout".into();
    frame.drawables[1].texture_asset_id = "solid".into();
    frame.drawables[1].masks = vec![bottom.clone()];
    let scene = CanvasScene::new(&doc, source.clone());
    assert_eq!(scene.mesh_hits(upper), [top.clone(), bottom.clone()]);
    assert!(scene.mesh_hits(lower).is_empty());
    Arc::make_mut(&mut source.viewport_frame).drawables[1].inverted_mask = true;
    let scene = CanvasScene::new(&doc, source);
    assert_eq!(scene.mesh_hits(upper), [bottom]);
    assert_eq!(scene.mesh_hits(lower), [top]);
    Ok(())
}
#[test]
fn alpha_holes_pass_through_and_mask_inversion_changes_candidates() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_two_layers()?;
    let mut source = pixel_scene(&mut doc)?;
    let bottom = source.viewport_frame.drawables[0].id.clone();
    let top = source.viewport_frame.drawables[1].id.clone();
    Arc::make_mut(&mut source.viewport_frame).drawables[1].texture_asset_id = "cutout".into();
    let scene = CanvasScene::new(&doc, source.clone());
    assert_eq!(
        scene.mesh_hits(ModelPoint { x: 70., y: 128. }),
        [bottom.clone()]
    );
    assert_eq!(
        scene.mesh_hits(ModelPoint { x: 190., y: 128. }),
        [top.clone(), bottom.clone()]
    );
    let f = Arc::make_mut(&mut source.viewport_frame);
    f.drawables[0].texture_asset_id = "cutout".into();
    f.drawables[1].texture_asset_id = "solid".into();
    f.drawables[1].masks = vec![bottom.clone()];
    assert!(
        CanvasScene::new(&doc, source.clone())
            .mesh_hits(ModelPoint { x: 70., y: 128. })
            .is_empty()
    );
    Arc::make_mut(&mut source.viewport_frame).drawables[1].inverted_mask = true;
    let scene = CanvasScene::new(&doc, source);
    assert_eq!(scene.mesh_hits(ModelPoint { x: 70., y: 128. }), [top]);
    assert_eq!(scene.mesh_hits(ModelPoint { x: 190., y: 128. }), [bottom]);
    Ok(())
}
#[test]
fn offscreen_plan_overrides_flat_rank_and_applies_group_alpha() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_two_layers()?;
    let mut source = pixel_scene(&mut doc)?;
    let bottom = source.viewport_frame.drawables[0].id.clone();
    let top = source.viewport_frame.drawables[1].id.clone();
    let f = Arc::make_mut(&mut source.viewport_frame);
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
    Arc::make_mut(&mut source.viewport_frame).offscreens[0].opacity = 0.;
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
