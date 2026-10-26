use zipzi_core::{
    Document, Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing, ModelSetup, Vec2,
};
use zipzi_project::{decode_project, encode_project};
fn document() -> Document {
    let mut d = Document::new();
    assert!(d
        .initialize(
            "b0000000-0000-4000-8000-000000000001",
            ModelSetup::default()
        )
        .is_ok());
    assert!(d
        .create_mesh(Mesh {
            id: "b0000000-0000-4000-8000-000000000002".into(),
            vertex_ids: vec![8, 2, 5, 12],
            base_positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(-1., 0.),
                Vec2::new(-1., 1.),
                Vec2::new(0., 1.)
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.)
            ],
            triangles: vec![[8, 2, 5], [8, 5, 12]],
            meshing: Some(MeshMeshing {
                boundary: vec![2, 5, 12, 8],
                constraints: vec![MeshConstraint {
                    endpoints: [5, 8],
                    kind: MeshConstraintKind::EdgeSupport
                }]
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    d
}

#[test]
fn manual_mesh_json_preserves_structure_and_exact_faces() {
    let d = document();
    let text = encode_project(&d).unwrap();
    assert!(d.same_content(&decode_project(&text).unwrap()));
    let mut wire: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(wire["format_version"], 21);
    wire["document"]["meshes"][0]["meshing"]["constraints"][0]["endpoints"] =
        serde_json::json!([2, 12]);
    assert!(decode_project(&wire.to_string()).is_err());
    wire["format_version"] = 20.into();
    assert_eq!(
        decode_project(&wire.to_string()).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );
}

#[cfg(feature = "binary-prototype")]
#[test]
fn manual_mesh_cbor_preserves_structure_and_exact_faces() {
    let d = document();
    let bytes = zipzi_project::encode_project_cbor(&d).unwrap();
    assert!(d.same_content(&zipzi_project::decode_project_cbor(&bytes).unwrap()));
}
