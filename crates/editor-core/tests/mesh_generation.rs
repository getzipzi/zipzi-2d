use editor_core::*;
use kasane_core::{DeltaKeyforms, Vec2};
mod support;

fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
fn mesh() -> ObjectRef {
    ObjectRef::new(ObjectKind::Mesh, id(902))
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
fn generation_rebuilds_canvas_and_survives_history_and_save() -> anyhow::Result<()> {
    let (doc, dir) = support::rectangle_document(false)?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
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
    assert!(!editor.command_available(editor.command(CommandId::GenerateMesh)));
    editor.execute(EditorCommand::Undo)?;
    present(&mut editor, 4, 2)?;
    assert!(editor.command_available(editor.command(CommandId::GenerateMesh)));
    editor.execute(EditorCommand::Redo)?;
    present(&mut editor, 20, 24)?;
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("generated.kasane"),
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
    let mut editor = Editor::from_document(doc);
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
        &dir.path().join("bound.kasane"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let mut session = kasane_sdk::AuthoringSession::new(
        &id(920),
        kasane_core::Canvas::new(256., 256., Vec2::default(), 1.),
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
    assert_eq!(session.glue(&id(910)).unwrap().pairs[0].vertex_a, 0);
    Ok(())
}

#[test]
fn generation_rejects_stale_locked_unsupported_and_invalid_targets() -> anyhow::Result<()> {
    let (doc, _dir) = support::rectangle_document(false)?;
    let mut editor = Editor::from_document(doc);
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
    let editor = Editor::from_document(doc);
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
    let saved = doc.save(&dir.path().join("preview-applied.kasane"))?;
    let mut session = kasane_sdk::AuthoringSession::new(
        &id(920),
        kasane_core::Canvas::new(256., 256., Vec2::default(), 1.),
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
    let mut editor = Editor::from_document(doc);
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
