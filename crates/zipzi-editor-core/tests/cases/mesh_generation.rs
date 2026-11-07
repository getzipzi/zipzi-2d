use super::support;
use zipzi_core::{DeltaKeyforms, Vec2};
use zipzi_editor_core::*;

fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
fn mesh() -> ObjectRef {
    ObjectRef::new(ObjectKind::Mesh, id(902))
}
fn saved_mesh_has_meshing(manifest: &std::path::Path) -> anyhow::Result<bool> {
    let session = crate::support::open_fixture(manifest)?;
    Ok(session
        .snapshot()
        .document()
        .mesh(&mesh().id)
        .unwrap()
        .meshing
        .is_some())
}
fn generate(editor: &mut Editor, columns: usize, rows: usize) -> anyhow::Result<()> {
    let expected_revision = editor.document().unwrap().mesh_generation_revision();
    editor.execute(EditorCommand::GenerateMeshGrid {
        object: mesh(),
        options: MeshGridOptions { columns, rows },
        expected_revision,
    })
}
fn present(editor: &mut Editor, vertices: usize, triangles: usize) -> anyhow::Result<()> {
    let frame = editor.preview()?.canvas.unwrap();
    let geometry = frame
        .geometry
        .meshes
        .iter()
        .find(|m| m.id == mesh().id)
        .unwrap();
    assert_eq!(geometry.vertices.len(), vertices);
    assert_eq!(geometry.triangles.len(), triangles);
    assert!(editor.present_frame(frame));
    Ok(())
}

#[test]
fn manual_mesh_stages_preview_apply_and_reject_stale_options() -> anyhow::Result<()> {
    let (mut doc, dir) = support::alpha_document()?;
    let target = doc.manual_mesh_target()?;
    let source = doc.mesh_generation_source_manual(&target)?;
    let options = MeshSettings {
        edge_spacing: 8.,
        interior_spacing: 8.,
        outer_margin: 0.,
        alpha_threshold: 64,
        clip_to_image: true,
        ..MeshSettings::default()
    };
    let stale = source.generate(MeshGenerationOptions::ManualEdge(options))?;
    assert!(stale.geometry().meshing.is_some());
    source.invalidate_previews();
    assert!(doc.apply_mesh_generation(&stale).is_err());
    assert_eq!(doc.history_lengths(), (0, 0));

    let edge = source.generate(MeshGenerationOptions::ManualEdge(options))?;
    let edge_vertex_count = edge.geometry().vertex_ids.len();
    let edge_meshing = edge.geometry().meshing.clone().unwrap();
    let staged_fill = source
        .with_geometry(edge.geometry().clone())
        .generate(MeshGenerationOptions::ManualInterior(options))?;
    assert!(
        staged_fill
            .geometry()
            .vertex_ids
            .starts_with(&edge.geometry().vertex_ids),
        "Interior stage retains the contour preview's allocated identities"
    );
    doc.apply_mesh_generation(&edge)?;
    assert_eq!(doc.history_lengths(), (1, 0));
    let target = doc.manual_mesh_target()?;
    let triangle = edge.geometry().triangles[0];
    let uv = triangle.map(|id| {
        let slot = edge
            .geometry()
            .vertex_ids
            .iter()
            .position(|&candidate| candidate == id)
            .unwrap();
        edge.geometry().uvs[slot]
    });
    let uv = Vec2::new(
        (uv[0].x + uv[1].x + uv[2].x) / 3.,
        (uv[0].y + uv[1].y + uv[2].y) / 3.,
    );
    let inserted_id =
        zipzi_sdk::ManualVertexIdAllocator::new(edge.geometry()).allocate(edge.geometry())?;
    assert!(doc.apply_manual_mesh_once(
        &target,
        zipzi_sdk::ManualMeshOperation::InsertVertex {
            id: inserted_id,
            uv,
        }
    )?);
    assert_eq!(doc.history_lengths(), (2, 0));
    let target = doc.manual_mesh_target()?;
    let source = doc.mesh_generation_source_manual(&target)?;
    let fill = source.generate(MeshGenerationOptions::ManualInterior(options))?;
    assert!(fill.geometry().vertex_ids.len() >= edge_vertex_count);
    assert_eq!(fill.geometry().meshing.as_ref(), Some(&edge_meshing));
    for id in &edge.geometry().vertex_ids {
        assert!(fill.geometry().vertex_ids.contains(id));
    }
    assert!(fill.geometry().vertex_ids.contains(&inserted_id));
    doc.apply_mesh_generation(&fill)?;
    assert_eq!(doc.history_lengths(), (3, 0));
    let target = doc.manual_mesh_target()?;
    let source = doc.mesh_generation_source_manual(&target)?;
    let reconnect = source.generate(MeshGenerationOptions::ManualReconnect)?;
    let reconnect_changed = reconnect.geometry() != fill.geometry();
    doc.apply_mesh_generation(&reconnect)?;
    let expected_history = 3 + usize::from(reconnect_changed);
    assert_eq!(doc.history_lengths(), (expected_history, 0));
    let target = doc.manual_mesh_target()?;
    let source = doc.mesh_generation_source_manual(&target)?;
    let full = source.generate(MeshGenerationOptions::ManualFull(options))?;
    assert!(full.geometry().meshing.is_some());
    doc.apply_mesh_generation(&full)?;
    assert_eq!(doc.history_lengths(), (expected_history + 1, 0));
    let saved = doc.save(&dir.path().join("manual-stages.zipzi"))?;
    assert!(saved_mesh_has_meshing(&saved.path)?);
    Ok(())
}

#[test]
fn manual_mesh_mode_keeps_model_display_and_one_history_step_per_point() -> anyhow::Result<()> {
    let (doc, dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let id = editor.active_id().unwrap();
    editor.enter_manual_mesh(id)?;
    assert!(editor.manual_active());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    let preview = editor.preview()?;
    assert!(
        preview
            .scene
            .viewport_frame
            .drawables
            .iter()
            .all(|d| !d.id.starts_with("__manual"))
    );
    assert_eq!(
        preview.scene.viewport_frame.drawables,
        preview.scene.runtime_frame.drawables
    );
    let frame = preview.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    let binding = editor.primary_viewport_binding().unwrap();
    let overlay = editor.overlay_for_viewport(binding);
    assert!(!overlay.automatic_lines.is_empty());
    editor.dispatch(editor.command(CommandId::ManualCreate))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let point = frame.transform.to_viewport(ModelPoint { x: 128., y: 128. });
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(editor.manual_active());
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    assert!(editor.manual_active());
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let task = editor.prepare_save(id, &dir.path().join("manual-editor.zipzi"), false)?;
    let saved = editor.finish_save(&mut task.run())?;
    assert!(saved_mesh_has_meshing(&saved.path)?);
    Ok(())
}

#[test]
fn manual_mesh_drag_keeps_last_valid_candidate_and_undo_cancels_gesture() -> anyhow::Result<()> {
    let (doc, _dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let geometry = frame
        .geometry
        .meshes
        .iter()
        .find(|m| m.id == mesh().id)
        .unwrap();
    let &(vertex_id, original) = geometry
        .vertices
        .iter()
        .min_by(|a, b| (a.1.x + a.1.y).total_cmp(&(b.1.x + b.1.y)))
        .unwrap();
    let start = frame.transform.to_viewport(original);
    let valid = ViewportPoint {
        x: start.x + 12.,
        y: start.y - 12.,
    };
    let invalid = ViewportPoint {
        x: start.x - 1000.,
        y: start.y + 1000.,
    };
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: valid,
            left_down: true,
        },
    )?;
    assert!(editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    let accepted = editor
        .document()
        .unwrap()
        .selected_vertex_positions()
        .into_iter()
        .find(|(id, _)| *id == vertex_id)
        .unwrap()
        .1;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: invalid,
            left_down: true,
        },
    )?;
    let retained = editor
        .document()
        .unwrap()
        .selected_vertex_positions()
        .into_iter()
        .find(|(id, _)| *id == vertex_id)
        .unwrap()
        .1;
    assert_eq!(retained, accepted);
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: invalid })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(!editor.document().unwrap().has_pending_edit());

    let frame = crate::support::present_canvas(&mut editor)?;
    let geometry = frame
        .geometry
        .meshes
        .iter()
        .find(|m| m.id == mesh().id)
        .unwrap();
    let position = geometry
        .vertices
        .iter()
        .find(|(id, _)| *id == vertex_id)
        .unwrap()
        .1;
    let start = frame.transform.to_viewport(position);
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: ViewportPoint {
                x: start.x + 6.,
                y: start.y - 6.,
            },
            left_down: true,
        },
    )?;
    assert!(editor.document().unwrap().has_pending_edit());
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(!editor.document().unwrap().has_pending_edit());
    editor.dispatch(editor.command(CommandId::CancelGesture))?;
    assert!(!editor.manual_active());
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn manual_mesh_edge_selection_pins_and_releases_an_automatic_connection() -> anyhow::Result<()> {
    let (doc, _dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    crate::support::present_canvas(&mut editor)?;
    let binding = editor.primary_viewport_binding().unwrap();
    let lines = editor.overlay_for_viewport(binding).automatic_lines;
    assert_eq!(lines.len(), 1);
    let (a, b) = lines[0];
    editor.dispatch(editor.command(CommandId::ManualCreate))?;
    crate::support::present_canvas(&mut editor)?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: a,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: b })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    crate::support::present_canvas(&mut editor)?;
    assert!(
        editor
            .overlay_for_viewport(binding)
            .automatic_lines
            .is_empty()
    );
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    crate::support::present_canvas(&mut editor)?;
    let midpoint = ViewportPoint {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    };
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: midpoint,
            shift: true,
            alt: false,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: midpoint })?;
    editor.dispatch(editor.command(CommandId::ManualRemoveSelection))?;
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    crate::support::present_canvas(&mut editor)?;
    assert_eq!(
        editor.overlay_for_viewport(binding).automatic_lines.len(),
        1
    );
    Ok(())
}

