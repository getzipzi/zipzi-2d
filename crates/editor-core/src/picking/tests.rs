use super::CanvasScene;
use crate::{Document, ModelPoint};
use kasane_core::{Canvas, Mesh, Vec2};
use kasane_sdk::{AuthoringSession, prepare_png_asset};
use std::sync::Arc;

fn fixture() -> anyhow::Result<(Document, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("texture.png");
    std::fs::write(&path, include_bytes!("../../tests/fixtures/accent.png"))?;
    let asset = uuid::Uuid::from_u128(1).to_string();
    let mut session = AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Create meshes", None, |edit| {
        edit.create_asset(prepare_png_asset(&asset, "Texture", &path)?)?;
        for index in 2..=3 {
            edit.create_mesh(Mesh {
                id: uuid::Uuid::from_u128(index).to_string(),
                name: format!("Mesh {index}"),
                texture_asset_id: asset.clone(),
                vertex_ids: vec![0, 1, 2],
                base_positions: vec![
                    Vec2::new(24., 24.),
                    Vec2::new(232., 24.),
                    Vec2::new(128., 232.),
                ],
                uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0.5, 1.)],
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            })?;
        }
        Ok(())
    })?;
    let receipt = session.save_project(&directory.path().join("fixture.kasane"), None)?;
    Ok((Document::open(&receipt.manifest)?, directory))
}

#[test]
fn mesh_property_commits_reuse_geometry_and_publish_fresh_labels_and_visibility()
-> anyhow::Result<()> {
    use crate::{DocumentSession, EditorCommand, ObjectKind, ObjectRef};
    let (document, _directory) = fixture()?;
    let object = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::from_u128(2).to_string());
    let mut session = DocumentSession::from_document(document);
    let (_, original) = session.scene()?;
    for (field, value, visible) in [
        ("name", serde_json::json!("Renamed"), true),
        ("enabled", serde_json::json!(false), false),
        ("enabled", serde_json::json!(true), true),
        ("appearance.opacity", serde_json::json!(0.), false),
        ("appearance.opacity", serde_json::json!(1.), true),
    ] {
        let patch = session
            .document()
            .object_property_patch(&object, field, &value)?;
        session.execute(EditorCommand::SetObjectProperties {
            object: object.clone(),
            patch,
        })?;
        let (_, scene) = session.scene()?;
        assert!(Arc::ptr_eq(&original.meshes, &scene.meshes));
        assert_eq!(scene.index.name(&object), "Renamed");
        assert_eq!(original.index.name(&object), "Mesh 2");
        assert_eq!(
            scene
                .mesh_hits(ModelPoint { x: 128., y: 128. })
                .contains(&object.id),
            visible
        );
        assert!(
            original
                .mesh_hits(ModelPoint { x: 128., y: 128. })
                .contains(&object.id)
        );
    }
    session.execute(EditorCommand::Undo)?;
    let (_, scene) = session.scene()?;
    assert!(
        !scene
            .mesh_hits(ModelPoint { x: 128., y: 128. })
            .contains(&object.id)
    );
    session.execute(EditorCommand::Redo)?;
    let (_, scene) = session.scene()?;
    assert!(
        scene
            .mesh_hits(ModelPoint { x: 128., y: 128. })
            .contains(&object.id)
    );
    Ok(())
}

#[test]
fn updated_positions_reuse_other_meshes_and_refresh_bounds_and_boundaries() -> anyhow::Result<()> {
    let (mut document, _directory) = fixture()?;
    let source = document.snapshot()?;
    let original = CanvasScene::new(&document, source.clone());
    let mut moved = source;
    let frame = Arc::make_mut(&mut moved.viewport_frame);
    frame.drawables[1].positions[0].x -= 50.;
    let updated = original.with_positions(&document, moved);
    assert!(Arc::ptr_eq(&original.meshes[0], &updated.meshes[0]));
    assert!(!Arc::ptr_eq(&original.meshes[1], &updated.meshes[1]));
    assert_ne!(original.meshes[1].bounds, updated.meshes[1].bounds);
    assert_ne!(original.meshes[1].boundary, updated.meshes[1].boundary);
    assert_eq!(original.meshes[1].vertices[0].1.x, 24.);
    assert_eq!(updated.meshes[1].vertices[0].1.x, -26.);
    assert!(
        updated
            .mesh_hits(ModelPoint { x: 0., y: 25. })
            .contains(&updated.meshes[1].id)
    );
    assert!(
        !original
            .mesh_hits(ModelPoint { x: 0., y: 25. })
            .contains(&original.meshes[1].id)
    );
    Ok(())
}

#[test]
fn geometry_reuse_respects_canvas_coordinates_drawable_order_and_topology() -> anyhow::Result<()> {
    let (mut document, _directory) = fixture()?;
    let source = document.snapshot()?;
    let original = CanvasScene::new(&document, source.clone());
    let mut reordered = source.clone();
    Arc::make_mut(&mut reordered.viewport_frame)
        .drawables
        .swap(0, 1);
    let reordered = original.with_positions(&document, reordered);
    for (index, mesh) in reordered.meshes.iter().enumerate() {
        assert_eq!(mesh.id, original.meshes[1 - index].id);
        assert_eq!(reordered.mesh(&mesh.id).unwrap().vertices, mesh.vertices);
        assert_eq!(mesh.vertices, original.meshes[1 - index].vertices);
    }
    // Metadata edits reuse exactly the same model-space mesh geometry.
    let updated = original.with_positions(&document, source.clone());
    assert!(
        original
            .meshes
            .iter()
            .zip(updated.meshes.iter())
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );
    document.preview_name("Renamed mesh")?;
    let renamed = document.snapshot()?;
    let renamed = original.with_positions(&document, renamed);
    assert!(!Arc::ptr_eq(&original.index, &renamed.index));
    assert!(
        original
            .meshes
            .iter()
            .zip(renamed.meshes.iter())
            .all(|(a, b)| !Arc::ptr_eq(a, b))
    );
    assert!(
        renamed
            .index
            .objects
            .iter()
            .any(|object| object.name == "Renamed mesh")
    );
    Ok(())
}
