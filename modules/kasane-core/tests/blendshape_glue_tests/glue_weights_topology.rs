use super::common::*;
use kasane_core::types::{
    BlendShapeBinding, BlendShapeKeyTable, BlendShapeTargetKind, DeltaGlueKeyform, DeltaKeyforms,
    Glue, GlueVertexPair, Mesh, Vec2,
};

#[test]
fn test_glue_blendshape_asymmetric_weights() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    let mut mesh_b = doc.get_mesh(MESH_B).unwrap().clone();
    mesh_b.base_positions[0] = Vec2::new(10.0, 20.0);
    assert!(doc.replace_mesh(mesh_b).status.is_ok());

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: MESH_A.to_string(),
        mesh_b_id: MESH_B.to_string(),
        pairs: vec![GlueVertexPair {
            vertex_a: 1,
            vertex_b: 10,
            weight_a: 0.7,
            weight_b: 0.3,
        }],
        intensity: 0.0,
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: GLUE_ID.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.0,
            },
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 1.0,
            },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    let mut preview: PreviewValues = HashMap::new();
    preview.insert(PARAM_BS.to_string(), 1.0);
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());

    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    // d = (10, 20)
    // a += d * (1.0 * 0.7) = (7.0, 14.0) -> canvas: (7.0, -14.0)
    // b -= d * (1.0 * 0.3) = (10 - 3.0, 20 - 6.0) = (7.0, 14.0) -> canvas: (7.0, -14.0)
    assert_eq!(a.positions[0], Vec2::new(7.0, 14.0));
    assert_eq!(b.positions[0], Vec2::new(7.0, 14.0));
}

#[test]
fn test_glue_blendshape_ordered_multiple_glues() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    let mesh_c_id = "44444444-4444-4444-8444-444444444445";
    assert!(doc
        .create_mesh(Mesh {
            id: mesh_c_id.to_string(),
            runtime_id: "MeshC".to_string(),
            name: "MeshC".to_string(),
            texture_asset_id: ASSET.to_string(),
            vertex_ids: vec![100, 200, 300],
            base_positions: vec![
                Vec2::new(20.0, 0.0),
                Vec2::new(30.0, 0.0),
                Vec2::new(20.0, 10.0)
            ],
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0)
            ],
            triangles: vec![[100, 200, 300]],
            ..Default::default()
        })
        .status
        .is_ok());

    // Glue 1: Mesh A (1) at (0, 0) and Mesh B (10) at (10, 0)
    let glue1_id = GLUE_ID;
    let glue1 = Glue {
        id: glue1_id.to_string(),
        runtime_id: "Glue1".to_string(),
        name: "Glue 1".to_string(),
        mesh_a_id: MESH_A.to_string(),
        mesh_b_id: MESH_B.to_string(),
        pairs: vec![GlueVertexPair {
            vertex_a: 1,
            vertex_b: 10,
            weight_a: 0.5,
            weight_b: 0.5,
        }],
        intensity: 0.0,
        binding: None,
    };
    assert!(doc.create_glue(glue1).status.is_ok());

    // Glue 2: Mesh B (10) and Mesh C (100) at (20, 0)
    let glue2_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaab";
    let glue2 = Glue {
        id: glue2_id.to_string(),
        runtime_id: "Glue2".to_string(),
        name: "Glue 2".to_string(),
        mesh_a_id: MESH_B.to_string(),
        mesh_b_id: mesh_c_id.to_string(),
        pairs: vec![GlueVertexPair {
            vertex_a: 10,
            vertex_b: 100,
            weight_a: 0.5,
            weight_b: 0.5,
        }],
        intensity: 0.0,
        binding: None,
    };
    assert!(doc.create_glue(glue2).status.is_ok());

    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    // Blend binding on Glue 1 (delta 1.0)
    let b1 = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: glue1_id.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.0,
            },
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 1.0,
            },
        ]),
    };
    assert!(doc.create_blend_binding(b1).status.is_ok());

    // Blend binding on Glue 2 (delta 1.0)
    let b2_id = "99999999-9999-4999-8999-999999999998";
    let b2 = BlendShapeBinding {
        id: b2_id.to_string(),
        target_id: glue2_id.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.0,
            },
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 1.0,
            },
        ]),
    };
    assert!(doc.create_blend_binding(b2).status.is_ok());

    let mut preview: PreviewValues = HashMap::new();
    preview.insert(PARAM_BS.to_string(), 1.0);
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());

    // Sequential evaluation:
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    let c = frame.drawables.iter().find(|d| d.id == mesh_c_id).unwrap();
    assert_eq!(b.positions[0], Vec2::new(10.0, 0.0));
    assert_eq!(c.positions[0], Vec2::new(10.0, 0.0));
}

