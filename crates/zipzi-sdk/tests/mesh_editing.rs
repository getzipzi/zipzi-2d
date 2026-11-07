use std::collections::HashMap;

use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{rectangle_mesh, AuthoringSession};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000101";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000102";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000103";
const OTHER_ID: &str = "00000000-0000-4000-8000-000000000104";

fn fixture() -> AuthoringSession {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = crate::support::image_fixture(ASSET_ID, "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "eye",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let mut session = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                DOCUMENT_ID,
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)?;
                edit.create_mesh(mesh)
            })
            .and_then(|value| {
                let prepared = draft.prepare("fixture")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    session.take_events();
    session
}

#[test]
fn mesh_create_spec_allocates_identity_and_keeps_defaults_in_sdk() {
    use zipzi_sdk::{MeshCreateSpec, MeshGeometry};
    let mut session = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(DOCUMENT_ID, Default::default()).unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    let mut draft = session.begin_draft();
    let id = draft
        .batch(draft.source(), |c| {
            c.create_mesh_spec(MeshCreateSpec {
                id: None,
                name: "Face".into(),
                texture_asset_id: String::new(),
                geometry: MeshGeometry {
                    vertex_ids: vec![42, 7, 99],
                    positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    triangles: vec![[42, 7, 99]],
                    meshing: None,
                },
            })
        })
        .unwrap();
    assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
    draft
        .batch(draft.source(), |c| {
            c.rename(zipzi_sdk::NameTarget::Mesh(&id), "Final".into())
        })
        .unwrap();
    let receipt = session.publish(draft.prepare("mesh").unwrap()).unwrap();
    assert!(!receipt.writes.is_empty());
    let mesh = session.snapshot().document().mesh(&id).unwrap();
    assert_eq!(mesh.runtime_id, id);
    assert_eq!(mesh.name, "Final");
    assert!(mesh.enabled);
    assert_eq!(mesh.vertex_ids, [42, 7, 99]);
    assert_eq!(mesh.appearance, zipzi_core::Appearance::default());
    assert_eq!(session.history_state().undo_steps, 1);
}

#[test]
fn explicit_replacements_preserve_geometry_and_identity() {
    let mut session = fixture();
    let original = session
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .cloned()
        .unwrap();
    let mut other = rectangle_mesh(
        OTHER_ID,
        "eye",
        ASSET_ID,
        Vec2::new(20.0, 20.0),
        Vec2::new(30.0, 30.0),
    )
    .unwrap();
    other.runtime_id = "runtime.other".into();
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.create_mesh(other))
            .and_then(|value| {
                let prepared = draft.prepare("second mesh")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let matches: Vec<_> = session
        .snapshot()
        .document()
        .mesh_order()
        .iter()
        .filter(|id| session.snapshot().document().mesh(id).unwrap().name == "eye")
        .collect();
    assert_eq!(matches.len(), 2);
    let mut asset = session
        .snapshot()
        .document()
        .asset(ASSET_ID)
        .cloned()
        .unwrap();
    asset.name = "renamed texture".into();
    let mut properties = original.clone();
    properties.appearance.opacity = 0.4;
    properties.draw_order = Some(5.0);
    let (_, receipt) = {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.replace_asset(asset)?;
                edit.rename(zipzi_sdk::NameTarget::Mesh(OTHER_ID), ("right eye").into())?;
                edit.replace_mesh(properties)
            })
            .and_then(|value| {
                let prepared = draft.prepare("metadata and appearance")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(!receipt.writes.is_empty());
    assert_eq!(
        session.snapshot().document().mesh(MESH_ID).unwrap().name,
        "eye"
    );
    assert_eq!(
        session
            .snapshot()
            .document()
            .asset(ASSET_ID)
            .cloned()
            .unwrap()
            .name,
        "renamed texture"
    );
    let changed = session
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .cloned()
        .unwrap();
    assert_eq!(changed.id, original.id);
    assert_eq!(changed.runtime_id, original.runtime_id);
    assert_eq!(changed.vertex_ids, original.vertex_ids);
    assert_eq!(changed.base_positions, original.base_positions);
    assert_eq!(changed.appearance.opacity, 0.4);
    assert_eq!(changed.draw_order, Some(5.0));
    session.undo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .asset(ASSET_ID)
            .cloned()
            .unwrap()
            .name,
        "texture"
    );
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .appearance
            .opacity,
        1.0
    );
    session.redo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .appearance
            .opacity,
        0.4
    );
}