#[test]
fn manual_mesh_saving_drag_commits_the_last_valid_candidate() -> anyhow::Result<()> {
    let (doc, dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let document_id = editor.active_id().unwrap();
    editor.enter_manual_mesh(document_id)?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let geometry = frame
        .geometry
        .meshes
        .iter()
        .find(|m| m.id == mesh().id)
        .unwrap();
    let &(vertex_id, original) = geometry
        .vertices
        .iter()
        .min_by(|a, b| (a.1.x + a.1.y).total_cmp(&(b.1.x + b.1.y)))
        .unwrap();
    let start = frame.transform.to_viewport(original);
    let moved = ViewportPoint {
        x: start.x + 12.,
        y: start.y - 12.,
    };
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: moved,
            left_down: true,
        },
    )?;
    assert!(editor.document().unwrap().has_pending_edit());
    let save = editor.prepare_save(
        document_id,
        &dir.path().join("manual-during-drag.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut save.run())?;
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let session = crate::support::open_fixture(&saved.path)?;
    let saved_mesh = session.snapshot().document().mesh(&mesh().id).unwrap();
    let slot = saved_mesh
        .vertex_ids
        .iter()
        .position(|&id| id == vertex_id)
        .unwrap();
    assert_ne!(
        saved_mesh.base_positions[slot],
        Vec2::new(original.x, original.y)
    );
    Ok(())
}

#[test]
fn generation_rebuilds_canvas_and_survives_history_and_save() -> anyhow::Result<()> {
    let (doc, dir) = support::rectangle_document(false)?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    present(&mut editor, 4, 2)?;
    assert!(editor.command_available(editor.command(CommandId::GenerateMesh)));
    generate(&mut editor, 4, 3)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(editor.document().unwrap().modified());
    assert_eq!(
        editor.document().unwrap().context().selections,
        vec![mesh()]
    );
    present(&mut editor, 20, 24)?;
    assert!(editor.command_available(editor.command(CommandId::GenerateMesh)));
    editor.execute(EditorCommand::Undo)?;
    present(&mut editor, 4, 2)?;
    assert!(editor.command_available(editor.command(CommandId::GenerateMesh)));
    editor.execute(EditorCommand::Redo)?;
    present(&mut editor, 20, 24)?;
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("generated.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let reopened = Document::open(&saved.path)?;
    assert_eq!(reopened.layers()[0].vertices, 20);
    Ok(())
}

#[test]
fn bound_generation_migrates_keyforms_blends_and_glue_atomically() -> anyhow::Result<()> {
    let (doc, dir) = support::rectangle_document(true)?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    assert!(
        editor
            .document()
            .unwrap()
            .mesh_grid_requires_square(&mesh())
    );
    assert!(generate(&mut editor, 4, 3).is_err());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    generate(&mut editor, 4, 4)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().layers()[0].vertices, 4);
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(editor.document().unwrap().layers()[0].vertices, 25);
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("bound.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let session = crate::support::open_fixture(&saved.path)?;
    let binding = session
        .snapshot()
        .document()
        .binding_for_mesh(&id(902))
        .unwrap();
    assert!(binding.keyforms.iter().all(|f| f.positions.len() == 25));
    assert_eq!(binding.keyforms[1].positions[12], Vec2::new(138., 133.));
    let DeltaKeyforms::Mesh(forms) = &session
        .snapshot()
        .document()
        .blend_binding(&id(908))
        .unwrap()
        .keyforms
    else {
        panic!()
    };
    assert!(forms.iter().all(|f| f.positions.len() == 25));
    assert_eq!(forms[1].positions[12], Vec2::new(2., 3.));
    assert_eq!(
        session.snapshot().document().glue(&id(910)).unwrap().pairs[0].vertex_a,
        Some(0)
    );
    Ok(())
}

#[test]
fn generation_rejects_stale_locked_unsupported_and_invalid_targets() -> anyhow::Result<()> {
    let (doc, _dir) = support::rectangle_document(false)?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    let version = editor.document().unwrap().mesh_generation_revision();
    for (columns, rows) in [(0, 4), (4, 0), (256, 256), (usize::MAX, 1)] {
        assert!(generate(&mut editor, columns, rows).is_err());
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![ObjectRef::new(ObjectKind::Part, id(903))],
        locked: true,
    })?;
    assert!(!editor.command_available(editor.command(CommandId::GenerateMesh)));
    assert!(generate(&mut editor, 4, 4).is_err());
    editor.execute(EditorCommand::Undo)?;
    assert!(
        editor
            .execute(EditorCommand::GenerateMeshGrid {
                object: mesh(),
                options: MeshGridOptions::default(),
                expected_revision: version
            })
            .is_err()
    );
    let original = editor.active_id().unwrap();
    editor.new_document()?;
    assert!(
        editor
            .execute_for(
                original,
                EditorCommand::GenerateMeshGrid {
                    object: mesh(),
                    options: MeshGridOptions::default(),
                    expected_revision: version
                }
            )
            .is_err()
    );
    let (doc, _dir) = support::sample_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    assert!(editor.command_available(editor.command(CommandId::GenerateMesh)));
    let (doc, _dir) = support::sample_document_with_two_layers()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    assert!(!editor.command_available(editor.command(CommandId::GenerateMesh)));
    Ok(())
}

#[test]
fn previews_are_isolated_repeatable_and_match_application() -> anyhow::Result<()> {
    let (mut doc, dir) = support::rectangle_document(false)?;
    let revision = doc.mesh_generation_revision();
    let source = doc.mesh_generation_source(&mesh())?;
    let sparse = source.generate(MeshGenerationOptions::Grid(MeshGridOptions {
        columns: 4,
        rows: 4,
    }))?;
    let candidate = source.generate(MeshGenerationOptions::Grid(MeshGridOptions {
        columns: 8,
        rows: 3,
    }))?;
    assert_eq!(sparse.geometry().positions.len(), 25);
    assert_eq!(candidate.geometry().positions.len(), 36);
    assert_eq!(doc.layers()[0].vertices, 4);
    assert_eq!(doc.mesh_generation_revision(), revision);
    assert_eq!(doc.history_lengths(), (0, 0));
    assert!(!doc.modified());
    assert!((sparse.minimum_angle() - 45.).abs() < 0.01);
    assert!(candidate.minimum_angle() < sparse.minimum_angle());
    let repeated = source.generate(candidate.options())?;
    assert_eq!(candidate.geometry(), repeated.geometry());
    doc.apply_mesh_generation(&candidate)?;
    assert_eq!(doc.history_lengths(), (1, 0));
    let saved = doc.save(&dir.path().join("preview-applied.zipzi"))?;
    let session = crate::support::open_fixture(&saved.path)?;
    let applied = session.snapshot().document().mesh(&mesh().id).unwrap();
    assert_eq!(applied.base_positions, candidate.geometry().positions);
    assert_eq!(applied.uvs, candidate.geometry().uvs);
    assert_eq!(applied.triangles, candidate.geometry().triangles);
    assert_eq!(applied.vertex_ids, candidate.geometry().vertex_ids);
    assert_eq!(doc.history_lengths(), (1, 0));
    assert!(doc.apply_mesh_generation(&sparse).is_err());
    Ok(())
}

#[test]
fn snapshot_preview_keeps_binding_constraints_and_rejects_stale_apply() -> anyhow::Result<()> {
    let (doc, _dir) = support::rectangle_document(true)?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    let source = editor.document().unwrap().mesh_generation_source(&mesh())?;
    assert!(
        source
            .generate(MeshGenerationOptions::Grid(MeshGridOptions {
                columns: 3,
                rows: 4
            }))
            .is_err()
    );
    let candidate = source.generate(MeshGenerationOptions::Grid(MeshGridOptions {
        columns: 4,
        rows: 4,
    }))?;
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![mesh()],
        locked: true,
    })?;
    assert!(
        editor
            .execute(EditorCommand::ApplyMeshGeneration(std::sync::Arc::new(
                candidate
            )))
            .is_err()
    );
    assert_eq!(editor.document().unwrap().layers()[0].vertices, 4);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn alpha_preview_maps_source_pixels_and_survives_regeneration_history_and_save()
