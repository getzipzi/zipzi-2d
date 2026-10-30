use zipzi_contract::{
    api_shape::ApiShape,
    authoring::{GeometryChecks, MeshCreateSpec, MeshGeometry, MeshOperation},
    binary_wire::{Reader, WireRead, WireWrite},
    operations::MESH_QUERY,
    runtime::{MeshQuery, MeshSnapshot},
};

#[test]
fn authoring_defaults_tags_and_field_order_survive_export_and_serde() {
    let defaults = serde_json::from_str::<GeometryChecks>("{}").unwrap();
    assert_eq!(defaults.min_triangle_area, 0.0);
    assert_eq!(defaults.model_bounds, None);
    assert!(!defaults.quality);
    let operation: MeshOperation = serde_json::from_value(serde_json::json!({
        "op": "create", "spec": {
            "name": "mesh", "id": null,
            "geometry": {"vertex_ids": [], "positions": [], "uvs": [], "triangles": []}
        }
    }))
    .unwrap();
    let MeshOperation::Create { spec } = &operation else {
        panic!("create tag changed")
    };
    assert_eq!(spec.texture_asset_id, "");
    assert_eq!(spec.geometry.meshing, None);
    assert_eq!(serde_json::to_value(operation).unwrap()["op"], "create");
    let shape = serde_json::to_value(MeshCreateSpec::api_shape()).unwrap();
    let fields: Vec<_> = shape["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["name", "geometry", "texture_asset_id", "id"]);
    let (input, output) = MESH_QUERY.shapes();
    assert_eq!(
        serde_json::to_value(input).unwrap(),
        serde_json::to_value(MeshQuery::api_shape()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(output).unwrap(),
        serde_json::to_value(MeshSnapshot::api_shape()).unwrap()
    );
}

#[test]
fn geometry_binary_layout_is_ordered_and_admitted_before_decode() {
    let geometry = MeshGeometry {
        vertex_ids: vec![7],
        positions: vec![zipzi_contract::model::Vec2::new(1.0, -2.0)],
        uvs: vec![zipzi_contract::model::Vec2::new(0.25, 0.5)],
        triangles: vec![],
        meshing: None,
    };
    let mut bytes = Vec::new();
    geometry.encode(&mut bytes).unwrap();
    let mut expected = Vec::new();
    // Counts precede each sequence; fields follow their Rust declaration order.
    for value in [
        1u32,
        7,
        1,
        1.0f32.to_bits(),
        (-2.0f32).to_bits(),
        1,
        0.25f32.to_bits(),
        0.5f32.to_bits(),
        0,
    ] {
        expected.extend_from_slice(&value.to_le_bytes());
    }
    expected.push(0); // absent meshing
    assert_eq!(bytes, expected);
    let mut cost = Reader::new(&bytes).unwrap();
    assert!(MeshGeometry::input_cost(&mut cost).unwrap() >= bytes.len());
    cost.finish().unwrap();
    let mut reader = Reader::new(&bytes).unwrap();
    assert_eq!(MeshGeometry::decode(&mut reader).unwrap(), geometry);
    reader.finish().unwrap();
    assert!(
        MeshGeometry::input_cost(&mut Reader::new(&bytes[..bytes.len() - 1]).unwrap()).is_err()
    );
}
