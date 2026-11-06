use super::common::*;
use zipzi_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, Parameter, ParameterKind, Vec2,
};

#[test]
fn blend_bindings_share_sample_identities_but_new_tables_refresh_them() {
    let doc = create_base_document();
    let table = BlendShapeKeyTable {
        id: KEY_TABLE.into(),
        parameter_id: PARAM_BS.into(),
        keys: vec![0., 1.],
        base_key_idx: 0,
        samples: Default::default(),
        base_sample_id: Default::default(),
    };
    let doc = prepare(&doc, |b| b.create_blend_key_table(table.clone()))
        .unwrap()
        .state;
    let identities = doc.binding_identities();
    let before = doc.clone(); // Retain the previous index, ruling out address reuse.
    let mut binding = BlendShapeBinding {
        id: BINDING_BS.into(),
        target_id: MESH_A.into(),
        target_kind: BlendShapeTargetKind::Mesh,
        key_table_id: KEY_TABLE.into(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![Vec2::default(); 3],
                ..Default::default()
            },
            DeltaMeshKeyform {
                positions: vec![Vec2::new(1., 0.); 3],
                ..Default::default()
            },
        ]),
    };
    let doc = prepare(&doc, |b| b.create_blend_binding(binding.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.binding_identities(), identities);
    if let DeltaKeyforms::Mesh(forms) = &mut binding.keyforms {
        forms[1].positions[0].x = 2.;
    }
    let doc = prepare(&doc, |b| b.replace_blend_binding(binding.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.binding_identities(), identities);
    binding.id = identities[0].id.clone();
    assert_eq!(
        prepare(&doc, |b| b.create_blend_binding(binding.clone()))
            .unwrap_err()
            .code,
        "DUPLICATE_ID"
    );

    let mut another_table = table;
    another_table.id = uuid::Uuid::new_v4().to_string();
    let doc = prepare(&doc, |b| b.create_blend_key_table(another_table.clone()))
        .unwrap()
        .state;
    let sample = doc.blend_key_table(&another_table.id).unwrap().samples[0]
        .id
        .clone();
    assert_eq!(
        doc.binding_identities()
            .into_iter()
            .find(|value| value.id == sample)
            .unwrap()
            .owner,
        another_table.id
    );
    assert_eq!(doc.binding_identities().len(), identities.len() + 2);
    binding.id = sample;
    assert_eq!(
        prepare(&doc, |b| b.create_blend_binding(binding))
            .unwrap_err()
            .code,
        "DUPLICATE_ID"
    );
    assert_eq!(before.binding_identities(), identities);
}

#[test]
fn test_blendshape_key_table_and_constraint_crud() {
    let doc = create_base_document();

    // Key table targeting normal parameter should fail
    let bad_table = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_NORM.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(prepare(&doc, |b| b.create_blend_key_table(bad_table)).is_err());

    // Valid key table
    let table = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    let doc = prepare(&doc, |b| b.create_blend_key_table(table.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.blend_key_table_order(), &[KEY_TABLE]);
    let stored = doc.blend_key_table(KEY_TABLE).unwrap();
    assert_eq!(stored.keys, table.keys);
    assert_eq!(stored.base_sample_id, stored.samples[table.base_key_idx].id);
    assert_ne!(stored.samples[0].id, stored.samples[1].id);

    // Valid constraint
    let constraint = BlendShapeConstraint {
        samples: Default::default(),
        id: CONSTRAINT.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![0.0, 1.0],
    };
    let doc = prepare(&doc, |b| b.create_blend_constraint(constraint.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.blend_constraint_order(), &[CONSTRAINT]);
    let stored = doc.blend_constraint(CONSTRAINT).unwrap();
    assert_eq!(stored.keys, constraint.keys);
    assert_eq!(stored.weights, constraint.weights);
    assert_ne!(stored.samples[0].id, stored.samples[1].id);

    // Erase parameter should fail because it's referenced by key table and constraint
    assert!(prepare(&doc, |b| b.remove_parameter(PARAM_BS)).is_err());
    assert_eq!(
        doc.blend_key_table(KEY_TABLE).unwrap().parameter_id,
        PARAM_BS
    );
    assert_eq!(
        doc.blend_constraint(CONSTRAINT).unwrap().parameter_id,
        PARAM_BS
    );
}

#[test]
fn test_blendshape_binding_and_mesh_topology_guard() {
    let doc = create_base_document();

    let table = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    let doc = prepare(&doc, |b| b.create_blend_key_table(table))
        .unwrap()
        .state;

    let constraint = BlendShapeConstraint {
        samples: Default::default(),
        id: CONSTRAINT.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![0.0, 1.0],
    };
    let doc = prepare(&doc, |b| b.create_blend_constraint(constraint))
        .unwrap()
        .state;

    let mut binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: MESH_A.to_string(),
        target_kind: BlendShapeTargetKind::Mesh,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![CONSTRAINT.to_string()],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(0.0, 0.0),
                    Vec2::new(0.0, 0.0),
                ],
                ..Default::default()
            },
            DeltaMeshKeyform {
                positions: vec![
                    Vec2::new(1.0, 0.0),
                    Vec2::new(2.0, 0.0),
                    Vec2::new(3.0, 0.0),
                ],
                ..Default::default()
            },
        ]),
    };
    if let DeltaKeyforms::Mesh(forms) = &mut binding.keyforms {
        for (form, sample) in forms
            .iter_mut()
            .zip(&doc.blend_key_table(KEY_TABLE).unwrap().samples)
        {
            form.sample_id = sample.id.clone();
        }
    }
    let doc = prepare(&doc, |b| b.create_blend_binding(binding.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.blend_binding_order(), &[BINDING_BS]);
    assert_eq!(doc.blend_binding(BINDING_BS), Some(&binding));

    // Trying to replace mesh with different vertex count without updating keyforms must be rejected
    let mut modified_mesh = doc.mesh(MESH_A).unwrap().clone();
    modified_mesh.vertex_ids = vec![1, 2, 3, 4];
    modified_mesh.base_positions.push(Vec2::new(5.0, 5.0));
    modified_mesh.uvs.push(Vec2::new(0.5, 0.5));
    modified_mesh.triangles = vec![[1, 2, 3], [2, 3, 4]];
    assert_eq!(
        prepare(&doc, |b| b.replace_mesh(modified_mesh))
            .unwrap_err()
            .code,
        "INVALID_LENGTH"
    );
}