-> anyhow::Result<()> {
    let (doc, dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    let source = editor.document().unwrap().mesh_generation_source(&mesh())?;
    assert!(source.alpha_error().is_none());
    let options = MeshGenerationOptions::Alpha(MeshSettings {
        edge_spacing: 8.,
        interior_spacing: 8.,
        outer_margin: 0.,
        alpha_threshold: 64,
        ..MeshSettings::default()
    });
    let preview = source.generate(options)?;
    let geometry = preview.geometry().clone();
    assert!(geometry.positions.len() > 4);
    // The opaque L is in the upper left. The low-alpha island is excluded.
    for (p, uv) in geometry.positions.iter().zip(&geometry.uvs) {
        assert!((p.x - (24. + 208. * uv.x)).abs() < 0.001);
        assert!((p.y - (24. + 208. * uv.y)).abs() < 0.001);
        assert!(uv.x >= 8. / 96. - 1e-6 && uv.x <= 66. / 96. + 1e-6);
        assert!(uv.y >= 1. - 42. / 64. - 1e-6 && uv.y <= 1. - 6. / 64. + 1e-6);
    }
    for face in &geometry.triangles {
        let [a, b, c] = face.map(|id| {
            geometry.positions[geometry.vertex_ids.iter().position(|v| *v == id).unwrap()]
        });
        assert!((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x) > 0.);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.execute(EditorCommand::ApplyMeshGeneration(std::sync::Arc::new(
        preview,
    )))?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let doc = editor.document().unwrap();
    assert!(doc.mesh_generation_target().is_ok());
    let regenerated = doc.mesh_generation_source(&mesh())?.generate(options)?;
    for (a, b) in regenerated
        .geometry()
        .positions
        .iter()
        .zip(&geometry.positions)
    {
        assert!((a.x - b.x).abs() < 0.001 && (a.y - b.y).abs() < 0.001);
    }
    let slots = |g: &zipzi_sdk::MeshGeometry| {
        g.triangles
            .iter()
            .map(|t| t.map(|id| g.vertex_ids.iter().position(|v| *v == id).unwrap()))
            .collect::<Vec<_>>()
    };
    assert_eq!(slots(regenerated.geometry()), slots(&geometry));
    assert!(
        regenerated
            .geometry()
            .vertex_ids
            .iter()
            .all(|id| !geometry.vertex_ids.contains(id))
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().layers()[0].vertices, 4);
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        editor.document().unwrap().layers()[0].vertices,
        geometry.positions.len()
    );
    let save = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("alpha.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut save.run())?;
    let reopened = Document::open(&saved.path)?;
    let session = crate::support::open_fixture(&saved.path)?;
    let applied = session.snapshot().document().mesh(&mesh().id).unwrap();
    assert_eq!(applied.base_positions, geometry.positions);
    assert_eq!(applied.uvs, geometry.uvs);
    assert_eq!(applied.triangles, geometry.triangles);
    assert!(
        reopened
            .mesh_generation_source(&mesh())?
            .generate(options)
            .is_ok()
    );
    Ok(())
}

#[test]
fn alpha_threshold_is_inclusive_and_stale_plans_do_not_edit() -> anyhow::Result<()> {
    let (bound, _dir) = support::rectangle_document(true)?;
    let options = MeshGenerationOptions::Alpha(MeshSettings::default());
    assert!(
        bound
            .mesh_generation_source(&mesh())?
            .generate(options)
            .is_err()
    );
    assert_eq!(bound.history_lengths(), (0, 0));
    let (mut doc, _dir) = support::alpha_document()?;
    let source = doc.mesh_generation_source(&mesh())?;
    assert!(
        source
            .generate(MeshGenerationOptions::Alpha(MeshSettings {
                alpha_threshold: 255,
                ..MeshSettings::default()
            }))
            .is_ok()
    );
    assert_eq!(doc.history_lengths(), (0, 0));
    let preview = source.generate(options)?;
    let old = source.generate(options)?;
    doc.apply_mesh_generation(&preview)?;
    assert!(doc.apply_mesh_generation(&old).is_err());
    assert_eq!(doc.history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn alpha_edge_band_pads_texture_atomically_and_regenerates_after_save() -> anyhow::Result<()> {
    let (mut doc, dir) = support::cropped_alpha_document()?;
    let source = doc.mesh_generation_source(&mesh())?;
    let original = source.texture.clone().unwrap();
    let settings = MeshGenerationOptions::Alpha(MeshSettings {
        edge_spacing: 16.,
        interior_spacing: 16.,
        edge_mode: zipzi_editor_core::adaptive_mesh::MeshEdgeMode::Triple,
        edge_width: 8.,
        clip_to_image: false,
        ..MeshSettings::default()
    });
    let preview = source.generate(settings)?;
    let padded = preview.render_texture(original.clone());
    assert!(padded.width > original.width && padded.height > original.height);
    assert_eq!(doc.history_lengths(), (0, 0));
    assert_eq!(
        doc.mesh_generation_source(&mesh())?.texture.unwrap().width,
        original.width
    );
    assert!(
        preview
            .geometry()
            .positions
            .iter()
            .any(|p| p.x < 24. && p.y > 232.)
    );
    assert!(
        preview
            .geometry()
            .uvs
            .iter()
            .all(|uv| (0. ..=1.).contains(&uv.x) && (0. ..=1.).contains(&uv.y))
    );
    // Every new outer texel is transparent, so clamp-to-edge cannot bleed color.
    for y in 0..padded.height {
        for x in 0..padded.width {
            if x == 0 || x == padded.width - 1 || y == 0 || y == padded.height - 1 {
                assert_eq!(padded.rgba[((y * padded.width + x) * 4 + 3) as usize], 0);
            }
        }
    }
    assert_eq!(
        padded
            .rgba
            .chunks_exact(4)
            .map(|p| p[3] as u64)
            .sum::<u64>(),
        original
            .rgba
            .chunks_exact(4)
            .map(|p| p[3] as u64)
            .sum::<u64>()
    );
    doc.apply_mesh_generation(&preview)?;
    assert_eq!(doc.history_lengths(), (1, 0));
    let applied = doc.mesh_generation_source(&mesh())?.texture.unwrap();
    assert_eq!(applied.rgba, padded.rgba);
    doc.undo()?;
    assert_eq!(
        doc.mesh_generation_source(&mesh())?.texture.unwrap().rgba,
        original.rgba
    );
    doc.redo()?;
    assert_eq!(
        doc.mesh_generation_source(&mesh())?.texture.unwrap().rgba,
        padded.rgba
    );
    let saved = doc.save(&dir.path().join("outside.zipzi"))?;
    let reopened = Document::open(&saved.path)?;
    let source = reopened.mesh_generation_source(&mesh())?;
    assert_eq!(source.texture.as_ref().unwrap().rgba, padded.rgba);
    let regenerated = source.generate(settings)?;
    let again = regenerated.render_texture(source.texture.unwrap());
    assert_eq!(
        (again.width, again.height),
        (padded.width, padded.height),
        "regeneration keeps growing padding"
    );
    assert_eq!(
        regenerated.geometry().positions.len(),
        preview.geometry().positions.len()
    );
    for p in &preview.geometry().positions {
        assert!(
            regenerated
                .geometry()
                .positions
                .iter()
                .any(|q| (p.x - q.x).abs() < 0.001 && (p.y - q.y).abs() < 0.001)
        );
    }
    // Save relocates resources in history; both versions must still undo/redo.
    doc.undo()?;
    assert_eq!(
        doc.mesh_generation_source(&mesh())?.texture.unwrap().rgba,
        original.rgba
    );
    doc.redo()?;
    assert_eq!(
        doc.mesh_generation_source(&mesh())?.texture.unwrap().rgba,
        padded.rgba
    );
    Ok(())
}

fn manual_interaction_fixture() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (doc, dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    editor.dispatch(editor.command(CommandId::ManualCreate))?;
    crate::support::present_canvas(&mut editor)?;
    Ok((editor, dir))
}

fn manual_click(editor: &mut Editor, point: ViewportPoint) -> anyhow::Result<()> {
    crate::support::pointer_input(
        editor,
        CanvasInput::Down {
            primary: false,
            point,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(editor, CanvasInput::Up { point })?;
    Ok(())
}

fn manual_lasso(
    editor: &mut Editor,
    path: &[(f32, f32)],
    shift: bool,
    alt: bool,
) -> anyhow::Result<()> {
    let frame = crate::support::present_canvas(editor)?;
    let bounds = frame
        .geometry
        .mesh(&mesh().id)
        .unwrap()
        .vertices
        .iter()
        .fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |[x0, y0, x1, y1], (_, p)| [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        );
    // Express paths in the fixture's original model rectangle in both views.
    let points: Vec<_> = path
        .iter()
        .map(|&(x, y)| {
            frame.transform.to_viewport(ModelPoint {
                x: bounds[0] + (x - 24.) / 208. * (bounds[2] - bounds[0]),
                y: bounds[1] + (y - 24.) / 208. * (bounds[3] - bounds[1]),
            })
        })
        .collect();
    crate::support::pointer_input(
        editor,
        CanvasInput::Down {
            point: points[0],
            shift,
            alt,
            primary: false,
        },
    )?;
    for &point in &points[1..] {
        crate::support::pointer_input(
            editor,
            CanvasInput::Move {
                point,
                left_down: true,
            },
        )?;
    }
    crate::support::pointer_input(
        editor,
        CanvasInput::Up {
            point: *points.last().unwrap(),
        },
    )?;
    Ok(())
}

fn manual_selected(editor: &Editor) -> Vec<zipzi_core::VertexId> {
    editor
        .overlay_for_viewport(editor.primary_viewport_binding().unwrap())
        .vertices
        .into_iter()
        .filter(|v| v.selected)
        .map(|v| v.id)
        .collect()
}

#[test]
fn manual_mesh_lasso_selects_polygon_adds_subtracts_and_never_edits_geometry() -> anyhow::Result<()>
{
    for source_view in [false, true] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        let original = edited_mesh(&editor);
        if source_view {
            editor.dispatch(editor.command(CommandId::ManualSourceView))?;
        }
        editor.dispatch(editor.command(CommandId::ManualLasso))?;
        assert!(editor.manual_active());
        assert!(
            editor
                .command_state(editor.command(CommandId::ManualLasso))
                .checked
        );
        // The fourth corner is inside the bounding box but outside the triangle.
        // Both shapes drag existing vertices, so region selection starts in empty space.
        manual_lasso(
            &mut editor,
            &[(12., 12.), (270., 0.), (0., 270.), (12., 12.)],
            false,
            false,
        )?;
        assert_eq!(
            manual_selected(&editor).len(),
            3,
            "source_view={source_view}"
        );
        let frame = crate::support::present_canvas(&mut editor)?;
        let bottom_right = frame
            .geometry
            .mesh(&mesh().id)
            .unwrap()
            .vertices
            .iter()
            .max_by(|(_, a), (_, b)| (a.x + a.y).total_cmp(&(b.x + b.y)))
            .unwrap()
            .0;
        assert!(!manual_selected(&editor).contains(&bottom_right));
        manual_lasso(
            &mut editor,
            &[(220., 220.), (244., 220.), (244., 244.), (220., 244.)],
            true,
            false,
        )?;
        assert_eq!(manual_selected(&editor).len(), 4);
        manual_lasso(
            &mut editor,
            &[(12., 12.), (36., 12.), (36., 36.), (12., 36.)],
            false,
            true,
        )?;
        assert_eq!(manual_selected(&editor).len(), 3);
        manual_lasso(
            &mut editor,
            &[(220., 220.), (244., 220.), (244., 244.), (220., 244.)],
            true,
            true,
        )?;
        assert_eq!(
            manual_selected(&editor).len(),
            2,
            "Option takes precedence over Shift"
        );
        let selected = manual_selected(&editor);
        manual_lasso(
            &mut editor,
            &[(70., 100.), (100., 100.), (130., 100.)],
            false,
            false,
        )?;
        assert_eq!(
            manual_selected(&editor),
            selected,
            "A collinear path preserves selection"
        );
        manual_lasso(&mut editor, &[(232., 232.)], false, false)?;
        assert_eq!(manual_selected(&editor), vec![bottom_right]);
        manual_lasso(&mut editor, &[(232., 232.)], true, false)?;
        assert_eq!(manual_selected(&editor), vec![bottom_right]);
        manual_lasso(&mut editor, &[(232., 232.)], false, true)?;
        assert!(manual_selected(&editor).is_empty());
        assert_eq!(edited_mesh(&editor), original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        assert!(!editor.document().unwrap().has_pending_edit());
    }
    Ok(())
}

#[test]
fn manual_mesh_lasso_cancel_lost_button_and_tool_switch_preserve_selection() -> anyhow::Result<()> {
    for interruption in [0, 1, 2, 3] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        editor.dispatch(editor.command(CommandId::ManualLasso))?;
        manual_lasso(&mut editor, &[(24., 24.)], false, false)?;
        let selection = manual_selected(&editor);
        assert_eq!(selection.len(), 1);
        let original = edited_mesh(&editor);
        let frame = crate::support::present_canvas(&mut editor)?;
        let points = [(10., 10.), (246., 10.), (246., 246.), (10., 246.)]
            .map(|(x, y)| frame.transform.to_viewport(ModelPoint { x, y }));
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: points[0],
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        for &point in &points[1..] {
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point,
                    left_down: true,
                },
            )?;
        }
        let binding = editor.primary_viewport_binding().unwrap();
        assert_eq!(editor.overlay_for_viewport(binding).lasso, points);
        assert_eq!(
            manual_selected(&editor),
            selection,
            "Selection is deferred until release"
        );
        assert!(editor.current_viewport_interaction().is_some());
        match interruption {
            0 => {
                editor.dispatch(editor.command(CommandId::CancelGesture))?;
            }
            1 => {
                crate::support::pointer_input(
                    &mut editor,
                    CanvasInput::Move {
                        point: points[3],
                        left_down: false,
                    },
                )?;
            }
            2 => {
                editor.dispatch(editor.command(CommandId::ManualSelect))?;
            }
            _ => {
                editor.dispatch(editor.command(CommandId::ManualSourceView))?;
            }
        }
        assert!(editor.current_viewport_interaction().is_none());
        crate::support::present_canvas(&mut editor)?;
        assert!(editor.overlay_for_viewport(binding).lasso.is_empty());
        assert_eq!(manual_selected(&editor), selection);
        assert!(editor.manual_active());
        assert_eq!(edited_mesh(&editor), original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn manual_mesh_create_click_selects_an_existing_point() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let geometry = frame
        .geometry
        .meshes
        .iter()
        .find(|m| m.id == mesh().id)
        .unwrap();
    let (id, position) = geometry.vertices[0];
    manual_click(&mut editor, frame.transform.to_viewport(position))?;
    let overlay = editor.overlay_for_viewport(editor.primary_viewport_binding().unwrap());
    assert!(
        overlay.vertices.iter().any(|v| v.id == id && v.selected),
        "Clicking an existing point must select it in Create mode"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn manual_mesh_create_near_boundary_snaps_to_the_highlighted_edge() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let geometry = frame
        .geometry
        .meshes
        .iter()
        .find(|m| m.id == mesh().id)
        .unwrap();
    let mut corners: Vec<_> = geometry
        .vertices
        .iter()
        .map(|(_, p)| frame.transform.to_viewport(*p))
        .collect();
    corners.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let point = ViewportPoint {
        x: (corners[0].x + corners[1].x) * 0.5,
        y: corners[0].y - 3.,
    };
    manual_click(&mut editor, point)?;
    assert_eq!(
        editor.layers()[0].vertices,
        5,
        "The 5-pixel edge hit must split the edge, including clicks just outside it"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn manual_mesh_plain_edit_retriangulates_outline_and_undo_restores_geometry() -> anyhow::Result<()>
{
    let (mut editor, _dir) = manual_interaction_fixture()?;
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let original = edited_mesh(&editor);
    for point in [
        ModelPoint { x: 70., y: 80. },
        ModelPoint { x: 170., y: 60. },
    ] {
        let frame = crate::support::present_canvas(&mut editor)?;
        manual_click(&mut editor, frame.transform.to_viewport(point))?;
    }
    let created = edited_mesh(&editor);
    assert_eq!(created.vertex_ids.len(), 6);
    let frame = crate::support::present_canvas(&mut editor)?;
    let start = frame.transform.to_viewport(ModelPoint { x: 24., y: 24. });
    let to = frame.transform.to_viewport(ModelPoint { x: 105., y: 105. });
    let token = editor
        .begin_canvas_gesture(frame.version.binding, start, false, false, false)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(
        token,
        CanvasInput::Move {
            point: to,
            left_down: true,
        },
    )?;
    editor.update_canvas_gesture(token, CanvasInput::Up { point: to })?;
    let moved = edited_mesh(&editor);
    assert_ne!(
        moved.base_positions,
        created.base_positions,
        "{:?}",
        editor.canvas_notice()
    );
    assert_eq!(moved.vertex_ids, created.vertex_ids);
    assert_eq!(editor.document().unwrap().history_lengths(), (3, 0));

    let frame = crate::support::present_canvas(&mut editor)?;
    manual_click(
        &mut editor,
        frame.transform.to_viewport(ModelPoint { x: 232., y: 24. }),
    )?;
    editor.dispatch(editor.command(CommandId::ManualRemoveSelection))?;
    let deleted = edited_mesh(&editor);
    assert_eq!(deleted.vertex_ids.len(), 5, "{:?}", editor.canvas_notice());
    assert_eq!(editor.document().unwrap().history_lengths(), (4, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), moved);
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), created);
    editor.dispatch(editor.command(CommandId::Undo))?;
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), original);
    for _ in 0..4 {
        editor.dispatch(editor.command(CommandId::Redo))?;
    }
    assert_eq!(edited_mesh(&editor), deleted);
    assert!(editor.manual_active());
    Ok(())
}

#[test]
fn manual_mesh_create_drag_from_empty_space_does_not_insert_a_point() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let start = frame.transform.to_viewport(ModelPoint { x: 100., y: 100. });
    let end = ViewportPoint {
        x: start.x + 25.,
        y: start.y + 30.,
    };
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (0, 0),
        "Only a click adds a point; an empty-space drag has no connection endpoints"
    );
    Ok(())
}

#[test]
fn manual_mesh_sparse_interior_is_a_valid_noop_on_an_untouched_mesh() -> anyhow::Result<()> {
    let (mut doc, _dir) = support::alpha_document()?;
    let target = doc.manual_mesh_target()?;
    let source = doc.mesh_generation_source_manual(&target)?;
    let options = MeshSettings {
        clip_to_image: true,
        interior_spacing: 10000.,
        ..MeshSettings::default()
    };
    let preview = source.generate(MeshGenerationOptions::ManualInterior(options))?;
    assert_eq!(preview.geometry().vertex_ids.len(), 4);
    doc.apply_mesh_generation(&preview)?;
    assert_eq!(doc.history_lengths(), (0, 0));
    let dense = source.generate(MeshGenerationOptions::ManualInterior(MeshSettings {
        interior_spacing: 12.,
        ..options
    }))?;
    assert!(dense.geometry().vertex_ids.len() > 4);
    Ok(())
}

fn edited_mesh(editor: &Editor) -> zipzi_core::Mesh {
    let document = editor.document().unwrap();
    let host = document.runtime_host();
    support::published_snapshot(&host, document.runtime_session_id())
        .unwrap()
        .document()
        .mesh(&mesh().id)
        .unwrap()
        .clone()
}

#[test]
fn manual_mesh_source_padding_preserves_texels_and_allows_outline_outside_original_image()
-> anyhow::Result<()> {
    let (mut editor, dir) = manual_interaction_fixture()?;
    let original = edited_mesh(&editor);
    let texture = editor.manual_target().unwrap().texture.clone();
    let frame = crate::support::present_canvas(&mut editor)?;
    let before_pose = frame.geometry.mesh(&mesh().id).unwrap().vertices.clone();
    editor.dispatch(editor.command(CommandId::ManualPadSource))?;
    assert!(editor.manual_active());
    let padded = edited_mesh(&editor);
    let target = editor.manual_target().unwrap().clone();
    let image = &target.texture;
    assert_eq!(
        (image.width, image.height),
        (texture.width + 8, texture.height + 8)
    );
    assert_eq!(padded.vertex_ids, original.vertex_ids);
    assert_eq!(padded.base_positions, original.base_positions);
    assert_eq!(padded.triangles, original.triangles);
    assert_eq!(padded.meshing, original.meshing);
    for (before, after) in original.uvs.iter().zip(&padded.uvs) {
        assert!((after.x * image.width as f32 - 4. - before.x * texture.width as f32).abs() < 1e-4);
        assert!(
            (after.y * image.height as f32 - 4. - before.y * texture.height as f32).abs() < 1e-4
        );
    }
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let pixel = &image.rgba[(y * image.width as usize + x) * 4..][..4];
            if (4..4 + texture.width as usize).contains(&x)
                && (4..4 + texture.height as usize).contains(&y)
            {
                let source = ((y - 4) * texture.width as usize + x - 4) * 4;
                assert_eq!(pixel, &texture.rgba[source..source + 4]);
            } else {
                assert_eq!(pixel, [0; 4]);
            }
        }
    }
    let frame = crate::support::present_canvas(&mut editor)?;
    assert_eq!(
        frame.geometry.mesh(&mesh().id).unwrap().vertices,
        before_pose
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));

    // The original lower-left image corner can now move into the new transparent area.
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let from = frame.transform.to_viewport(ModelPoint { x: 24., y: 24. });
    let outside = ModelPoint {
        x: 24. - 2. * 208. / texture.width as f32,
        y: 24. - 2. * 208. / texture.height as f32,
    };
    let to = frame.transform.to_viewport(outside);
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: from,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    assert!(!editor.command_available(editor.command(CommandId::ManualPadSource)));
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: to,
            left_down: true,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: to })?;
    let extended = edited_mesh(&editor);
    assert!((extended.base_positions[0].x - outside.x).abs() < 1e-4);
    assert!((extended.base_positions[0].y - outside.y).abs() < 1e-4);
    assert!(extended.uvs[0].x * (image.width as f32) < 4.);
    assert!(extended.uvs[0].y * (image.height as f32) < 4.);
    assert!(
        extended
            .uvs
            .iter()
            .all(|uv| (0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y))
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), padded);
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), original);
    assert_eq!(editor.manual_target().unwrap().texture.rgba, texture.rgba);
    assert_eq!(editor.manual_target().unwrap().texture.width, texture.width);
    editor.dispatch(editor.command(CommandId::Redo))?;
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(edited_mesh(&editor), extended);
    assert_eq!(editor.manual_target().unwrap().texture.rgba, image.rgba);
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("padded-outline.zipzi"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let mut reopened = Document::open(&saved.path)?;
    reopened.select(&mesh().id)?;
    let reopened_target = reopened.manual_mesh_target()?;
    assert_eq!(reopened_target.texture.rgba, image.rgba);
    assert_eq!(
        support::published_snapshot(&reopened.runtime_host(), reopened.runtime_session_id())?
            .document()
            .mesh(&mesh().id)
            .unwrap()
            .uvs,
        extended.uvs
    );
    Ok(())
}

