use super::support;
use editor_core::*;
use kasane_core::{DeltaKeyforms, Vec2};

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
    let options = MeshGenerationOptions::Alpha(AlphaMeshOptions {
        outside_spacing: 8.,
        inside_spacing: 8.,
        outside_margin: 0.,
        inside_margin: 2.,
        minimum_margin: 0.,
        alpha_threshold: 64,
        ..AlphaMeshOptions::deformation_large()
    });
    let preview = source.generate(options)?;
    let geometry = preview.geometry().clone();
    assert!(geometry.positions.len() > 4);
    // The opaque L is in the upper left. The low-alpha island is excluded.
    for (p, uv) in geometry.positions.iter().zip(&geometry.uvs) {
        assert!((p.x - (24. + 208. * uv.x)).abs() < 0.001);
        assert!((p.y - (24. + 208. * uv.y)).abs() < 0.001);
        assert!(uv.x >= 8. / 96. - 1e-6 && uv.x <= 66. / 96. + 1e-6);
        assert!(uv.y >= 6. / 64. - 1e-6 && uv.y <= 42. / 64. + 1e-6);
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
        &dir.path().join("alpha.kasane"),
        false,
    )?;
    let saved = editor.finish_save(&mut save.run())?;
    let reopened = Document::open(&saved.manifest)?;
    let mut session = kasane_sdk::AuthoringSession::new(
        &id(920),
        kasane_core::Canvas::new(256., 256., Vec2::default(), 1.),
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
fn alpha_rejects_dependencies_empty_masks_and_stale_plans_without_edits() -> anyhow::Result<()> {
    let (bound, _dir) = support::rectangle_document(true)?;
    let options = MeshGenerationOptions::Alpha(AlphaMeshOptions::deformation_large());
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
            .generate(MeshGenerationOptions::Alpha(AlphaMeshOptions {
                alpha_threshold: 255,
                ..AlphaMeshOptions::default()
            }))
            .is_err()
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
fn alpha_outside_margin_pads_texture_atomically_and_regenerates_after_save() -> anyhow::Result<()> {
    let (mut doc, dir) = support::alpha_document()?;
    let source = doc.mesh_generation_source(&mesh())?;
    let original = source.texture.clone().unwrap();
    let settings = MeshGenerationOptions::Alpha(AlphaMeshOptions {
        outside_spacing: 16.,
        inside_spacing: 16.,
        outside_margin: 24.,
        minimum_margin: 8.,
        inside_margin: 4.,
        clip_to_image: false,
        remove_faint_islands: true,
        ..AlphaMeshOptions::deformation_small()
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
            .any(|p| p.x < 24. && p.y < 24.)
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
    let saved = doc.save(&dir.path().join("outside.kasane"))?;
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
