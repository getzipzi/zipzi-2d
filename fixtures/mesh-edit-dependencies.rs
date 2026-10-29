//! Synthetic authoring fixture shared by SDK, Runtime and Editor tests.
use zipzi_core::*;
use zipzi_sdk::{EditSession, SdkError};
pub fn id(n: u32) -> String {
    format!("a8210000-0000-4000-8000-{n:012x}")
}
#[allow(dead_code)]
pub fn install(e: &mut EditSession<'_>) -> Result<(), SdkError> {
    e.create_transform(Transform {
        id: id(3),
        data: TransformData::Rotation(RotationTransform {
            pose: RotationPose {
                origin: Vec2::new(11., 7.).into(),
                angle: 27.,
                scale: 1.3,
                ..Default::default()
            },
            ..Default::default()
        }),
        ..Default::default()
    })?;
    let mut mesh =
        zipzi_sdk::rectangle_mesh(&id(2), "Mixed mesh", "", Vec2::default(), Vec2::new(1., 1.))?;
    mesh.vertex_ids = vec![10, 30, 70, 90, 120];
    mesh.uvs = vec![
        Vec2::new(0.1, 0.1),
        Vec2::new(0.9, 0.1),
        Vec2::new(0.9, 0.9),
        Vec2::new(0.1, 0.9),
        Vec2::new(0.4, 0.45),
    ];
    // Intentionally non-affine, including an overlapping base region.
    mesh.base_positions = vec![
        Vec2::new(0., 0.),
        Vec2::new(3., 0.),
        Vec2::new(2., 4.),
        Vec2::new(0., 2.),
        Vec2::new(1.8, 1.7),
    ];
    mesh.triangles = vec![[10, 30, 120], [30, 70, 120], [70, 90, 120], [90, 10, 120]];
    mesh.deformer_id = id(3);
    e.create_mesh(mesh.clone())?;
    mesh.id = id(4);
    mesh.runtime_id = id(4);
    mesh.deformer_id.clear();
    e.create_mesh(mesh)?;
    e.create_parameter(Parameter {
        id: id(5),
        minimum: 0.,
        maximum: 1.,
        ..Default::default()
    })?;
    let base = e
        .candidate_document()
        .get_mesh(&id(2))
        .unwrap()
        .base_positions
        .clone();
    e.create_binding(MeshBinding {
        id: id(6),
        mesh_id: id(2),
        axes: vec![BindingAxis {
            parameter_id: id(5),
            keys: vec![0., 1.],
            ..Default::default()
        }],
        keyforms: vec![
            MeshKeyform {
                keys: vec![0.],
                positions: base.clone(),
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![1.],
                positions: base
                    .iter()
                    .enumerate()
                    .map(|(i, p)| Vec2::new(p.x + i as f32 * 2., p.y - 3.))
                    .collect(),
                ..Default::default()
            },
        ],
    })?;
    for n in [10, 20] {
        e.create_parameter(Parameter {
            id: id(n),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(n + 1),
            parameter_id: id(n),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: vec![],
            base_sample_id: String::new(),
        })?;
        e.create_blend_binding(BlendShapeBinding {
            id: id(n + 2),
            target_id: id(2),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: id(n + 1),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::default(); 5],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: (0..5)
                        .map(|i| Vec2::new(i as f32 + n as f32, (i * i) as f32))
                        .collect(),
                    opacity: Some(0.2),
                    ..Default::default()
                },
            ]),
        })?;
    }
    for (n, other, a, b) in [(30, 4, Some(10), Some(10)), (32, 2, Some(70), None)] {
        e.create_glue(Glue {
            id: id(n),
            mesh_a_id: id(2).into(),
            mesh_b_id: id(other).into(),
            intensity: 0.,
            pairs: vec![GlueVertexPair {
                id: id(n + 1),
                vertex_a: a,
                vertex_b: b,
                weight_a: 0.3,
                weight_b: 0.7,
            }],
            ..Default::default()
        })?;
    }
    Ok(())
}