#[test]
fn manual_mesh_source_padding_retains_bound_shapes_glue_and_rejects_stale_targets()
-> anyhow::Result<()> {
    let (mut document, _dir) = mixed_dependency_document()?;
    let target = document.manual_mesh_target()?;
    let host = document.runtime_host();
    let before = support::published_snapshot(&host, document.runtime_session_id())?
        .document()
        .clone();
    let before_pose = document.snapshot()?.runtime_frame;
    assert!(!document.pad_manual_mesh_source(&target, 0)?);
    assert!(document.pad_manual_mesh_source(&target, u32::MAX).is_err());
    assert_eq!(document.history_lengths(), (0, 0));
    assert!(document.pad_manual_mesh_source(&target, 32)?);
    let after = support::published_snapshot(&host, document.runtime_session_id())?
        .document()
        .clone();
    for id in before.binding_order() {
        assert_eq!(after.binding(id), before.binding(id));
    }
    for id in before.blend_binding_order() {
        assert_eq!(after.blend_binding(id), before.blend_binding(id));
    }
    for id in before.glue_order() {
        assert_eq!(after.glue(id), before.glue(id));
    }
    let after_pose = document.snapshot()?.runtime_frame;
    for (a, b) in before_pose.drawables.iter().zip(&after_pose.drawables) {
        assert_eq!(a.positions, b.positions);
    }
    assert_eq!(document.history_lengths(), (1, 0));
    assert!(document.pad_manual_mesh_source(&target, 32).is_err());
    let next = document.manual_mesh_target()?;
    assert!(document.pad_manual_mesh_source(&next, 32)?);
    assert_eq!(
        document.manual_mesh_target()?.texture.width,
        target.texture.width + 128
    );
    document.undo()?;
    document.undo()?;
    assert_eq!(
        support::published_snapshot(&host, document.runtime_session_id())?
            .document()
            .mesh(&target.object.id),
        before.mesh(&target.object.id)
    );
    assert_eq!(
        document.manual_mesh_target()?.texture.rgba,
        target.texture.rgba
    );
    Ok(())
}

#[test]
fn manual_mesh_padded_bound_non_affine_outline_drags_beyond_original_image_in_both_views()
-> anyhow::Result<()> {
    for source_view in [false, true] {
        let (doc, _dir) = mixed_dependency_document()?;
        let mut editor = crate::support::editor_from_document(doc);
        support::attach_viewports(&mut editor);
        editor.set_canvas_metrics(ViewportMetrics {
            width: 640.,
            height: 480.,
            scale_factor: 1.,
        });
        editor.enter_manual_mesh(editor.active_id().unwrap())?;
        let original_texture = editor.manual_target().unwrap().texture.clone();
        // Repeated expansion must make the newly added space editable too.
        for _ in 0..4 {
            editor.dispatch(editor.command(CommandId::ManualPadSource))?;
        }
        if source_view {
            editor.dispatch(editor.command(CommandId::ManualSourceView))?;
        } else {
            editor.set_viewport_zoom(editor.primary_viewport_binding().unwrap(), 60., None)?;
        }
        editor.dispatch(editor.command(CommandId::ManualSelect))?;
        let target = editor.manual_target().unwrap().clone();
        let doc = editor.document().unwrap();
        let host = doc.runtime_host();
        let sid = doc.runtime_session_id();
        let before = support::published_snapshot(&host, sid)?.document().clone();
        let mesh_id = dependency_fixture::id(2);
        let mesh = before.mesh(&mesh_id).unwrap();
        let uv = zipzi_core::Vec2::new(
            (16. + 0.7 * original_texture.width as f32) / target.texture.width as f32,
            8. / target.texture.height as f32,
        );
        let local = zipzi_sdk::ManualMeshUvMap::new(zipzi_sdk::MeshGeometry {
            vertex_ids: mesh.vertex_ids.clone(),
            positions: mesh.base_positions.clone(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            meshing: mesh.meshing.clone(),
        })?
        .position(uv)?;
        let frame = crate::support::present_canvas(&mut editor)?;
        let vertex = frame.geometry.mesh(&mesh_id).unwrap().vertices[1].1;
        let destination = if source_view {
            let p = target.mapping.position(uv);
            ModelPoint { x: p.x, y: p.y }
        } else {
            let delta = zipzi_core::Vec2::new(
                local.x - mesh.base_positions[1].x,
                local.y - mesh.base_positions[1].y,
            );
            let (sin, cos) = 27_f32.to_radians().sin_cos();
            ModelPoint {
                x: vertex.x + 1.3 * (cos * delta.x - sin * delta.y),
                y: vertex.y + 1.3 * (sin * delta.x + cos * delta.y),
            }
        };
        let from = frame.transform.to_viewport(vertex);
        let to = frame.transform.to_viewport(destination);
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: from,
                primary: false,
                shift: false,
                alt: false,
            },
        )?;
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: to,
                left_down: true,
            },
        )?;
        crate::support::pointer_input(&mut editor, CanvasInput::Up { point: to })?;
        let after = support::published_snapshot(&host, sid)?.document().clone();
        let moved = after.mesh(&mesh_id).unwrap();
        assert!(
            (moved.uvs[1].x - uv.x).hypot(moved.uvs[1].y - uv.y) < 1e-5,
            "source view {source_view}: {:?}",
            moved.uvs[1]
        );
        assert!(moved.uvs[1].y * (target.texture.height as f32) < 16.);
        assert_eq!(editor.document().unwrap().history_lengths(), (5, 0));
        for id in before.glue_order() {
            assert_eq!(before.glue(id), after.glue(id));
        }
        editor.dispatch(editor.command(CommandId::Undo))?;
        assert_eq!(
            support::published_snapshot(&host, sid)?
                .document()
                .mesh(&mesh_id),
            before.mesh(&mesh_id)
        );
        assert_eq!(
            support::published_snapshot(&host, sid)?
                .document()
                .binding(&dependency_fixture::id(6)),
            before.binding(&dependency_fixture::id(6))
        );
        editor.dispatch(editor.command(CommandId::Redo))?;
        assert_eq!(
            support::published_snapshot(&host, sid)?
                .document()
                .mesh(&mesh_id),
            after.mesh(&mesh_id)
        );
    }
    Ok(())
}

#[test]
fn manual_mesh_isolation_keeps_reference_at_full_opacity_and_mode_active() -> anyhow::Result<()> {
    for isolate_first in [false, true] {
        let (doc, _dir) = support::alpha_document()?;
        let mut editor = crate::support::editor_from_document(doc);
        support::attach_viewports(&mut editor);
        let binding = editor.primary_viewport_binding().unwrap();
        if isolate_first {
            editor.toggle_viewport_isolation(binding)?;
        }
        editor.enter_manual_mesh(editor.active_id().unwrap())?;
        if !isolate_first {
            editor.toggle_viewport_isolation(binding)?;
        }
        editor.dispatch(editor.command(CommandId::ManualSourceView))?;
        assert!(editor.manual_active());
        for display in [IsolationDisplay::Dim, IsolationDisplay::Hide] {
            editor.set_viewport_isolation_display(binding, display)?;
            let preview = editor.preview()?;
            let frame = &preview.scene.viewport_frame;
            let reference = frame
                .drawables
                .iter()
                .find(|d| d.id.starts_with("__manual_mesh_reference_"))
                .unwrap();
            assert!(reference.visible);
            assert_eq!(reference.opacity, 1.);
            assert_eq!(
                frame.drawables.len(),
                1,
                "The UV plane contains only its source image"
            );
        }
        editor.toggle_viewport_isolation(binding)?;
        assert!(editor.manual_active());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn manual_mesh_drag_to_empty_space_creates_connected_point_with_one_undo() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let original = edited_mesh(&editor);
    let frame = crate::support::present_canvas(&mut editor)?;
    let from = frame.transform.to_viewport(ModelPoint { x: 24., y: 24. });
    let to = frame.transform.to_viewport(ModelPoint { x: 128., y: 128. });
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: from,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: to,
            left_down: true,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: to })?;
    let created = edited_mesh(&editor);
    assert_eq!(created.vertex_ids.len(), 5);
    assert_eq!(created.meshing.as_ref().unwrap().constraints.len(), 1);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), original);
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(edited_mesh(&editor), created);
    Ok(())
}

