use super::common::*;
use zipzi_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, ParameterKind, Vec2,
};
use zipzi_core::{RotationTransform, TransformData, WarpTransform};

#[test]
fn parameter_edits_validate_blend_dependents_atomically() {
    let mut builder = create_base_document().builder();
    builder
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .unwrap();
    let doc = builder.freeze().unwrap().state;
    let mut p = doc.parameter(PARAM_BS).unwrap().clone();
    p.maximum = 0.5;
    assert!(prepare(&doc, |b| b.replace_parameter(p)).is_err());
    let mut p = doc.parameter(PARAM_BS).unwrap().clone();
    p.kind = ParameterKind::Normal;
    assert!(prepare(&doc, |b| b.replace_parameter(p)).is_err());

    // Constraints may reference normal parameters.
    let mut builder = doc.builder();
    builder
        .create_blend_constraint(BlendShapeConstraint {
            samples: Default::default(),
            id: CONSTRAINT.into(),
            parameter_id: PARAM_NORM.into(),
            keys: vec![-1.0, 1.0],
            weights: vec![1.0, 0.0],
        })
        .unwrap();
    let doc = builder.freeze().unwrap().state;
    let mut p = doc.parameter(PARAM_NORM).unwrap().clone();
    p.minimum = 0.0;
    assert!(prepare(&doc, |b| b.replace_parameter(p)).is_err());
}

#[test]
fn warp_grid_edits_validate_blend_dependents_atomically() {
    use zipzi_core::types::{DeltaWarpKeyform, Transform};
    let mut builder = create_base_document().builder();
    let warp_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    builder
        .create_transform(Transform {
            id: warp_id.into(),
            runtime_id: "Warp".into(),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![Vec2::default(); 4],
            }),
            ..Default::default()
        })
        .unwrap();
    builder
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .unwrap();
    builder
        .create_blend_binding(BlendShapeBinding {
            id: BINDING_BS.into(),
            target_id: warp_id.into(),
            target_kind: BlendShapeTargetKind::Warp,
            key_table_id: KEY_TABLE.into(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Warp(vec![
                DeltaWarpKeyform {
                    points: vec![Vec2::default(); 4],
                    ..Default::default()
                };
                2
            ]),
        })
        .unwrap();
    let doc = builder.freeze().unwrap().state;
    let mut warp = doc.transform(warp_id).unwrap().clone();
    warp.warp_mut().unwrap().rows = 2;
    warp.warp_mut().unwrap().points = vec![Vec2::default(); 6];
    assert!(prepare(&doc, |b| b.replace_transform(warp)).is_err());
    let mut warp = doc.transform(warp_id).unwrap().clone();
    warp.data = TransformData::Rotation(RotationTransform::default());
    assert!(prepare(&doc, |b| b.replace_transform(warp)).is_err());
}

#[test]
fn topology_edit_updates_all_dependencies_atomically() {
    use std::collections::HashMap;
    let mut builder = create_base_document().builder();
    builder
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .unwrap();
    let blend = BlendShapeBinding {
        id: BINDING_BS.into(),
        target_id: MESH_A.into(),
        target_kind: BlendShapeTargetKind::Mesh,
        key_table_id: KEY_TABLE.into(),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![Vec2::default(); 3],
                ..Default::default()
            };
            2
        ]),
    };
    builder.create_blend_binding(blend.clone()).unwrap();
    let glue = Glue {
        id: GLUE_ID.into(),
        runtime_id: "TopologyGlue".into(),
        name: "Glue".into(),
        mesh_a_id: MESH_A.into(),
        mesh_b_id: MESH_B.into(),
        pairs: vec![GlueVertexPair {
            vertex_a: Some(1),
            vertex_b: Some(10),
            weight_a: 0.5,
            weight_b: 0.5,
            ..Default::default()
        }],
        intensity: 1.0,
        binding: None,
    };
    builder.create_glue(glue.clone()).unwrap();
    let doc = builder.freeze().unwrap().state;
    let blend = doc.blend_binding(BINDING_BS).unwrap().clone();
    let glue = doc.glue(GLUE_ID).unwrap().clone();
    let original = doc.mesh(MESH_A).unwrap().clone();
    let mut mesh = original.clone();
    mesh.vertex_ids[0] = 10;
    for tri in &mut mesh.triangles {
        for v in tri {
            if *v == 1 {
                *v = 10;
            }
        }
    }
    let mapping = HashMap::from([(1, Some(10)), (2, Some(2)), (3, Some(3))]);
    let failed = prepare(&doc, |b| {
        b.replace_mesh_topology(
            mesh.clone(),
            None,
            vec![blend.clone()],
            vec![glue.clone()],
            mapping.clone(),
        )
    });
    assert!(failed.is_err());
    assert_eq!(doc.mesh(MESH_A), Some(&original));
    let mut updated = glue.clone();
    updated.pairs[0].vertex_a = Some(10);
    let edited = prepare(&doc, |b| {
        b.replace_mesh_topology(
            mesh.clone(),
            None,
            vec![blend],
            vec![updated.clone()],
            mapping,
        )
    })
    .unwrap();
    assert_eq!(edited.writes.records().len(), 2);
    let doc = edited.state;
    assert_eq!(doc.mesh(MESH_A), Some(&mesh));
    assert_eq!(doc.glue(GLUE_ID), Some(&updated));
    assert_eq!(doc.glue_order(), &[GLUE_ID]);
    assert_eq!(doc.blend_binding_order(), &[BINDING_BS]);
}
