use super::super::support::{self, present_canvas as shown};
use super::common::*;
use editor_core::*;
use kasane_core::Vec2;
use std::sync::Arc;

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
        ViewportPoint {
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
    assert!(bent[1].y > original[0].y + 1.);
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
    let mut reopened = crate::support::editor_from_document(reopened);
    reopened.set_canvas_metrics(ViewportMetrics {
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
        alt: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: ViewportPoint {
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
        let end = ViewportPoint {
            x: handle.x + 24.,
            y: handle.y - 20.,
        };
        editor.canvas_input(CanvasInput::Down {
            point: handle,
            shift: false,
            alt: false,
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
            alt: false,
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
        let mut reopened = crate::support::editor_from_document(Document::open(&receipt.manifest)?);
        reopened.set_canvas_metrics(ViewportMetrics {
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
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
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
                mesh_a_id: (meshes[0].clone()).into(),
                mesh_b_id: (meshes[1].clone()).into(),
                intensity,
                pairs: vec![
                    GlueVertexPair {
                        vertex_a: Some(0),
                        vertex_b: Some(0),
                        weight_a: 0.5,
                        weight_b: 0.5,
                        ..Default::default()
                    },
                    GlueVertexPair {
                        vertex_a: Some(1),
                        vertex_b: Some(2),
                        weight_a: 0.5,
                        weight_b: 0.5,
                        ..Default::default()
                    },
                ],
                binding: None,
            })
        })?;
        sdk.save_project(&receipt.manifest, None)?;
        let mut editor = crate::support::editor_from_document(Document::open(&receipt.manifest)?);
        editor.set_canvas_metrics(ViewportMetrics {
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
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
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
        alt: false,
    })?;
    for distance in [10., 20., 30.] {
        editor.canvas_input(CanvasInput::Move {
            point: ViewportPoint {
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
            alt: false,
        })?;
        let mut previous = original.clone();
        for distance in [10., 20.] {
            let retained_properties = previous.get(&object).unwrap().properties.clone();
            editor.canvas_input(CanvasInput::Move {
                point: ViewportPoint {
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

#[test]
fn locked_deformer_has_only_read_only_lines_and_blocks_control_adjustments() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = shown(&mut editor)?;
    let object = ObjectRef::new(ObjectKind::Deformer, warp);
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    let point = editor.overlay().vertices[0].position;
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![object.clone()],
        locked: true,
    })?;
    let overlay = editor.overlay();
    assert!(overlay.vertices.is_empty());
    assert!(overlay.handles.is_empty());
    assert!(
        overlay
            .deformers
            .iter()
            .any(|d| d.selected && !d.lines.is_empty())
    );
    assert!(
        editor
            .pick_candidates(point, Some(frame.version))
            .iter()
            .all(|p| p.object != object)
    );
    assert!(
        editor
            .execute(EditorCommand::AdjustDeformer {
                object,
                adjustment: DeformerAdjustment {
                    translation: editor_core::ModelVector { x: 1., y: 0. },
                    angle: 0.,
                    scale: 1.
                }
            })
            .is_err()
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn locked_mask_still_clips_unlocked_objects_in_canvas_candidates() -> anyhow::Result<()> {
    let (mut doc, _dir) = support::sample_document_with_two_layers()?;
    let mut source = pixel_scene(&mut doc)?;
    let bottom = source.viewport_frame.drawables[0].id.clone();
    let top = source.viewport_frame.drawables[1].id.clone();
    let f = Arc::make_mut(&mut source.viewport_frame);
    f.drawables[0].texture_asset_id = "cutout".into();
    f.drawables[1].masks = vec![bottom.clone()];
    let scene = CanvasScene::new(&doc, source);
    doc.set_objects_locked(&[ObjectRef::new(ObjectKind::Mesh, bottom)], true)?;
    let t = ViewportTransform::new(
        ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        },
        ViewState::default(),
    )
    .unwrap();
    let candidates = |x| {
        scene.candidates_with_locks(
            t.to_viewport(ModelPoint { x, y: 128. }),
            t,
            &[],
            ToolId::Select,
            DeformerVisibility::default(),
            &doc.object_state_index(),
        )
    };
    assert!(candidates(70.).is_empty());
    assert_eq!(
        candidates(190.)
            .iter()
            .map(|p| p.object.id.clone())
            .collect::<Vec<_>>(),
        vec![top]
    );
    Ok(())
}