#[test]
fn manual_mesh_delete_outer_edge_then_create_outside_the_new_outline() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let original = edited_mesh(&editor);
    editor.dispatch(editor.command(CommandId::ManualDelete))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    manual_click(
        &mut editor,
        frame.transform.to_viewport(ModelPoint { x: 128., y: 24. }),
    )?;
    assert_eq!(edited_mesh(&editor).triangles.len(), 1);
    editor.dispatch(editor.command(CommandId::ManualCreate))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    manual_click(
        &mut editor,
        frame.transform.to_viewport(ModelPoint { x: 160., y: 80. }),
    )?;
    assert_eq!(edited_mesh(&editor).vertex_ids.len(), 4);
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), original);
    Ok(())
}

#[test]
fn manual_mesh_all_primary_tools_exit_without_changing_geometry_or_history() -> anyhow::Result<()> {
    for (command, tool) in [
        (CommandId::SelectTool, ToolId::Select),
        (CommandId::HandTool, ToolId::Hand),
        (CommandId::LassoTool, ToolId::Lasso),
        (CommandId::GlueTool, ToolId::Glue),
    ] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        let original = edited_mesh(&editor);
        editor.dispatch(editor.command(command))?;
        assert!(!editor.manual_active());
        assert_eq!(editor.active_tool(), tool);
        assert_eq!(edited_mesh(&editor), original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        editor.enter_manual_mesh(editor.active_id().unwrap())?;
        editor.execute(EditorCommand::SetTool(tool))?;
        assert!(!editor.manual_active());
        assert_eq!(editor.active_tool(), tool);
    }
    Ok(())
}

#[test]
fn manual_mesh_primary_gestures_create_move_delete_without_switching_tools() -> anyhow::Result<()> {
    for lasso in [false, true] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        editor.dispatch(editor.command(CommandId::ManualSelect))?;
        if lasso {
            editor.dispatch(editor.command(CommandId::ManualLasso))?;
        }
        let tool = Some(if lasso {
            ManualTool::Lasso
        } else {
            ManualTool::Select
        });
        let original = edited_mesh(&editor);
        let frame = crate::support::present_canvas(&mut editor)?;
        let point = frame.transform.to_viewport(ModelPoint { x: 128., y: 128. });
        let binding = frame.version.binding;
        let token = editor
            .begin_canvas_gesture(binding, point, false, false, false)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(token, CanvasInput::Up { point })?;
        let created = edited_mesh(&editor);
        assert_eq!(created.vertex_ids.len(), 5);
        assert_eq!(editor.manual_tool(), tool);
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));

        let frame = crate::support::present_canvas(&mut editor)?;
        let to = frame.transform.to_viewport(ModelPoint { x: 140., y: 140. });
        let token = editor
            .begin_canvas_gesture(binding, point, false, false, false)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(
            token,
            CanvasInput::Move {
                point: to,
                left_down: true,
            },
        )?;
        editor.update_canvas_gesture(token, CanvasInput::Up { point: to })?;
        let moved = edited_mesh(&editor);
        assert_ne!(moved, created);
        assert_eq!(moved.vertex_ids.len(), 5);
        assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));

        crate::support::present_canvas(&mut editor)?;
        let token = editor
            .begin_canvas_gesture(binding, to, true, false, true)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(token, CanvasInput::Up { point: to })?;
        assert_eq!(edited_mesh(&editor).vertex_ids.len(), 4);
        assert_eq!(editor.manual_tool(), tool);
        assert_eq!(editor.document().unwrap().history_lengths(), (3, 0));
        for expected in [&moved, &created, &original] {
            editor.dispatch(editor.command(CommandId::Undo))?;
            assert_eq!(&edited_mesh(&editor), expected);
        }
        for expected in [&created, &moved] {
            editor.dispatch(editor.command(CommandId::Redo))?;
            assert_eq!(&edited_mesh(&editor), expected);
        }
    }
    Ok(())
}

#[test]
fn manual_mesh_plain_split_then_primary_connect_in_a_single_gesture() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    // Plain click splits; Command is reserved for fixed connections.
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let edge = frame.transform.to_viewport(ModelPoint { x: 128., y: 24. });
    let binding = frame.version.binding;
    let token = editor
        .begin_canvas_gesture(binding, edge, false, false, false)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(token, CanvasInput::Up { point: edge })?;
    let split = edited_mesh(&editor);
    assert_eq!(split.vertex_ids.len(), 5);
    assert_eq!(split.meshing.as_ref().unwrap().boundary.len(), 5);
    assert_eq!(editor.manual_tool(), Some(ManualTool::Select));

    editor.dispatch(editor.command(CommandId::ManualDelete))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let to = frame.transform.to_viewport(ModelPoint { x: 128., y: 128. });
    let token = editor
        .begin_canvas_gesture(binding, edge, false, false, true)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(
        token,
        CanvasInput::Move {
            point: to,
            left_down: true,
        },
    )?;
    editor.update_canvas_gesture(token, CanvasInput::Up { point: to })?;
    let connected = edited_mesh(&editor);
    assert_eq!(connected.vertex_ids.len(), 6);
    assert_eq!(connected.meshing.as_ref().unwrap().constraints.len(), 1);
    assert_eq!(editor.manual_tool(), Some(ManualTool::Delete));
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), split);
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(edited_mesh(&editor), connected);
    Ok(())
}

#[test]
fn manual_mesh_primary_delete_cancels_on_drag_escape_or_lost_button() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let original = edited_mesh(&editor);
    for ending in 0..3 {
        let frame = crate::support::present_canvas(&mut editor)?;
        let point = frame.transform.to_viewport(ModelPoint { x: 24., y: 24. });
        let away = ViewportPoint {
            x: point.x + 20.,
            y: point.y + 20.,
        };
        let token = editor
            .begin_canvas_gesture(frame.version.binding, point, true, false, true)?
            .capture
            .unwrap();
        match ending {
            0 => {
                editor.update_canvas_gesture(
                    token,
                    CanvasInput::Move {
                        point: away,
                        left_down: true,
                    },
                )?;
                editor.update_canvas_gesture(token, CanvasInput::Up { point })?;
            }
            1 => {
                editor.update_canvas_gesture(token, CanvasInput::Cancel)?;
            }
            _ => {
                editor.update_canvas_gesture(
                    token,
                    CanvasInput::Move {
                        point,
                        left_down: false,
                    },
                )?;
            }
        }
        assert_eq!(edited_mesh(&editor), original);
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(editor.manual_tool(), Some(ManualTool::Create));
        assert!(editor.current_viewport_interaction().is_none());
    }
    Ok(())
}

#[test]
fn manual_mesh_cmd_click_fixes_only_automatic_edges_with_one_undo() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let original = edited_mesh(&editor);
    let frame = crate::support::present_canvas(&mut editor)?;
    let binding = frame.version.binding;
    let (a, b) = editor.overlay_for_viewport(binding).automatic_lines[0];
    let point = ViewportPoint {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    };
    for cancel in [true, false] {
        let token = editor
            .begin_canvas_gesture(binding, point, false, false, true)?
            .capture
            .unwrap();
        assert_eq!(
            editor.overlay_for_viewport(binding).manual_mesh_feedback,
            Some(zipzi_editor_core::ManualMeshFeedback::FixEdge)
        );
        assert_eq!(edited_mesh(&editor), original);
        if cancel {
            editor.update_canvas_gesture(
                token,
                CanvasInput::Move {
                    point: ViewportPoint {
                        x: point.x + 20.,
                        y: point.y,
                    },
                    left_down: true,
                },
            )?;
            editor.update_canvas_gesture(token, CanvasInput::Up { point })?;
            assert_eq!(edited_mesh(&editor), original);
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        } else {
            editor.update_canvas_gesture(token, CanvasInput::Up { point })?;
        }
    }
    let fixed = edited_mesh(&editor);
    assert_eq!(fixed.vertex_ids, original.vertex_ids);
    assert_eq!(fixed.triangles, original.triangles);
    assert_eq!(fixed.meshing.as_ref().unwrap().constraints.len(), 1);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    crate::support::present_canvas(&mut editor)?;
    // Clicking a fixed edge or empty space with Command does not add points.
    for point in [
        point,
        frame.transform.to_viewport(ModelPoint { x: 80., y: 128. }),
    ] {
        let token = editor
            .begin_canvas_gesture(binding, point, false, false, true)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(token, CanvasInput::Up { point })?;
        assert_eq!(edited_mesh(&editor), fixed);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(edited_mesh(&editor), original);
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(edited_mesh(&editor), fixed);
    Ok(())
}

