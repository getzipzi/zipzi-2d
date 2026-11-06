use crate::support::image;
pub use crate::support::prepare;
use zipzi_core::state::DocumentState;
use zipzi_core::types::{Canvas, Mesh, Parameter, ParameterKind, Vec2};

pub const DOC: &str = "11111111-1111-4111-8111-111111111111";
pub const ASSET: &str = "22222222-2222-4222-8222-222222222222";
pub const MESH_A: &str = "33333333-3333-4333-8333-333333333333";
pub const MESH_B: &str = "44444444-4444-4444-8444-444444444444";
pub const PARAM_NORM: &str = "55555555-5555-4555-8555-555555555555";
pub const PARAM_BS: &str = "66666666-6666-4666-8666-666666666666";
pub const KEY_TABLE: &str = "77777777-7777-4777-8777-777777777777";
pub const CONSTRAINT: &str = "88888888-8888-4888-8888-888888888888";
pub const BINDING_BS: &str = "99999999-9999-4999-8999-999999999999";
pub const GLUE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

pub fn create_base_document() -> DocumentState {
    let base =
        DocumentState::new(DOC, Canvas::new(1000., 1000., Vec2::default(), 1.).into()).unwrap();
    let mut doc = base.builder();
    doc.create_asset(image(ASSET.into(), "texture", 100, 100))
        .unwrap();

    doc.create_mesh(Mesh {
        id: MESH_A.to_string(),
        runtime_id: "MeshA".to_string(),
        name: "MeshA".to_string(),
        texture_asset_id: (ASSET.to_string()).into(),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    })
    .unwrap();

    doc.create_mesh(Mesh {
        id: MESH_B.to_string(),
        runtime_id: "MeshB".to_string(),
        name: "MeshB".to_string(),
        texture_asset_id: (ASSET.to_string()).into(),
        vertex_ids: vec![10, 20, 30],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[10, 20, 30]],
        ..Default::default()
    })
    .unwrap();

    doc.create_parameter(Parameter {
        id: PARAM_NORM.to_string(),
        runtime_id: "ParamNorm".to_string(),
        name: "ParamNorm".to_string(),
        minimum: -1.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::Normal,
        repeat: false,
    })
    .unwrap();

    doc.create_parameter(Parameter {
        id: PARAM_BS.to_string(),
        runtime_id: "ParamBS".to_string(),
        name: "ParamBS".to_string(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::BlendShape,
        repeat: false,
    })
    .unwrap();

    doc.freeze().unwrap().state
}

pub fn assert_vec2_near(a: Vec2, b: Vec2, eps: f32) {
    assert!(
        (a.x - b.x).abs() <= eps && (a.y - b.y).abs() <= eps,
        "left: {:?}, right: {:?}, eps: {}",
        a,
        b,
        eps
    );
}