#[test]
fn test_glue_blendshape_faceless_endpoints() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    // Create a faceless mesh (0 triangles, like Hiyori's 4 faceless meshes)
    let faceless_id = "33333333-3333-4333-8333-333333333399";
    assert!(doc
        .create_mesh(Mesh {
            id: faceless_id.to_string(),
            runtime_id: "FacelessMesh".to_string(),
            name: "Faceless".to_string(),
            texture_asset_id: ASSET.to_string(),
            vertex_ids: vec![101],
            base_positions: vec![Vec2::new(0.0, 0.0)],
            uvs: vec![Vec2::new(0.0, 0.0)],
            triangles: vec![], // 0 triangles!
            ..Default::default()
        })
        .status
        .is_ok());

    let mut mesh_b = doc.get_mesh(MESH_B).unwrap().clone();
    mesh_b.base_positions[0] = Vec2::new(10.0, 10.0);
    assert!(doc.replace_mesh(mesh_b).status.is_ok());

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "GlueFaceless".to_string(),
        name: "GlueFaceless".to_string(),
        mesh_a_id: faceless_id.to_string(),
        mesh_b_id: MESH_B.to_string(),
        pairs: vec![GlueVertexPair {
            vertex_a: 101,
            vertex_b: 10,
            weight_a: 0.5,
            weight_b: 0.5,
        }],
        intensity: 0.0,
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: GLUE_ID.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.0,
            },
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 1.0,
            },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    let mut preview: PreviewValues = HashMap::new();
    preview.insert(PARAM_BS.to_string(), 1.0);
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());

    let faceless = frame
        .drawables
        .iter()
        .find(|d| d.id == faceless_id)
        .unwrap();
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    // d = (10, 10)
    // faceless += (5, 5) -> canvas (5, -5)
    // b -= (5, 5) -> canvas (5, -5)
    assert_eq!(faceless.positions[0], Vec2::new(5.0, 5.0));
    assert_eq!(b.positions[0], Vec2::new(5.0, 5.0));
}

#[test]
fn test_glue_blendshape_seam_a_b_a() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    let mut mesh_b = doc.get_mesh(MESH_B).unwrap().clone();
    mesh_b.base_positions[0] = Vec2::new(10.0, 20.0);
    assert!(doc.replace_mesh(mesh_b).status.is_ok());

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: MESH_A.to_string(),
        mesh_b_id: MESH_B.to_string(),
        pairs: vec![GlueVertexPair {
            vertex_a: 1,
            vertex_b: 10,
            weight_a: 0.5,
            weight_b: 0.5,
        }],
        intensity: 0.2,
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: GLUE_ID.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.0,
            },
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.6,
            },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    let mut preview: PreviewValues = HashMap::new();
    let mut frame_initial = DrawableFrame::default();
    let mut frame_mid = DrawableFrame::default();
    let mut frame_returned = DrawableFrame::default();

    // Start at A: 0.0
    preview.insert(PARAM_BS.to_string(), 0.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame_initial).is_ok());

    // Move to B: 1.0
    preview.insert(PARAM_BS.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame_mid).is_ok());

    // Return to A: 0.0
    preview.insert(PARAM_BS.to_string(), 0.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame_returned).is_ok());

    let a_init = frame_initial
        .drawables
        .iter()
        .find(|d| d.id == MESH_A)
        .unwrap();
    let a_ret = frame_returned
        .drawables
        .iter()
        .find(|d| d.id == MESH_A)
        .unwrap();
    let b_init = frame_initial
        .drawables
        .iter()
        .find(|d| d.id == MESH_B)
        .unwrap();
    let b_ret = frame_returned
        .drawables
        .iter()
        .find(|d| d.id == MESH_B)
        .unwrap();

    assert_eq!(a_init.positions, a_ret.positions);
    assert_eq!(b_init.positions, b_ret.positions);
}

#[test]
fn hidden_geometry_capture_preserves_visible_glue_result() {
    use kasane_core::{evaluate_frame, evaluation::evaluate_frame_including_hidden, DrawableFrame};
    let mut doc = create_base_document();
    let mut hidden = doc.get_mesh(MESH_A).unwrap().clone();
    hidden.enabled = false;
    hidden.base_positions[0] = Vec2::new(80.0, 30.0);
    assert!(doc.replace_mesh(hidden).status.is_ok());
    assert!(doc
        .create_glue(Glue {
            id: GLUE_ID.into(),
            mesh_a_id: MESH_A.into(),
            mesh_b_id: MESH_B.into(),
            runtime_id: "capture-glue".into(),
            name: "capture glue".into(),
            binding: None,
            intensity: 1.0,
            pairs: vec![GlueVertexPair {
                vertex_a: 1,
                vertex_b: 10,
                weight_a: 0.5,
                weight_b: 0.5
            }],
        })
        .status
        .is_ok());
    let mut normal = DrawableFrame::default();
    let mut captured = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &Default::default(), &mut normal).is_ok());
    assert!(evaluate_frame_including_hidden(&doc, &Default::default(), &mut captured).is_ok());
    let visible = |frame: &DrawableFrame| {
        frame
            .drawables
            .iter()
            .find(|d| d.id == MESH_B)
            .unwrap()
            .positions
            .clone()
    };
    assert_eq!(visible(&captured), visible(&normal));
}