#[test]
fn manual_mesh_plain_shift_and_option_selection_still_work() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let binding = frame.version.binding;
    let a = frame.transform.to_viewport(ModelPoint { x: 24., y: 24. });
    let b = frame.transform.to_viewport(ModelPoint { x: 232., y: 24. });
    for (point, shift, alt, count) in [
        (a, false, false, 1),
        (b, true, false, 2),
        (a, false, true, 1),
    ] {
        let token = editor
            .begin_canvas_gesture(binding, point, shift, alt, false)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(token, CanvasInput::Up { point })?;
        let overlay = editor.overlay_for_viewport(binding);
        assert_eq!(
            overlay.vertices.iter().filter(|v| v.selected).count(),
            count
        );
    }
    let start = frame.transform.to_viewport(ModelPoint { x: 10., y: 10. });
    let end = frame.transform.to_viewport(ModelPoint { x: 240., y: 240. });
    let token = editor
        .begin_canvas_gesture(binding, start, false, false, false)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(
        token,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    editor.update_canvas_gesture(token, CanvasInput::Up { point: end })?;
    assert_eq!(
        editor
            .overlay_for_viewport(binding)
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
fn manual_mesh_other_intents_exit_without_blocking_selection_or_properties() -> anyhow::Result<()> {
    for command in [
        EditorCommand::Select(mesh().id),
        EditorCommand::SelectObject(ObjectRef::new(ObjectKind::Part, id(903))),
        EditorCommand::SelectObjects(vec![mesh(), ObjectRef::new(ObjectKind::Part, id(903))]),
        EditorCommand::SelectCanvasObjects(vec![]),
        EditorCommand::Navigate(ObjectRef::new(ObjectKind::Part, id(903))),
        EditorCommand::CreateParameter(NewParameter::default()),
        EditorCommand::SetObjectProperty {
            object: mesh(),
            field: "name".into(),
            value: serde_json::json!("Renamed"),
        },
        EditorCommand::PreviewName("Renamed".into()),
        EditorCommand::SetObjectsLocked {
            objects: vec![mesh()],
            locked: true,
        },
        EditorCommand::SetTool(ToolId::Lasso),
        EditorCommand::SetTool(ToolId::Glue),
    ] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        editor.execute(command)?;
        assert!(!editor.manual_active());
        assert!(editor.manual_target().is_none());
        editor.execute(EditorCommand::FinishEdit)?;
        assert!(
            editor
                .preview()?
                .scene
                .viewport_frame
                .drawables
                .iter()
                .all(|drawable| !drawable.id.starts_with("__manual_mesh_reference_"))
        );
    }
    let (mut editor, _dir) = manual_interaction_fixture()?;
    assert!(editor.command_available(editor.command(CommandId::Deselect)));
    editor.dispatch(editor.command(CommandId::Deselect))?;
    assert!(!editor.manual_active());
    assert!(editor.document().unwrap().context().selections.is_empty());
    Ok(())
}

#[test]
fn manual_mesh_interruption_keeps_completed_edits_cancels_drag_and_retires_capture()
-> anyhow::Result<()> {
    for authoring in [false, true] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        let frame = support::present_canvas(&mut editor)?;
        manual_click(
            &mut editor,
            frame.transform.to_viewport(ModelPoint { x: 128., y: 128. }),
        )?;
        let completed = edited_mesh(&editor);
        editor.dispatch(editor.command(CommandId::ManualSelect))?;
        let frame = support::present_canvas(&mut editor)?;
        let start = frame.transform.to_viewport(ModelPoint { x: 24., y: 24. });
        let end = frame.transform.to_viewport(ModelPoint { x: 32., y: 32. });
        let token = editor
            .begin_canvas_gesture(frame.version.binding, start, false, false, false)?
            .capture
            .unwrap();
        editor.update_canvas_gesture(
            token,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        assert!(editor.document().unwrap().has_pending_edit());
        if authoring {
            let plan = editor.document().unwrap().plan_parameter_object_creation(
                ObjectKind::Parameter,
                "Created".into(),
                &[],
            )?;
            editor.execute_organization_authoring(editor.active_id().unwrap(), &plan)?;
            assert_eq!(editor.document().unwrap().parameters().len(), 1);
        } else {
            let other = ObjectRef::new(ObjectKind::Part, id(903));
            editor.execute(EditorCommand::SelectObject(other.clone()))?;
            assert_eq!(editor.document().unwrap().context().selection, Some(other));
        }
        assert!(!editor.manual_active());
        assert!(!editor.document().unwrap().has_pending_edit());
        assert!(editor.current_viewport_interaction().is_none());
        assert!(!editor.viewport_interaction_active());
        assert!(!editor.viewport_interaction_valid(token));
        assert!(
            editor
                .update_canvas_gesture(token, CanvasInput::Up { point: end })
                .is_err()
        );
        assert_eq!(edited_mesh(&editor), completed);
        if authoring {
            editor.execute(EditorCommand::Undo)?;
        }
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(edited_mesh(&editor).vertex_ids.len(), 4);
        assert!(!editor.manual_active());
    }
    Ok(())
}

#[test]
fn manual_mesh_inspector_preview_exits_before_acquiring_its_candidate() -> anyhow::Result<()> {
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let object = mesh();
    let field = property::mesh::OPACITY.key;
    let previous = editor.document().unwrap().property_values(&object)?[&field].clone();
    let patch = PropertyPatch {
        scope: editor.document().unwrap().property_scope(&object),
        expected: [(field, previous)].into(),
        changes: [(field, PropertyValue::Float(0.5))].into(),
    };
    let token = editor.begin_property_edit(
        editor.editing_context_token().unwrap(),
        object.clone(),
        &patch,
    )?;
    assert!(!editor.manual_active());
    editor.update_property_edit(&token, &patch)?;
    assert!(editor.property_edit_owned(&token));
    editor.finish_property_edit(&token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert_eq!(
        editor.document().unwrap().property_values(&object)?[&field],
        patch.changes[&field]
    );
    Ok(())
}

#[test]
fn manual_mesh_authoring_creation_and_deletion_exit_cleanly() -> anyhow::Result<()> {
    for kind in [ObjectKind::Parameter, ObjectKind::ParameterGroup] {
        let (mut editor, _dir) = manual_interaction_fixture()?;
        let plan = editor.document().unwrap().plan_parameter_object_creation(
            kind,
            "Created".into(),
            &[],
        )?;
        let result = editor.execute_organization_authoring(editor.active_id().unwrap(), &plan)?;
        assert!(!editor.manual_active());
        let created = result
            .suggested_selection
            .iter()
            .find(|object| object.kind == kind)
            .unwrap()
            .clone();
        editor.inspect_authoring_object(editor.active_id().unwrap(), created.clone())?;
        assert_eq!(
            editor.document().unwrap().context().inspected,
            Some(created)
        );
    }
    let (mut editor, _dir) = manual_interaction_fixture()?;
    let plan = editor
        .document()
        .unwrap()
        .plan_scene_authoring(zipzi_sdk::SceneEdit::Delete {
            ids: vec![mesh().id],
            scope: zipzi_sdk::SceneCopyScope::Selected,
        })?;
    editor.execute_scene_authoring(editor.active_id().unwrap(), &plan)?;
    assert!(!editor.manual_active());
    editor.execute(EditorCommand::Undo)?;
    assert!(!editor.manual_active());
    editor.execute(EditorCommand::SelectObject(mesh()))?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    assert!(editor.manual_editable());
    Ok(())
}

#[test]
fn viewport_generation_candidate_is_disposable_and_applies_once() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::alpha_document()?;
    let source = doc.mesh_generation_source(&mesh())?;
    let plan = source.generate(MeshGenerationOptions::ManualEdge(MeshSettings {
        clip_to_image: true,
        ..Default::default()
    }))?;
    let n = plan.geometry().vertex_ids.len();
    assert!(n > 4);
    doc.preview_mesh_generation(&plan)?;
    assert!(source.is_current(&doc));
    assert_eq!(doc.history_lengths(), (0, 0));
    let snapshot = doc.snapshot()?;
    let drawable = snapshot
        .viewport_frame
        .drawables
        .iter()
        .find(|d| d.id == mesh().id)
        .unwrap();
    assert_eq!(drawable.positions.len(), n);
    assert!(doc.clear_mesh_generation_preview());
    assert_eq!(doc.layers()[0].vertices, 4);
    doc.preview_mesh_generation(&plan)?;
    doc.finish_edit()?;
    assert_eq!(
        doc.layers()[0].vertices,
        4,
        "save/finish must not silently apply generation"
    );
    assert_eq!(doc.history_lengths(), (0, 0));
    doc.preview_mesh_generation(&plan)?;
    doc.apply_mesh_generation(&plan)?;
    assert_eq!(doc.layers()[0].vertices, n);
    assert_eq!(doc.history_lengths(), (1, 0));
    Ok(())
}

#[path = "../../../../fixtures/mesh-edit-dependencies.rs"]
mod dependency_fixture;
fn mixed_dependency_document() -> anyhow::Result<(Document, tempfile::TempDir)> {
    support::mixed_document()
}

#[test]
fn manual_mesh_uv_plane_keeps_non_square_pixels_and_restores_model_camera() -> anyhow::Result<()> {
    let (doc, _dir) = mixed_dependency_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    let binding = editor.primary_viewport_binding().unwrap();
    editor.set_viewport_zoom(binding, 2., None)?;
    editor.pan_viewport(binding, ViewportPoint { x: 17., y: 29. })?;
    let original = editor.viewport_view(binding).unwrap();
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let model_preview = editor.preview()?;
    assert_eq!(editor.viewport_view(binding).unwrap(), original);
    assert!(!editor.viewport_isolated(binding));
    assert_eq!(
        model_preview.scene.viewport_frame.drawables,
        model_preview.scene.runtime_frame.drawables
    );
    assert!(editor.present_frame(model_preview.canvas.unwrap()));
    editor.dispatch(editor.command(CommandId::ManualSourceView))?;
    let preview = editor.preview()?;
    let t = editor.manual_target().unwrap();
    let frame = preview.canvas.unwrap();
    let g = frame.geometry.mesh(&dependency_fixture::id(2)).unwrap();
    assert_eq!(
        g.vertices[0].1,
        ModelPoint {
            x: 0.1 * t.texture.width as f32,
            y: 0.1 * t.texture.height as f32
        }
    );
    assert_ne!(t.texture.width, t.texture.height);
    assert_eq!(preview.scene.viewport_frame.drawables.len(), 1);
    assert_eq!(
        preview.scene.viewport_frame.drawables[0].uvs[0],
        Vec2::new(0., 0.)
    );
    assert_ne!(editor.viewport_view(binding).unwrap(), original);
    editor.dispatch(editor.command(CommandId::ManualSourceView))?;
    let result = editor.preview()?;
    assert_eq!(editor.viewport_view(binding).unwrap(), original);
    assert!(
        result
            .scene
            .viewport_frame
            .drawables
            .iter()
            .any(|d| d.id == dependency_fixture::id(4))
    );
    assert!(
        result
            .scene
            .viewport_frame
            .drawables
            .iter()
            .all(|d| !d.id.starts_with("__manual"))
    );
    editor.dispatch(editor.command(CommandId::ManualSourceView))?;
    editor.exit_manual_mesh();
    assert_eq!(editor.viewport_view(binding).unwrap(), original);
    Ok(())
}
#[test]
fn manual_mesh_mixed_generation_preserves_glue_and_rejects_rebuild() -> anyhow::Result<()> {
    let (mut doc, _dir) = mixed_dependency_document()?;
    let id = dependency_fixture::id;
    let target = doc.manual_mesh_target()?;
    let source = doc.mesh_generation_source_manual(&target)?;
    assert!(!source.manual_rebuild_available());
    let settings = MeshSettings {
        interior_spacing: 8.,
        clip_to_image: true,
        ..Default::default()
    };
    assert!(
        source
            .generate(MeshGenerationOptions::ManualFull(settings))
            .is_err()
    );
    let host = doc.runtime_host();
    let original = support::published_snapshot(&host, doc.runtime_session_id())?
        .document()
        .clone();
    let preview = source.generate(MeshGenerationOptions::ManualInterior(settings))?;
    assert!(
        preview
            .geometry()
            .vertex_ids
            .starts_with(&original.mesh(&id(2)).unwrap().vertex_ids)
    );
    doc.preview_mesh_generation(&preview)?;
    doc.finish_edit()?; // saving/settling must not apply a generation preview
    assert_eq!(doc.history_lengths(), (0, 0));
    doc.clear_mesh_generation_preview();
    doc.apply_mesh_generation(&preview)?;
    let model = support::published_snapshot(&host, doc.runtime_session_id())?
        .document()
        .clone();
    assert_eq!(original.glue(&id(30)), model.glue(&id(30)));
    assert_eq!(original.glue(&id(32)), model.glue(&id(32)));
    assert_eq!(
        original.binding(&id(6)).unwrap().axes,
        model.binding(&id(6)).unwrap().axes
    );
    assert_eq!(
        model.binding(&id(6)).unwrap().keyforms[0].positions.len(),
        model.mesh(&id(2)).unwrap().vertex_ids.len()
    );
    doc.undo()?;
    assert_eq!(
        support::published_snapshot(&host, doc.runtime_session_id())?
            .document()
            .mesh(&id(2)),
        original.mesh(&id(2))
    );
    Ok(())
}

#[test]
fn manual_mesh_secondary_viewport_shows_model_and_cannot_own_uv_candidate() -> anyhow::Result<()> {
    let (doc, _dir) = mixed_dependency_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    let metrics = ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    };
    editor.set_canvas_metrics(metrics);
    editor.set_viewport_metrics(other, metrics);
    let second = editor.viewport_binding(other).unwrap();
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let main = editor.preview()?;
    assert_eq!(main.scene.viewport_frame.drawables.len(), 2);
    assert!(
        main.scene
            .viewport_frame
            .drawables
            .iter()
            .all(|d| !d.id.starts_with("__manual"))
    );
    assert!(
        !editor
            .command_state(editor.command_for_viewport(
                CommandId::ManualSourceView,
                second,
                CommandSource::Toolbar
            ))
            .enabled
    );
    assert!(
        !editor
            .command_state(editor.command_for_viewport(
                CommandId::GenerateMesh,
                second,
                CommandSource::Toolbar
            ))
            .enabled
    );
    let context = editor.preview_for(second)?;
    assert!(
        context
            .scene
            .viewport_frame
            .drawables
            .iter()
            .all(|d| !d.id.starts_with("__manual"))
    );
    let frame = context.canvas.unwrap();
    let p = frame.transform.to_viewport(
        frame
            .geometry
            .mesh(&dependency_fixture::id(2))
            .unwrap()
            .vertices[0]
            .1,
    );
    assert!(editor.present_frame(frame));
    support::pointer_input_for(
        &mut editor,
        second,
        CanvasInput::Down {
            point: p,
            primary: false,
            shift: false,
            alt: false,
        },
    )?;
    support::pointer_input_for(&mut editor, second, CanvasInput::Up { point: p })?;
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
#[test]
fn manual_mesh_source_resize_refits_uv_camera_and_rejects_old_generation() -> anyhow::Result<()> {
    let (doc, _dir) = mixed_dependency_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    editor.dispatch(editor.command(CommandId::ManualSourceView))?;
    editor.preview()?;
    let before = editor
        .viewport_view(editor.primary_viewport_binding().unwrap())
        .unwrap();
    let d = editor.document().unwrap();
    let target = editor.manual_target().unwrap();
    let source = d.mesh_generation_source_manual(&target)?;
    let preview = source.generate(MeshGenerationOptions::ManualReconnect)?;
    let sid = d.runtime_session_id();
    let host = d.runtime_host();
    let client = host.write().register_client()?;
    let image = host.read().resources().prepare_rgba(
        192,
        64,
        std::sync::Arc::from(vec![255u8; 192 * 64 * 4]),
    )?;
    support::publish_host(&host, client, sid, "Replace image", |e| {
        e.replace_image(&dependency_fixture::id(40), image)
    })?;
    editor.sync_runtime()?;
    let frame = editor.preview()?;
    let target = editor.manual_target().unwrap();
    assert_eq!((target.texture.width, target.texture.height), (192, 64));
    assert_ne!(
        editor
            .viewport_view(editor.primary_viewport_binding().unwrap())
            .unwrap(),
        before
    );
    assert!(!source.is_current(editor.document().unwrap()));
    assert_eq!(frame.scene.viewport_frame.drawables[0].positions[1].x, 192.);
    assert!(
        editor
            .document()
            .unwrap()
            .mesh_generation_source_manual(&target)?
            .is_current(editor.document().unwrap())
    );
    assert!(
        editor
            .execute(EditorCommand::ApplyMeshGeneration(std::sync::Arc::new(
                preview
            )))
            .is_err()
    );
    Ok(())
}

#[test]
fn manual_mesh_alpha_rebuild_transfers_bound_shapes_and_enforces_domain() -> anyhow::Result<()> {
    let (doc, _dir) = mixed_dependency_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    let host = editor.runtime_host();
    let sid = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    support::publish_host(&host, client, sid, "Release seams", |e| {
        e.edit_scene(
            zipzi_core::document::SceneEdit::Delete {
                ids: vec![dependency_fixture::id(30), dependency_fixture::id(32)],
                scope: Default::default(),
            },
            Default::default(),
            &Default::default(),
        )
        .map(|_| ())
    })?;
    editor.sync_runtime()?;
    let object = ObjectRef::new(ObjectKind::Mesh, dependency_fixture::id(2));
    let doc = editor.document().unwrap();
    let source = doc.mesh_generation_source(&object)?;
    assert!(source.alpha_error().is_none());
    let preview = source.generate(MeshGenerationOptions::Alpha(MeshSettings {
        edge_spacing: 8.,
        interior_spacing: 8.,
        outer_margin: 0.,
        clip_to_image: true,
        alpha_threshold: 64,
        ..Default::default()
    }));
    // The artwork extends beyond the old cropped UV coverage: this operation
    // must report a transfer-domain refusal rather than discard the bindings.
    assert!(preview.is_err());
    assert!(
        support::published_snapshot(&host, sid)?
            .document()
            .binding_for_mesh(&object.id)
            .is_some()
    );
    support::publish_host(&host, client, sid, "Full image fixture coverage", |e| {
        let mut mesh = e.read_document()?.mesh(&object.id).unwrap().clone();
        mesh.uvs[..4].copy_from_slice(&[
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ]);
        e.replace_mesh(mesh)
    })?;
    editor.sync_runtime()?;
    let doc = editor.document().unwrap();
    let source = doc.mesh_generation_source(&object)?;
    let preview = source.generate(MeshGenerationOptions::Alpha(MeshSettings {
        edge_spacing: 8.,
        interior_spacing: 8.,
        outer_margin: 0.,
        clip_to_image: true,
        alpha_threshold: 64,
        ..Default::default()
    }))?;
    assert!(
        preview
            .geometry()
            .vertex_ids
            .iter()
            .all(|id| ![10, 30, 70, 90, 120].contains(id))
    );
    let count = preview.geometry().vertex_ids.len();
    editor.execute(EditorCommand::ApplyMeshGeneration(std::sync::Arc::new(
        preview,
    )))?;
    assert_eq!(
        support::published_snapshot(&host, sid)?
            .document()
            .binding_for_mesh(&object.id)
            .unwrap()
            .keyforms[1]
            .positions
            .len(),
        count
    );
    Ok(())
}

fn model_mesh_document(editor: &Editor) -> zipzi_core::state::DocumentState {
    let d = editor.document().unwrap();
    support::published_snapshot(&d.runtime_host(), d.runtime_session_id())
        .unwrap()
        .document()
        .clone()
}

fn pose_editor() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (mut doc, dir) = mixed_dependency_document()?;
    let id = dependency_fixture::id;
    doc.set_preview_parameters(&[(id(5), 0.2), (id(10), 0.02), (id(20), 0.01)])?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 960.,
        height: 720.,
        scale_factor: 1.,
    });
    let host = editor.runtime_host();
    let sid = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    support::publish_host(&host, client, sid, "Activate seam", |e| {
        let mut g = e.read_document()?.glue(&id(30)).unwrap().clone();
        g.intensity = 0.6;
        e.replace_glue(g)
    })?;
    editor.sync_runtime()?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    Ok((editor, dir))
}

