use zipzi_core::{ModelSetup, Vec2};
use zipzi_runtime::*;
use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};

fn id(n: u32) -> String {
    format!("79100000-0000-4000-8000-{n:012x}")
}

#[test]
fn selected_mesh_query_checks_exact_session_and_admits_before_copy_without_poisoning() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let other = rt
        .create_session(client, &id(2), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "large", None).unwrap();
    let base = MeshGeometry {
        vertex_ids: vec![0, 1, 2, 3],
        positions: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        uvs: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    };
    let geometry = zipzi_sdk::rectangle_grid_geometry(&base, 100, 100).unwrap();
    rt.edit_meshes(
        client,
        edit,
        vec![MeshOperation::Create {
            spec: MeshCreateSpec {
                id: Some(id(3)),
                name: "Grid".into(),
                texture_asset_id: String::new(),
                geometry,
            },
        }],
    )
    .unwrap();
    assert!(matches!(
        rt.mesh_snapshot(client, other, Some(edit), &id(3)),
        Err(RuntimeError::InvalidReference)
    ));
    assert!(matches!(
        rt.mesh_snapshot(client, session, Some(edit), &id(3)),
        Err(RuntimeError::Budget("mesh snapshot"))
    ));
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 1);
    rt.edit_meshes(
        client,
        edit,
        vec![MeshOperation::Rename {
            id: id(3),
            name: "Renamed".into(),
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(
        rt.document(session).unwrap().get_mesh(&id(3)).unwrap().name,
        "Renamed"
    );
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}