#[test]
fn test_glue_crud_and_references() {
    let doc = create_base_document();

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: (MESH_A.to_string()).into(),
        mesh_b_id: (MESH_B.to_string()).into(),
        pairs: vec![
            GlueVertexPair {
                vertex_a: Some(1),
                vertex_b: Some(10),
                weight_a: 0.5,
                weight_b: 0.5,
                ..Default::default()
            },
            GlueVertexPair {
                vertex_a: Some(2),
                vertex_b: Some(20),
                weight_a: 0.5,
                weight_b: 0.5,
                ..Default::default()
            },
        ],
        intensity: 1.0,
        binding: None,
    };
    let doc = prepare(&doc, |b| b.create_glue(glue.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.glue_order(), &[GLUE_ID]);
    assert_eq!(doc.glue(GLUE_ID), Some(&glue));
    assert_eq!(doc.glues_for_mesh(MESH_A).len(), 1);

    // References: MESH_A cannot be erased while glued
    assert!(prepare(&doc, |b| b.remove_mesh(MESH_A)).is_err());
    assert_eq!(doc.glues_for_mesh(MESH_A)[0].id, GLUE_ID);

    // Mesh topology change that drops a glued vertex must be rejected
    let mut modified_mesh = doc.mesh(MESH_A).unwrap().clone();
    modified_mesh.vertex_ids = vec![2, 3, 4]; // vertex 1 dropped!
    modified_mesh.base_positions = vec![
        Vec2::new(10.0, 0.0),
        Vec2::new(0.0, 10.0),
        Vec2::new(5.0, 5.0),
    ];
    modified_mesh.uvs = vec![
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.5, 0.5),
    ];
    modified_mesh.triangles = vec![[2, 3, 4]];
    assert_eq!(
        prepare(&doc, |b| b.replace_mesh(modified_mesh))
            .unwrap_err()
            .code,
        "MISSING_VERTEX"
    );

    let doc = prepare(&doc, |b| b.remove_glue(GLUE_ID)).unwrap().state;
    assert!(doc.glue_order().is_empty());
    assert!(doc.glue(GLUE_ID).is_none());

    let doc = prepare(&doc, |b| b.remove_mesh(MESH_A)).unwrap().state;
    assert!(doc.mesh(MESH_A).is_none());
}