#[test]
fn topology_requires_fresh_snapshot_complete_mapping_and_atomic_validation() {
    let mut session = fixture();
    let original = session.snapshot().document().mesh(MESH_ID).unwrap().clone();
    let mut draft = session.begin_draft();
    let mut geometry = zipzi_sdk::MeshGeometry {
        vertex_ids: original.vertex_ids.clone(),
        positions: original.base_positions.clone(),
        uvs: original.uvs.clone(),
        triangles: original.triangles.clone(),
        meshing: None,
    };
    geometry.triangles[0][0] = 999;
    assert!(draft
        .batch(draft.source(), |c| c.remesh(
            MESH_ID,
            geometry,
            Default::default(),
            &Default::default()
        ))
        .is_err());
    assert!(draft.prepare("invalid").is_err());
    assert_eq!(
        session.snapshot().document().mesh(MESH_ID).unwrap(),
        &original
    );
    let (_, stale) = crate::support::prepared(&session, |c| {
        c.remesh_rectangle_grid(MESH_ID, 4, 4, Default::default(), &Default::default())
    })
    .unwrap();
    let (_, next) = crate::support::prepared(&session, |c| {
        c.remesh_rectangle_grid(MESH_ID, 2, 2, Default::default(), &Default::default())
    })
    .unwrap();
    session.publish(next).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .unwrap()
            .vertex_ids
            .len(),
        9
    );
    session.undo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().mesh(MESH_ID).unwrap(),
        &original
    );
    assert_eq!(session.publish(stale).unwrap_err().code, "STALE_SOURCE");
}

#[test]
fn invalid_property_replace_aborts_every_write_in_batch() {
    let mut session = fixture();
    let version = session.version();
    let mut properties = session.snapshot().document().mesh(MESH_ID).unwrap().clone();
    properties.texture_asset_id = OTHER_ID.into();
    let mut draft = session.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| {
                c.rename(zipzi_sdk::NameTarget::Mesh(MESH_ID), "changed".into())?;
                c.replace_mesh(properties)
            })
            .unwrap_err()
            .code,
        "MISSING_ASSET"
    );
    assert!(draft.prepare("bad texture").is_err());
    assert_eq!(session.version(), version);
    assert_eq!(
        session.snapshot().document().mesh(MESH_ID).unwrap().name,
        "eye"
    );
    assert!(session.take_events().is_empty());
}

#[test]
fn name_only_edit_changes_content_and_history_restores_exact_nodes() {
    let mut session = fixture();
    let revision = session.version().revision;
    let evaluation_revision = session.snapshot().content_id();
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.rename(zipzi_sdk::NameTarget::Mesh(MESH_ID), ("new name").into())
            })
            .and_then(|value| {
                let prepared = draft.prepare("rename")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(session.version().revision, revision + 1);
    assert_ne!(session.snapshot().content_id(), evaluation_revision);
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .name,
        "new name"
    );
    session.undo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .name,
        "eye"
    );
    assert_eq!(session.snapshot().content_id(), evaluation_revision);
    session.redo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .name,
        "new name"
    );
    assert_ne!(session.snapshot().content_id(), evaluation_revision);
}

