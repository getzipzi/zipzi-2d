use super::common::*;
use kasane_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, ParameterKind, Vec2,
};
use kasane_core::{RotationTransform, TransformData, WarpTransform};

#[test]
fn parameter_edits_validate_blend_dependents_atomically() {
    let mut doc = create_base_document();
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    let before = doc.clone();
    let mut p = doc.get_parameter(PARAM_BS).unwrap().clone();
    p.maximum = 0.5;
    assert!(!doc.replace_parameter(p).status.is_ok());
    assert!(doc.same_content(&before));
    let mut p = doc.get_parameter(PARAM_BS).unwrap().clone();
    p.kind = ParameterKind::Normal;
    assert!(!doc.replace_parameter(p).status.is_ok());
    assert!(doc.same_content(&before));

    // Constraints may reference normal parameters, as the Core does.
    assert!(doc
        .create_blend_constraint(BlendShapeConstraint {
            samples: Default::default(),
            id: CONSTRAINT.into(),
            parameter_id: PARAM_NORM.into(),
            keys: vec![-1.0, 1.0],
            weights: vec![1.0, 0.0],
        })
        .status
        .is_ok());
    let before = doc.clone();
    let mut p = doc.get_parameter(PARAM_NORM).unwrap().clone();
    p.minimum = 0.0;
    assert!(!doc.replace_parameter(p).status.is_ok());
    assert!(doc.same_content(&before));
}

#[test]
fn warp_grid_edits_validate_blend_dependents_atomically() {
    use kasane_core::types::{DeltaWarpKeyform, Transform};
    let mut doc = create_base_document();
    let warp_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    assert!(doc
        .create_transform(Transform {
            id: warp_id.into(),
            runtime_id: "Warp".into(),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![Vec2::default(); 4]
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    assert!(doc
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
        .status
        .is_ok());
    let before = doc.clone();
    let mut warp = doc.get_transform(warp_id).unwrap().clone();
    warp.warp_mut().unwrap().rows = 2;
    warp.warp_mut().unwrap().points = vec![Vec2::default(); 6];
    assert!(!doc.replace_transform(warp).status.is_ok());
    assert!(doc.same_content(&before));
    let mut warp = doc.get_transform(warp_id).unwrap().clone();
    warp.data = TransformData::Rotation(RotationTransform::default());
    assert!(!doc.replace_transform(warp).status.is_ok());
    assert!(doc.same_content(&before));
}

#[test]
fn topology_edit_updates_all_dependencies_atomically() {
    use std::collections::HashMap;
    let mut doc = create_base_document();
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
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
    assert!(doc.create_blend_binding(blend.clone()).status.is_ok());
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
    assert!(doc.create_glue(glue.clone()).status.is_ok());
    let original = doc.get_mesh(MESH_A).unwrap().clone();
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
    let revision = doc.revision();
    let failed = doc.replace_mesh_topology(
        mesh.clone(),
        None,
        vec![blend.clone()],
        vec![glue.clone()],
        mapping.clone(),
    );
    assert!(!failed.status.is_ok());
    assert_eq!(doc.get_mesh(MESH_A), Some(&original));
    assert_eq!(doc.revision(), revision);
    let mut updated = glue.clone();
    updated.pairs[0].vertex_a = Some(10);
    assert!(doc
        .replace_mesh_topology(
            mesh.clone(),
            None,
            vec![blend],
            vec![updated.clone()],
            mapping
        )
        .status
        .is_ok());
    assert_eq!(doc.get_mesh(MESH_A), Some(&mesh));
    assert_eq!(doc.get_glue(GLUE_ID), Some(&updated));
    assert_eq!(doc.revision(), revision + 1);
    assert_eq!(doc.glue_order(), &[GLUE_ID]);
    assert_eq!(doc.blend_binding_order(), &[BINDING_BS]);
}