#[test]
fn manual_mesh_model_edits_invert_small_rotations_at_large_origins() -> anyhow::Result<()> {
    for (angle, reflect_x, reflect_y) in [
        (0., false, false),
        (27., false, false),
        (27., true, false),
        (27., false, true),
        (27., true, true),
    ] {
        let (doc, _dir) = support::alpha_document()?;
        let mut editor = crate::support::editor_from_document(doc);
        support::attach_viewports(&mut editor);
        editor.set_canvas_metrics(ViewportMetrics {
            width: 800.,
            height: 600.,
            scale_factor: 1.,
        });
        let host = editor.runtime_host();
        let sid = editor.document().unwrap().runtime_session_id();
        let client = host.write().register_client()?;
        support::publish_host(&host, client, sid, "Small rotation", |e| {
            crate::support::insert_transform(
                e,
                zipzi_core::Transform {
                    id: id(990),
                    data: zipzi_core::TransformData::Rotation(zipzi_core::RotationTransform {
                        pose: zipzi_core::RotationPose {
                            origin: Vec2::new(10_000., 10_000.).into(),
                            angle,
                            scale: 0.1,
                            reflect_x,
                            reflect_y,
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )?;
            let mut m = e.read_document()?.mesh(&mesh().id).unwrap().clone();
            m.deformer_id = id(990);
            e.replace_mesh(m)
        })?;
        editor.sync_runtime()?;
        editor.enter_manual_mesh(editor.active_id().unwrap())?;
        let original = model_mesh_document(&editor)
            .mesh(&mesh().id)
            .unwrap()
            .clone();
        let history = editor.document().unwrap().history_lengths();
        let frame = crate::support::present_canvas(&mut editor)?;
        let vertices = &frame.geometry.mesh(&mesh().id).unwrap().vertices;
        let start = frame.transform.to_viewport(vertices[0].1);
        let target = ModelPoint {
            x: vertices[0].1.x + (vertices[2].1.x - vertices[0].1.x) * 0.2,
            y: vertices[0].1.y + (vertices[2].1.y - vertices[0].1.y) * 0.2,
        };
        let end = frame.transform.to_viewport(target);
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        assert!(
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: end,
                    left_down: true,
                }
            )?,
            "{:?}",
            editor.canvas_notice()
        );
        let frame = crate::support::present_canvas(&mut editor)?;
        let actual = frame.geometry.mesh(&mesh().id).unwrap().vertices[0].1;
        assert!(
            (actual.x - target.x).hypot(actual.y - target.y) < 0.003,
            "{actual:?} vs {target:?}"
        );
        crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        let dragged = model_mesh_document(&editor)
            .mesh(&mesh().id)
            .unwrap()
            .clone();
        assert!((dragged.uvs[0].x - 0.2).hypot(dragged.uvs[0].y - 0.2) < 1e-4);
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );

        let frame = crate::support::present_canvas(&mut editor)?;
        let vertices = &frame.geometry.mesh(&mesh().id).unwrap().vertices;
        let target = ModelPoint {
            x: vertices[0].1.x * 0.2 + vertices[1].1.x * 0.5 + vertices[2].1.x * 0.3,
            y: vertices[0].1.y * 0.2 + vertices[1].1.y * 0.5 + vertices[2].1.y * 0.3,
        };
        manual_click(&mut editor, frame.transform.to_viewport(target))?;
        let inserted = model_mesh_document(&editor)
            .mesh(&mesh().id)
            .unwrap()
            .clone();
        assert_eq!(inserted.vertex_ids.len(), 5, "{:?}", editor.canvas_notice());
        let expected_uv = Vec2::new(0.84, 0.34);
        assert!(
            (inserted.uvs[4].x - expected_uv.x).hypot(inserted.uvs[4].y - expected_uv.y) < 2e-4
        );
        let frame = crate::support::present_canvas(&mut editor)?;
        let actual = frame.geometry.mesh(&mesh().id).unwrap().vertices[4].1;
        assert!((actual.x - target.x).hypot(actual.y - target.y) < 0.003);
        assert!(editor.canvas_notice().is_none());
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 2, 0)
        );
        for expected in [&dragged, &original] {
            editor.execute(EditorCommand::Undo)?;
            assert_eq!(
                model_mesh_document(&editor).mesh(&mesh().id),
                Some(expected)
            );
        }
        for expected in [&dragged, &inserted] {
            editor.execute(EditorCommand::Redo)?;
            assert_eq!(
                model_mesh_document(&editor).mesh(&mesh().id),
                Some(expected)
            );
        }
    }
    Ok(())
}

#[test]
fn manual_mesh_model_drag_inverts_mixed_pose_and_glue_from_one_frozen_source() -> anyhow::Result<()>
{
    let id = dependency_fixture::id;
    let mut final_models = vec![];
    for staged in [false, true] {
        let (mut editor, _dir) = pose_editor()?;
        let before = model_mesh_document(&editor);
        let history = editor.document().unwrap().history_lengths();
        let frame = crate::support::present_canvas(&mut editor)?;
        let shown = frame.geometry.mesh(&id(2)).unwrap();
        let values = frame
            .geometry
            .source
            .runtime_frame
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.value))
            .collect();
        let local =
            zipzi_core::evaluation::authoring_mesh_positions(&before, &id(2), &values).unwrap();
        let r = before.transform(&id(3)).unwrap().rotation().unwrap().pose;
        let (sin, cos) = r.angle.to_radians().sin_cos();
        let x = (local[4].x - local[0].x) * 0.15;
        let y = (local[4].y - local[0].y) * 0.15;
        let gain = 1. - before.glue(&id(30)).unwrap().intensity * 0.3;
        let delta = ModelVector {
            x: (x * cos - y * sin) * r.scale * gain,
            y: (x * sin + y * cos) * r.scale * gain,
        };
        let original = shown.vertices[0].1;
        let start = frame.transform.to_viewport(original);
        let target = ModelPoint {
            x: original.x + delta.x,
            y: original.y + delta.y,
        };
        let end = frame.transform.to_viewport(target);
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        if staged {
            let intermediate = ViewportPoint {
                x: start.x + (end.x - start.x) * 0.5,
                y: start.y + (end.y - start.y) * 0.5,
            };
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: intermediate,
                    left_down: true,
                },
            )?;
            let valid = model_mesh_document(&editor);
            crate::support::present_canvas(&mut editor)?;
            crate::support::pointer_input(
                &mut editor,
                CanvasInput::Move {
                    point: ViewportPoint {
                        x: end.x + 9000.,
                        y: end.y + 9000.,
                    },
                    left_down: true,
                },
            )?;
            assert_eq!(
                model_mesh_document(&editor).mesh(&id(2)),
                valid.mesh(&id(2))
            );
            assert!(editor.canvas_notice().is_some());
        }
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        let candidate = crate::support::present_canvas(&mut editor)?;
        let actual = candidate.geometry.mesh(&id(2)).unwrap().vertices[0].1;
        assert!(
            (actual.x - target.x).hypot(actual.y - target.y) < 0.002,
            "{actual:?} vs {target:?}; {:?}",
            editor.canvas_notice()
        );
        crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        let after = model_mesh_document(&editor);
        let m = after.mesh(&id(2)).unwrap();
        let expected = Vec2::new(0.145, 0.1525);
        assert!(
            (m.uvs[0].x - expected.x).hypot(m.uvs[0].y - expected.y) < 2e-5,
            "{:?}",
            m.uvs[0]
        );
        assert_eq!(before.glue(&id(30)), after.glue(&id(30)));
        assert_eq!(before.glue(&id(32)), after.glue(&id(32)));
        assert_eq!(before.transform(&id(3)), after.transform(&id(3)));
        for n in [12, 22] {
            let DeltaKeyforms::Mesh(a) = &before.blend_binding(&id(n)).unwrap().keyforms else {
                panic!()
            };
            let DeltaKeyforms::Mesh(b) = &after.blend_binding(&id(n)).unwrap().keyforms else {
                panic!()
            };
            for (a, b) in a.iter().zip(b) {
                assert_eq!(
                    (&a.sample_id, a.opacity, a.draw_order, a.multiply, a.screen),
                    (&b.sample_id, b.opacity, b.draw_order, b.multiply, b.screen)
                );
            }
        }
        assert_eq!(
            before.binding(&id(6)).unwrap().axes,
            after.binding(&id(6)).unwrap().axes
        );
        for (a, b) in before
            .binding(&id(6))
            .unwrap()
            .keyforms
            .iter()
            .zip(&after.binding(&id(6)).unwrap().keyforms)
        {
            assert_eq!(
                (&a.keys, &a.samples, a.appearance, a.draw_order),
                (&b.keys, &b.samples, b.appearance, b.draw_order)
            );
        }
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(
            model_mesh_document(&editor).mesh(&id(2)),
            before.mesh(&id(2))
        );
        editor.execute(EditorCommand::Redo)?;
        assert_eq!(
            model_mesh_document(&editor).mesh(&id(2)),
            after.mesh(&id(2))
        );
        final_models.push(after);
    }
    assert_eq!(final_models[0].mesh(&id(2)), final_models[1].mesh(&id(2)));
    // Each independently installed fixture owns fresh sample identities.
    for (a, b) in final_models[0]
        .binding(&id(6))
        .unwrap()
        .keyforms
        .iter()
        .zip(&final_models[1].binding(&id(6)).unwrap().keyforms)
    {
        assert_eq!(a.positions, b.positions);
    }
    for n in [12, 22] {
        let DeltaKeyforms::Mesh(a) = &final_models[0].blend_binding(&id(n)).unwrap().keyforms
        else {
            panic!()
        };
        let DeltaKeyforms::Mesh(b) = &final_models[1].blend_binding(&id(n)).unwrap().keyforms
        else {
            panic!()
        };
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.positions, b.positions);
        }
    }
    Ok(())
}