#[test]
fn rectangle_grid_preserves_bound_surface_and_history() {
    use zipzi_core::{BindingAxis, MeshBinding, MeshKeyform, Parameter};
    let mut session = fixture();
    let parameter_id = "00000000-0000-4000-8000-000000000105";
    let binding_id = "00000000-0000-4000-8000-000000000106";
    let corners = vec![
        Vec2::new(0., 0.),
        Vec2::new(12., -1.),
        Vec2::new(8., 12.),
        Vec2::new(-1., 11.),
    ];
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_parameter({
                    let mut value = Parameter {
                        id: parameter_id.into(),
                        name: "pose".into(),
                        minimum: 0.,
                        maximum: 1.,
                        default_value: 0.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                edit.create_binding(MeshBinding {
                    id: binding_id.into(),
                    mesh_id: MESH_ID.into(),
                    axes: vec![BindingAxis {
                        id: Default::default(),
                        samples: Default::default(),
                        parameter_id: parameter_id.into(),
                        keys: vec![0., 1.],
                    }],
                    keyforms: vec![
                        MeshKeyform {
                            keys: vec![0.],
                            positions: vec![
                                Vec2::new(0., 0.),
                                Vec2::new(10., 0.),
                                Vec2::new(10., 10.),
                                Vec2::new(0., 10.),
                            ],
                            ..Default::default()
                        },
                        MeshKeyform {
                            keys: vec![1.],
                            positions: corners,
                            ..Default::default()
                        },
                    ],
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("binding")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = session
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .cloned()
        .unwrap();
    let version = session.version();
    let history = {
        let history = session.history_state();
        (history.undo_steps, history.redo_steps)
    };
    assert!(
        crate::support::prepared(&session, |c| c.remesh_rectangle_grid(
            MESH_ID,
            4,
            3,
            Default::default(),
            &Default::default()
        ))
        .is_ok()
    );
    assert_eq!(session.version(), version);
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    let values = HashMap::from([(parameter_id.to_string(), 0.5)]);
    let before = crate::support::frame(session.snapshot(), &values).unwrap();
    let (_, p) = crate::support::prepared(&session, |c| {
        c.remesh_rectangle_grid(MESH_ID, 4, 4, Default::default(), &Default::default())
    })
    .unwrap();
    let stale = session.begin_draft();
    let receipt = session.publish(p).unwrap();
    assert!(!receipt.writes.is_empty());
    let mesh = session
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .cloned()
        .unwrap();
    assert_eq!(mesh.vertex_ids.len(), 25);
    assert_eq!(mesh.triangles.len(), 32);
    let after = crate::support::frame(session.snapshot(), &values).unwrap();
    // Independently interpolate the old evaluated surface at every new vertex.
    let old = &before.drawables[0].positions;
    let new = &after.drawables[0].positions;
    for row in 0..=4 {
        for col in 0..=4 {
            let u = col as f32 / 4.;
            let v = row as f32 / 4.;
            let weights = if u >= v {
                [1. - u, u - v, v, 0.]
            } else {
                [1. - v, 0., u, v - u]
            };
            let x: f32 = old.iter().zip(weights).map(|(p, w)| p.x * w).sum();
            let y: f32 = old.iter().zip(weights).map(|(p, w)| p.y * w).sum();
            assert!((new[row * 5 + col].x - x).abs() < 1e-5);
            assert!((new[row * 5 + col].y - y).abs() < 1e-5);
        }
    }
    session.undo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap(),
        original
    );
    let current = session.version();
    assert_eq!(
        session
            .publish(stale.prepare("old geometry").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(session.version(), current);
    session.redo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap(),
        mesh
    );
}

#[test]
fn rectangle_grid_rejects_invalid_geometry_and_overflow() {
    use zipzi_sdk::{rectangle_grid_geometry, MeshGeometry};
    let mesh = fixture()
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .cloned()
        .unwrap();
    let mut source = MeshGeometry {
        vertex_ids: mesh.vertex_ids,
        positions: mesh.base_positions,
        uvs: mesh.uvs,
        triangles: mesh.triangles,
        meshing: mesh.meshing,
    };
    assert!(rectangle_grid_geometry(&source, usize::MAX, 1).is_err());
    assert!(rectangle_grid_geometry(&source, 0, 1).is_err());
    assert!(rectangle_grid_geometry(&source, 256, 256).is_err());
    let grid = rectangle_grid_geometry(&source, 4, 3).unwrap();
    assert_eq!(grid.vertex_ids.len(), 20);
    assert_eq!(grid.triangles.len(), 24);
    source.positions[0].x = f32::NAN;
    assert!(rectangle_grid_geometry(&source, 4, 4).is_err());
    source.positions[0].x = 0.;
    source.vertex_ids[3] = u32::MAX;
    source.triangles[1][2] = u32::MAX;
    assert!(rectangle_grid_geometry(&source, 2, 2).is_err());
    assert!(rectangle_grid_geometry(&source, 1, 1).is_ok());
}

#[test]
fn selected_mesh_api_defaults_are_owned_by_rust_serde_and_exported_consistently() {
    use zipzi_core::api_shape::{ApiShape, DefaultValue, Shape};
    let spec: zipzi_sdk::MeshCreateSpec = serde_json::from_value(serde_json::json!({
        "name":"Face", "geometry":{"vertex_ids":[42,7,99], "positions":[{"x":0.,"y":0.},{"x":1.,"y":0.},{"x":0.,"y":1.}],
            "uvs":[{"x":0.,"y":0.},{"x":1.,"y":0.},{"x":0.,"y":1.}], "triangles":[[42,7,99]]}
    })).unwrap();
    assert_eq!(spec.texture_asset_id, "");
    assert_eq!(spec.id, None);
    assert_eq!(spec.geometry.vertex_ids, vec![42, 7, 99]);
    let Shape::Record { fields, .. } = zipzi_sdk::MeshCreateSpec::api_shape() else {
        panic!("record");
    };
    assert_eq!(
        fields
            .iter()
            .find(|f| f.name == "texture_asset_id")
            .unwrap()
            .default,
        Some(DefaultValue::EmptyString)
    );
    assert_eq!(
        fields.iter().find(|f| f.name == "id").unwrap().default,
        Some(DefaultValue::Null)
    );
}
