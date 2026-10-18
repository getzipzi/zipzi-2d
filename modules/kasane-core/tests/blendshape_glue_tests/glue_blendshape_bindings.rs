use super::common::*;
use kasane_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaGlueKeyform, DeltaKeyforms, Glue, GlueVertexPair, Parameter, ParameterKind, Vec2,
};

#[test]
fn test_glue_blendshape_binding_crud_and_erase_references() {
    let mut doc = create_base_document();

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
        intensity: 0.5,
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
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
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: 0.3 },
        ]),
    };
    assert!(doc.create_blend_binding(binding.clone()).status.is_ok());
    assert_eq!(doc.blend_bindings_for_target(GLUE_ID), vec![&binding]);

    // Glue cannot be erased while referenced by BlendShapeBinding
    let erase_res = doc.erase_object(GLUE_ID);
    assert!(!erase_res.status.is_ok());
    assert!(erase_res.referrers.contains(&BINDING_BS.to_string()));

    // Erasing the BlendShapeBinding succeeds
    assert!(doc.erase_object(BINDING_BS).status.is_ok());
    assert!(doc.blend_bindings_for_target(GLUE_ID).is_empty());

    // Now erasing the glue succeeds
    assert!(doc.erase_object(GLUE_ID).status.is_ok());
}

#[test]
fn test_glue_blendshape_and_ordinary_coexistence() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    // Base positions:
    // Mesh A v1: (0, 0)
    // Mesh B v10: (10, 20)
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
        intensity: 0.3, // base intensity
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
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
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: 0.4 }, // delta +0.4 -> total 0.7
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    // Case 1: PARAM_BS = 0.0 -> intensity = 0.3
    let mut preview: PreviewValues = HashMap::new();
    preview.insert(PARAM_BS.to_string(), 0.0);
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    // d = (10, 20)
    // disp_a = (10, 20) * 0.5 * 0.3 = (1.5, 3.0) -> canvas y inverted: (1.5, -3.0)
    // disp_b = -(10, 20) * 0.5 * 0.3 = (-1.5, -3.0) -> pos: (8.5, 17.0) -> canvas: (8.5, -17.0)
    assert_eq!(a.positions[0], Vec2::new(1.5, 3.0));
    assert_eq!(b.positions[0], Vec2::new(8.5, 17.0));

    // Case 2: PARAM_BS = 1.0 -> intensity = 0.3 + 0.4 = 0.7
    preview.insert(PARAM_BS.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    // disp_a = (10, 20) * 0.5 * 0.7 = (3.5, 7.0) -> canvas: (3.5, -7.0)
    // disp_b = pos (10 - 3.5, 20 - 7.0) = (6.5, 13.0) -> canvas: (6.5, -13.0)
    assert_vec2_near(a.positions[0], Vec2::new(3.5, 7.0), 1e-5);
    assert_vec2_near(b.positions[0], Vec2::new(6.5, 13.0), 1e-5);
}

#[test]
fn test_glue_blendshape_multi_binding_and_shared_constraint() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    let param_bs2 = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    assert!(doc
        .create_parameter(Parameter {
            id: param_bs2.to_string(),
            runtime_id: "ParamBS2".to_string(),
            name: "ParamBS2".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::BlendShape,
            repeat: false,
        })
        .status
        .is_ok());

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
        intensity: 0.0,
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let kt1 = BlendShapeKeyTable {
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(kt1).status.is_ok());

    let kt2_id = "77777777-7777-4777-8777-777777777778";
    let kt2 = BlendShapeKeyTable {
        id: kt2_id.to_string(),
        parameter_id: param_bs2.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(kt2).status.is_ok());

    // Constraint on PARAM_NORM (minimum: -1.0, maximum: 1.0): weight 1.0 at 0.0, 0.5 at 1.0
    let constraint = BlendShapeConstraint {
        id: CONSTRAINT.to_string(),
        parameter_id: PARAM_NORM.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.5],
    };
    assert!(doc.create_blend_constraint(constraint).status.is_ok());

    // Binding 1: delta +0.2
    let b1 = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: GLUE_ID.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![CONSTRAINT.to_string()],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: 0.2 },
        ]),
    };
    assert!(doc.create_blend_binding(b1).status.is_ok());

    // Binding 2: delta +0.4
    let b2_id = "99999999-9999-4999-8999-999999999998";
    let b2 = BlendShapeBinding {
        id: b2_id.to_string(),
        target_id: GLUE_ID.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: kt2_id.to_string(),
        constraint_ids: vec![CONSTRAINT.to_string()],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: 0.4 },
        ]),
    };
    assert!(doc.create_blend_binding(b2).status.is_ok());

    let mut preview: PreviewValues = HashMap::new();
    let mut frame = DrawableFrame::default();

    // PARAM_BS = 1.0, param_bs2 = 1.0, PARAM_NORM = 0.0 (constraint weight = 1.0)
    // intensity = 0.0 + 0.2*1.0 + 0.4*1.0 = 0.6
    preview.insert(PARAM_BS.to_string(), 1.0);
    preview.insert(param_bs2.to_string(), 1.0);
    preview.insert(PARAM_NORM.to_string(), 0.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    // (10, 20) * 0.5 * 0.6 = (3.0, 6.0) -> canvas: (3.0, -6.0)
    assert_vec2_near(a.positions[0], Vec2::new(3.0, 6.0), 1e-5);

    // PARAM_NORM = 1.0 -> constraint weight = 0.5
    // intensity = 0.0 + 0.2*0.5 + 0.4*0.5 = 0.3
    preview.insert(PARAM_NORM.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    // (10, 20) * 0.5 * 0.3 = (1.5, 3.0) -> canvas: (1.5, -3.0)
    assert_vec2_near(a.positions[0], Vec2::new(1.5, 3.0), 1e-5);
}

#[test]
fn test_glue_blendshape_negative_and_excess_delta_clamping() {
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
        intensity: 0.2, // base intensity 0.2
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 0.5, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    // Delta keyforms: at 0.5 delta is -0.5 (raw 0.2 - 0.5 = -0.3, should clamp to 0.0)
    // At 1.0 delta is +1.2 (raw 0.2 + 1.2 = 1.4, should clamp to 1.0)
    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: GLUE_ID.to_string(),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: -0.5 },
            DeltaGlueKeyform { intensity: 1.2 },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    let mut preview: PreviewValues = HashMap::new();
    let mut frame = DrawableFrame::default();

    // At 0.5: intensity clamped to 0.0 -> no displacement at all!
    preview.insert(PARAM_BS.to_string(), 0.5);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    assert_eq!(a.positions[0], Vec2::new(0.0, 0.0));
    assert_eq!(b.positions[0], Vec2::new(10.0, 20.0));

    // At 1.0: intensity clamped to 1.0 -> full displacement!
    preview.insert(PARAM_BS.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();
    // full 1.0 displacement: (10, 20) * 0.5 = (5.0, 10.0) -> canvas: (5.0, -10.0)
    assert_eq!(a.positions[0], Vec2::new(5.0, 10.0));
    assert_eq!(b.positions[0], Vec2::new(5.0, 10.0));
}
