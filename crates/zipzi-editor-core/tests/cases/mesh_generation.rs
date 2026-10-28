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
    let mut session = zipzi_sdk::AuthoringSession::new(
        &id(920),
        zipzi_core::Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(manifest, None)?;
    Ok(session.mesh(&mesh().id).unwrap().meshing.is_some())
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
    assert!(saved_mesh_has_meshing(&saved.manifest)?);
    Ok(())
}

#[test]
fn manual_mesh_mode_uses_full_reference_and_one_history_step_per_point() -> anyhow::Result<()> {
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
            .any(|drawable| {
                drawable.id.starts_with("__manual_mesh_reference_") && drawable.positions.len() == 4
            })
    );
    assert!(
        preview
            .scene
            .runtime_frame
            .drawables
            .iter()
            .all(|drawable| { !drawable.id.starts_with("__manual_mesh_reference_") })
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
    assert!(saved_mesh_has_meshing(&saved.manifest)?);
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
    let mut session = zipzi_sdk::AuthoringSession::new(
        &id(920),
        zipzi_core::Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let saved_mesh = session.mesh(&mesh().id).unwrap();
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
    let reopened = Document::open(&saved.manifest)?;
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
    let mut session = zipzi_sdk::AuthoringSession::new(
        &id(920),
        zipzi_core::Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let binding = session.binding_for_mesh(&id(902)).unwrap();
    assert!(binding.keyforms.iter().all(|f| f.positions.len() == 25));
    assert_eq!(binding.keyforms[1].positions[12], Vec2::new(138., 133.));
    let DeltaKeyforms::Mesh(forms) = session.blend_binding(&id(908)).unwrap().keyforms else {
        panic!()
    };
    assert!(forms.iter().all(|f| f.positions.len() == 25));
    assert_eq!(forms[1].positions[12], Vec2::new(2., 3.));
    assert_eq!(session.glue(&id(910)).unwrap().pairs[0].vertex_a, Some(0));
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
    let mut session = zipzi_sdk::AuthoringSession::new(
        &id(920),
        zipzi_core::Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let applied = session.mesh(&mesh().id).unwrap();
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
        let [a, b, c] = face.map(|i| geometry.positions[i as usize]);
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
    assert_eq!(regenerated.geometry().triangles, geometry.triangles);
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
    let reopened = Document::open(&saved.manifest)?;
    let mut session = zipzi_sdk::AuthoringSession::new(
        &id(920),
        zipzi_core::Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let applied = session.mesh(&mesh().id).unwrap();
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
    let reopened = Document::open(&saved.manifest)?;
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
    let runtime = host.read();
    runtime
        .document(document.runtime_session_id())
        .unwrap()
        .get_mesh(&mesh().id)
        .unwrap()
        .clone()
}

#[test]
fn manual_mesh_isolation_keeps_reference_at_full_opacity_and_mode_active() -> anyhow::Result<()> {
    use zipzi_core::evaluation::RenderCommand;
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
            if display == IsolationDisplay::Dim {
                let drawn = frame.render_plan.iter().position(|c| matches!(c, RenderCommand::DrawMesh { mesh_id } if mesh_id == &reference.id)).unwrap();
                let context_end = frame.render_plan.iter().position(|c| matches!(c, RenderCommand::EndOffscreen { offscreen_id } if offscreen_id == "zipzi-editor:isolation:context")).unwrap();
                assert!(
                    drawn > context_end,
                    "Reference must not be composited into the dimmed context"
                );
            }
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
    let (mut editor, _dir) = manual_interaction_fixture()?;
    editor.dispatch(editor.command(CommandId::ManualSelect))?;
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
    assert_eq!(editor.manual_tool(), Some(ManualTool::Select));
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
    assert_eq!(editor.manual_tool(), Some(ManualTool::Select));
    assert_eq!(editor.document().unwrap().history_lengths(), (3, 0));
    for expected in [&moved, &created, &original] {
        editor.dispatch(editor.command(CommandId::Undo))?;
        assert_eq!(&edited_mesh(&editor), expected);
    }
    for expected in [&created, &moved] {
        editor.dispatch(editor.command(CommandId::Redo))?;
        assert_eq!(&edited_mesh(&editor), expected);
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