#[test]
fn manual_mesh_model_insertion_maps_displayed_pose_back_to_uv() -> anyhow::Result<()> {
    let id = dependency_fixture::id;
    let (mut editor, _dir) = pose_editor()?;
    let before = model_mesh_document(&editor);
    let frame = crate::support::present_canvas(&mut editor)?;
    let p = &frame.geometry.mesh(&id(2)).unwrap().vertices;
    // The upper triangle has no Glue endpoints; its affine parent preserves barycentrics.
    let target = ModelPoint {
        x: (p[2].1.x + p[3].1.x + p[4].1.x) / 3.,
        y: (p[2].1.y + p[3].1.y + p[4].1.y) / 3.,
    };
    manual_click(&mut editor, frame.transform.to_viewport(target))?;
    let after = model_mesh_document(&editor);
    let mesh = after.mesh(&id(2)).unwrap();
    assert_eq!(mesh.vertex_ids.len(), 6, "{:?}", editor.canvas_notice());
    let expected = Vec2::new((0.9 + 0.1 + 0.4) / 3., (0.9 + 0.9 + 0.45) / 3.);
    assert!((mesh.uvs[5].x - expected.x).hypot(mesh.uvs[5].y - expected.y) < 2e-5);
    let frame = crate::support::present_canvas(&mut editor)?;
    let actual = frame.geometry.mesh(&id(2)).unwrap().vertices[5].1;
    assert!((actual.x - target.x).hypot(actual.y - target.y) < 0.002);
    assert_eq!(frame.geometry.meshes.len(), 2);
    for n in [30, 32] {
        assert_eq!(before.glue(&id(n)), after.glue(&id(n)));
    }
    assert_eq!(
        after.binding(&id(6)).unwrap().keyforms[1].positions.len(),
        6
    );
    for n in [12, 22] {
        let DeltaKeyforms::Mesh(forms) = &after.blend_binding(&id(n)).unwrap().keyforms else {
            panic!()
        };
        assert!(forms.iter().all(|f| f.positions.len() == 6));
    }
    Ok(())
}

#[test]
fn manual_mesh_model_drag_and_insertion_invert_nonlinear_parent() -> anyhow::Result<()> {
    let (doc, _dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 800.,
        height: 600.,
        scale_factor: 1.,
    });
    let host = editor.runtime_host();
    let sid = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    support::publish_host(&host, client, sid, "Warp fixture", |e| {
        crate::support::insert_transform(
            e,
            zipzi_core::Transform {
                id: id(990),
                data: zipzi_core::TransformData::Warp(zipzi_core::WarpTransform {
                    rows: 1,
                    columns: 1,
                    quad: true,
                    bezier: None,
                    points: vec![
                        Vec2::new(0., 0.),
                        Vec2::new(200., 0.),
                        Vec2::new(0., 200.),
                        Vec2::new(260., 240.),
                    ],
                }),
                ..Default::default()
            },
        )?;
        let mut m = e.read_document()?.mesh(&mesh().id).unwrap().clone();
        m.base_positions = m.uvs.clone();
        m.deformer_id = id(990);
        e.replace_mesh(m)
    })?;
    editor.sync_runtime()?;
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let frame = crate::support::present_canvas(&mut editor)?;
    // This is the actual warped image of UV (0.3, 0.2), not the chord between corners.
    let target = ModelPoint {
        x: 60. + 60. * 0.3 * 0.2,
        y: 40. + 40. * 0.3 * 0.2,
    };
    manual_click(&mut editor, frame.transform.to_viewport(target))?;
    let m = model_mesh_document(&editor)
        .mesh(&mesh().id)
        .unwrap()
        .clone();
    assert_eq!(m.vertex_ids.len(), 5, "{:?}", editor.canvas_notice());
    assert!(
        (m.uvs[4].x - 0.3).hypot(m.uvs[4].y - 0.2) < 2e-5,
        "{:?}",
        m.uvs[4]
    );
    let frame = crate::support::present_canvas(&mut editor)?;
    let p = frame.geometry.mesh(&mesh().id).unwrap().vertices[4].1;
    let start = frame.transform.to_viewport(p);
    let destination = ModelPoint {
        x: 80. + 60. * 0.4 * 0.3,
        y: 60. + 40. * 0.4 * 0.3,
    };
    let end = frame.transform.to_viewport(destination);
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    crate::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    let frame = crate::support::present_canvas(&mut editor)?;
    let p = frame.geometry.mesh(&mesh().id).unwrap().vertices[4].1;
    assert!(
        (p.x - destination.x).hypot(p.y - destination.y) < 0.002,
        "{p:?} {:?}",
        editor.canvas_notice()
    );
    let m = model_mesh_document(&editor)
        .mesh(&mesh().id)
        .unwrap()
        .clone();
    assert!((m.uvs[4].x - 0.4).hypot(m.uvs[4].y - 0.3) < 2e-5);
    Ok(())
}

#[test]
fn manual_mesh_folded_pose_refuses_ambiguous_click_and_source_view_recovers() -> anyhow::Result<()>
{
    let (doc, _dir) = support::alpha_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    let host = editor.runtime_host();
    let sid = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    support::publish_host(&host, client, sid, "Fold fixture", |e| {
        let mut m = e.read_document()?.mesh(&mesh().id).unwrap().clone();
        m.base_positions = vec![
            Vec2::new(0., 0.),
            Vec2::new(100., 0.),
            Vec2::new(100., 100.),
            Vec2::new(100., 0.),
        ];
        e.replace_mesh(m)
    })?;
    editor.sync_runtime()?;
    let before = model_mesh_document(&editor);
    let history = editor.document().unwrap().history_lengths();
    editor.enter_manual_mesh(editor.active_id().unwrap())?;
    let frame = crate::support::present_canvas(&mut editor)?;
    manual_click(
        &mut editor,
        frame.transform.to_viewport(ModelPoint { x: 65., y: 30. }),
    )?;
    assert_eq!(
        model_mesh_document(&editor).mesh(&mesh().id),
        before.mesh(&mesh().id)
    );
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert!(editor.canvas_notice().unwrap().contains("overlap"));
    editor.dispatch(editor.command(CommandId::ManualSourceView))?;
    let frame = crate::support::present_canvas(&mut editor)?;
    manual_click(
        &mut editor,
        frame.transform.to_viewport(ModelPoint {
            x: 96. * 0.3,
            y: 64. * 0.2,
        }),
    )?;
    assert_eq!(
        model_mesh_document(&editor)
            .mesh(&mesh().id)
            .unwrap()
            .vertex_ids
            .len(),
        5
    );
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (history.0 + 1, 0)
    );
    Ok(())
}

#[test]
fn manual_mesh_drag_reuses_panel_records_and_unchanged_connectivity() -> anyhow::Result<()> {
    use std::sync::Arc;
    let id = dependency_fixture::id;
    let (mut editor, _dir) = pose_editor()?;
    let original = model_mesh_document(&editor);
    let index = editor.document().unwrap().object_index();
    let object = ObjectRef::new(ObjectKind::Mesh, id(2));
    let peer = ObjectRef::new(ObjectKind::Mesh, id(4));
    let mut frame = crate::support::present_canvas(&mut editor)?;
    let shown = frame.geometry.mesh(&id(2)).unwrap();
    let start = frame.transform.to_viewport(shown.vertices[4].1);
    let towards = frame.transform.to_viewport(shown.vertices[1].1);
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    let mut retained_connectivity = 0;
    for sample in 0..8 {
        let amount = 0.3 + sample as f32 * 0.02;
        let point = ViewportPoint {
            x: start.x + (towards.x - start.x) * amount,
            y: start.y + (towards.y - start.y) * amount,
        };
        let before_uvs = frame
            .geometry
            .source
            .runtime_frame
            .drawables
            .iter()
            .find(|d| d.id == id(2))
            .unwrap()
            .uvs
            .clone();
        let topology = frame.geometry.mesh(&id(2)).unwrap().triangles.clone();
        crate::support::pointer_input(
            &mut editor,
            CanvasInput::Move {
                point,
                left_down: true,
            },
        )?;
        let current = editor.document().unwrap().object_index();
        assert!(
            Arc::ptr_eq(&index, &current),
            "a coordinate-only drag rebuilt the panel index"
        );
        assert!(std::ptr::eq(
            index.get(&peer).unwrap(),
            current.get(&peer).unwrap()
        ));
        frame = crate::support::present_canvas(&mut editor)?;
        let after = frame
            .geometry
            .source
            .runtime_frame
            .drawables
            .iter()
            .find(|d| d.id == id(2))
            .unwrap();
        assert_ne!(before_uvs, after.uvs, "drag must update actual data");
        let actual_mesh = frame.geometry.mesh(&id(2)).unwrap();
        let rendered_triangles: Vec<_> = after
            .indices
            .chunks_exact(3)
            .map(|t| {
                [
                    actual_mesh.vertices[t[0] as usize].0,
                    actual_mesh.vertices[t[1] as usize].0,
                    actual_mesh.vertices[t[2] as usize].0,
                ]
            })
            .collect();
        assert_eq!(
            actual_mesh.triangles.as_ref(),
            rendered_triangles.as_slice()
        );
        if sample > 0 && topology.as_ref() == rendered_triangles.as_slice() {
            assert!(Arc::ptr_eq(
                &topology,
                &frame.geometry.mesh(&id(2)).unwrap().triangles
            ));
            retained_connectivity += 1;
        }
        assert_eq!(
            current.get(&object).unwrap().properties["triangles"].as_u64(),
            Some(rendered_triangles.len() as u64)
        );
    }
    assert!(
        retained_connectivity > 0,
        "fixture did not exercise topology reuse"
    );
    editor.cancel_gesture();
    assert_eq!(
        model_mesh_document(&editor).mesh(&id(2)),
        original.mesh(&id(2))
    );
    // Inspect the candidate before commit, when retained panels must stay shared.
    let (mut document, _assets) = mixed_dependency_document()?;
    let before = document.object_index();
    let target = document.manual_mesh_target()?;
    document.preview_manual_mesh(
        &target,
        zipzi_sdk::ManualMeshOperation::InsertVertex {
            id: 200,
            uv: Vec2::new(0.3, 0.25),
        },
    )?;
    let after = document.object_index();
    assert_eq!(before.get(&object).unwrap().properties["vertices"], 5);
    assert_eq!(after.get(&object).unwrap().properties["vertices"], 6);
    assert!(std::ptr::eq(
        before.get(&peer).unwrap(),
        after.get(&peer).unwrap()
    ));
    assert_eq!(before.bindings.as_ptr(), after.bindings.as_ptr());
    document.cancel_manual_mesh_gesture();
    assert_eq!(
        document.object_index().get(&object).unwrap().properties["vertices"],
        5
    );
    Ok(())
}
