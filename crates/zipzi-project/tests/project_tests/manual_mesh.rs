use super::helpers::*;
use zipzi_core::state::*;
use zipzi_core::{Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing, ModelSetup, Vec2};

fn document() -> DocumentState {
    let mut d = DocumentState::new(
        "b0000000-0000-4000-8000-000000000001",
        ModelSetup::default().into(),
    )
    .unwrap();
    d = prepare(&d, |builder| {
        builder.create_mesh(Mesh {
            id: "b0000000-0000-4000-8000-000000000002".into(),
            runtime_id: "ManualMesh".into(),
            vertex_ids: vec![8, 2, 5, 12],
            base_positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(-1., 0.),
                Vec2::new(-1., 1.),
                Vec2::new(0., 1.),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[8, 2, 5], [8, 5, 12]],
            meshing: Some(MeshMeshing {
                boundary: vec![2, 5, 12, 8],
                constraints: vec![MeshConstraint {
                    endpoints: [5, 8],
                    kind: MeshConstraintKind::EdgeSupport,
                }],
            }),
            ..Default::default()
        })
    });
    d
}

#[test]
fn manual_mesh_json_preserves_structure_and_exact_faces() {
    let d = document();
    let text = fixture_bytes(&d);
    assert_eq!(project_json(&text), project_json(&resaved_native(&text)));
    let mut wire: serde_json::Value = project_json(&text);
    assert_eq!(wire["format_version"], 22);
    wire["document"]["meshes"][0]["meshing"]["constraints"][0]["endpoints"] =
        serde_json::json!([2, 12]);
    assert!(load_native(&replace_project_json(&text, &wire)).is_err());
    wire["format_version"] = 20.into();
    assert_eq!(
        load_native(&replace_project_json(&text, &wire))
            .unwrap_err()
            .code,
        "UNSUPPORTED_VERSION"
    );
}