#[test]
fn test_blendshape_evaluation() {
    use std::collections::HashMap;
    use zipzi_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};

    let mut builder = create_base_document().builder();

    let param_bs2 = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    builder
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
        .unwrap();

    // Create blend key table on PARAM_BS (range 0.0..1.0, keys [0.0, 1.0], base_key = 0)
    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    builder.create_blend_key_table(bkt).unwrap();

    // Create a constraint on param_bs2 (range 0.0..1.0):
    // at 0.0 weight is 1.0, at 1.0 weight is 0.0
    let bsc = BlendShapeConstraint {
        samples: Default::default(),
        id: CONSTRAINT.to_string(),
        parameter_id: param_bs2.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.0],
    };
    builder.create_blend_constraint(bsc).unwrap();

    // Create blend binding for MESH_A:
    // base key (0.0): delta is zero
    // key 1 (1.0): dx = 5.0, dy = 10.0, draw_order = 50.0
    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: MESH_A.to_string(),
        target_kind: BlendShapeTargetKind::Mesh,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![CONSTRAINT.to_string()],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![Vec2::default(); 3],
                ..Default::default()
            },
            DeltaMeshKeyform {
                sample_id: Default::default(),
                positions: vec![
                    Vec2::new(5.0, 10.0),
                    Vec2::new(5.0, 10.0),
                    Vec2::new(5.0, 10.0),
                ],
                opacity: None,
                draw_order: Some(50.0),
                multiply: None,
                screen: None,
            },
        ]),
    };
    builder.create_blend_binding(binding).unwrap();

    // Case 1: PARAM_BS = 0.0 (base key) -> no delta
    let mut preview: PreviewValues = HashMap::new();
    preview.insert(PARAM_BS.to_string(), 0.0);
    preview.insert(param_bs2.to_string(), 0.0); // constraint weight = 1.0
    let doc = builder.freeze().unwrap().state;
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(0.0, 0.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(10.0, 0.0));
    assert_eq!(mesh_a.draw_order, 0);

    // Case 2: PARAM_BS = 1.0, constraint param_bs2 = 0.0 (weight 1.0) -> full delta (5, 10)
    // Note: doc.source_canvas().unwrap().flag & 1 == 0 inverts Y for final output
    preview.insert(PARAM_BS.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(5.0, 10.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(15.0, 10.0));
    assert_eq!(mesh_a.draw_order, 50);

    // Case 3: PARAM_BS = 0.5, constraint = 0.0 (weight 1.0) -> half delta (2.5, 5.0)
    preview.insert(PARAM_BS.to_string(), 0.5);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(2.5, 5.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(12.5, 5.0));
    assert_eq!(mesh_a.draw_order, 25);

    // Case 4: PARAM_BS = 1.0, constraint param_bs2 = 0.5 (halfway between 0 and 1 -> weight 0.5)
    // Effective weight = 1.0 * 0.5 = 0.5 -> delta is (2.5, 5.0)
    preview.insert(PARAM_BS.to_string(), 1.0);
    preview.insert(param_bs2.to_string(), 0.5);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(2.5, 5.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(12.5, 5.0));
    assert_eq!(mesh_a.draw_order, 25);

    // Case 5: PARAM_BS = 1.0, constraint param_bs2 = 1.0 (weight 0.0) -> 0 delta
    preview.insert(PARAM_BS.to_string(), 1.0);
    preview.insert(param_bs2.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(0.0, 0.0));
    assert_eq!(mesh_a.draw_order, 0);
}

#[test]
fn test_glue_evaluation() {
    use std::collections::HashMap;
    use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};

    let mut builder = create_base_document().builder();

    let mut mesh_b_mod = builder.mesh(MESH_B).unwrap().clone();
    mesh_b_mod.base_positions[0] = Vec2::new(10.0, 20.0);
    builder.replace_mesh(mesh_b_mod).unwrap();

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: (MESH_A.to_string()).into(),
        mesh_b_id: (MESH_B.to_string()).into(),
        pairs: vec![GlueVertexPair {
            vertex_a: Some(1),  // at (0, 0)
            vertex_b: Some(10), // at (10, 20)
            weight_a: 0.5,
            weight_b: 0.5,
            ..Default::default()
        }],
        intensity: 1.0,
        binding: None,
    };
    builder.create_glue(glue).unwrap();

    let doc = builder.freeze().unwrap().state;
    let mut frame = DrawableFrame::default();
    let preview = HashMap::new();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());

    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let mesh_b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();

    // d = (10 - 0, 20 - 0) = (10, 20)
    // p_a = (0, 0) + (10, 20) * 0.5 = (5, 10) -> canvas y inverted: (5, -10)
    // p_b = (10, 20) - (10, 20) * 0.5 = (5, 10) -> canvas y inverted: (5, -10)
    assert_eq!(mesh_a.positions[0], Vec2::new(5.0, 10.0));
    assert_eq!(mesh_b.positions[0], Vec2::new(5.0, 10.0));

    // Test with intensity 0.5
    let mut glue_half = doc.glue(GLUE_ID).unwrap().clone();
    glue_half.intensity = 0.5;
    let doc = prepare(&doc, |b| b.replace_glue(glue_half)).unwrap().state;

    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let mesh_b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();

    // p_a = (0, 0) + (10, 20) * (0.5 * 0.5) = (2.5, 5.0) -> canvas y inverted: (2.5, -5.0)
    // p_b = (10, 20) - (10, 20) * (0.5 * 0.5) = (7.5, 15.0) -> canvas y inverted: (7.5, -15.0)
    assert_eq!(mesh_a.positions[0], Vec2::new(2.5, 5.0));
    assert_eq!(mesh_b.positions[0], Vec2::new(7.5, 15.0));
}
