mod support;
use std::collections::{HashMap, HashSet};
use support::{prepare, project_json};
use zipzi_core::state::*;
use zipzi_core::{RotationTransform, TransformData, WarpTransform};
use zipzi_project::{ProjectIo, ResourceStore, ResourceView};

use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::types::{
    BindingAxis, Canvas, Mesh, MeshBinding, MeshKeyform, Parameter, Part, RotationPose, Transform,
    Vec2,
};
use zipzi_moc3::encode_moc3;

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u64() as usize % (max - min + 1))
    }

    fn next_f32(&mut self, min: f32, max: f32) -> f32 {
        let t = (self.next_u64() & 0xFFFF_FFFF) as f32 / 4294967295.0;
        min + t * (max - min)
    }

    fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }

    fn choose<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            Some(&slice[self.next_range(0, slice.len() - 1)])
        }
    }
}

fn make_uuid(kind: u16, id: u32) -> String {
    format!("{kind:04x}{id:04x}-4111-4111-8111-111111111111")
}

fn build_initial_document() -> (DocumentState, ResourceView) {
    let mut doc = DocumentState::new(
        make_uuid(0x0001, 1),
        Canvas::new(1280., 720., Vec2::new(640., 360.), 100.).into(),
    )
    .unwrap();
    let store = ResourceStore::default();
    let image = store
        .prepare_rgba(512, 512, vec![255; 512 * 512 * 4].into())
        .unwrap();
    let resources = ResourceView::default().with_images([image.clone()]);
    let asset1 = make_uuid(0x0002, 1);
    let asset2 = make_uuid(0x0002, 2);
    doc = prepare(&doc, |builder| {
        for id in [&asset1, &asset2] {
            builder.create_asset(AssetRecord {
                id: id.clone(),
                name: id.clone(),
                content: AssetContent::Image {
                    resource: image.key(),
                    width: 512,
                    height: 512,
                },
            })?;
        }
        Ok(())
    });

    let mut param_ids = Vec::new();
    for i in 0..2 {
        let pid = make_uuid(0x0003, i + 1);
        param_ids.push(pid.clone());
        doc = prepare(&doc, |builder| {
            builder.create_parameter(Parameter {
                id: pid.clone(),
                runtime_id: format!("Param_{i}"),
                name: format!("Parameter {i}"),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                decimal_places: 4,
                ..Default::default()
            })
        });
    }

    let root_part = make_uuid(0x0004, 1);
    doc = prepare(&doc, |builder| {
        builder.create_part(Part {
            id: root_part.clone(),
            runtime_id: "RootPart".to_string(),
            name: "Root Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 0.0,
        })
    });

    let rot_id = make_uuid(0x0005, 1);
    doc = prepare(&doc, |builder| {
        builder.create_transform(Transform {
            id: rot_id.clone(),
            runtime_id: "RotTransform".to_string(),
            name: "Rot Transform".to_string(),
            part_id: zipzi_core::PartId::optional(root_part.clone()),
            parent_id: zipzi_core::TransformId::optional(String::new()),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 0.0,
                pose: RotationPose {
                    origin: Vec2::new(640.0, 360.0).into(),
                    angle: 0.0,
                    scale: 1.0,
                    reflect_x: false,
                    reflect_y: false,
                },
            }),
            ..Default::default()
        })
    });

    let warp_id = make_uuid(0x0005, 2);
    let mut warp = Transform {
        id: warp_id.clone(),
        runtime_id: "WarpTransform".to_string(),
        name: "Warp Transform".to_string(),
        part_id: zipzi_core::PartId::optional(root_part.clone()),
        parent_id: zipzi_core::TransformId::optional(rot_id.clone()),
        data: TransformData::Warp(WarpTransform {
            bezier: None,
            rows: 2,
            columns: 2,
            quad: true,
            points: Vec::new(),
        }),
        ..Default::default()
    };
    for r in 0..=2 {
        for c in 0..=2 {
            warp.warp_mut()
                .unwrap()
                .points
                .push(Vec2::new(c as f32 * 50.0 - 50.0, r as f32 * 50.0 - 50.0));
        }
    }
    doc = prepare(&doc, |builder| builder.create_transform(warp));

    let mesh_id = make_uuid(0x0006, 1);
    let mesh = Mesh {
        id: mesh_id.clone(),
        runtime_id: "Mesh_1".to_string(),
        name: "Mesh 1".to_string(),
        texture_asset_id: (asset1).into(),
        part_id: root_part,
        deformer_id: warp_id,
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(-20.0, 20.0),
            Vec2::new(-20.0, -20.0),
            Vec2::new(20.0, -20.0),
            Vec2::new(20.0, 20.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        ],
        triangles: vec![[1, 2, 3], [1, 3, 4]],
        draw_order: Some(1.0),
        ..Default::default()
    };
    doc = prepare(&doc, |builder| builder.create_mesh(mesh));

    let binding_id = make_uuid(0x0007, 1);
    let binding = MeshBinding {
        id: binding_id,
        mesh_id: mesh_id.clone(),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: param_ids[0].clone(),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: vec![
            MeshKeyform {
                keys: vec![-1.0],
                positions: vec![
                    Vec2::new(-25.0, 20.0),
                    Vec2::new(-25.0, -20.0),
                    Vec2::new(15.0, -20.0),
                    Vec2::new(15.0, 20.0),
                ],
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![0.0],
                positions: vec![
                    Vec2::new(-20.0, 20.0),
                    Vec2::new(-20.0, -20.0),
                    Vec2::new(20.0, -20.0),
                    Vec2::new(20.0, 20.0),
                ],
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![1.0],
                positions: vec![
                    Vec2::new(-15.0, 20.0),
                    Vec2::new(-15.0, -20.0),
                    Vec2::new(25.0, -20.0),
                    Vec2::new(25.0, 20.0),
                ],
                ..Default::default()
            },
        ],
    };
    doc = prepare(&doc, |builder| builder.create_binding(binding));

    (doc, resources)
}

fn verify_invariants(doc: &DocumentState, resources: &ResourceView, label: &str) {
    // Invariant 1: Parts hierarchy is a valid DAG (no cycles, no dangling parent)
    for p_id in doc.part_order() {
        let p = doc.part(p_id).expect("part in order must exist");
        if !p.parent_id.is_empty() {
            assert!(
                doc.part(&p.parent_id).is_some(),
                "Dangling part parent: {p_id} -> {}",
                p.parent_id
            );
        }
        let mut visited = HashSet::new();
        visited.insert(p_id.clone());
        let mut curr = p.parent_id.clone();
        while !curr.is_empty() {
            assert!(
                visited.insert(curr.clone()),
                "Cycle detected in part hierarchy starting from {p_id} at {label}"
            );
            curr = doc
                .part(&curr)
                .expect("ancestor must exist")
                .parent_id
                .clone();
        }
    }

    // Invariant 2: Transform hierarchy is a valid DAG
    for t_id in doc.transform_order() {
        let t = doc.transform(t_id).expect("transform in order must exist");
        assert!(
            doc.part(t.part()).is_some(),
            "Transform {t_id} points to missing part {}",
            t.part()
        );
        if !t.parent_id.is_none() {
            assert!(
                doc.transform(t.parent()).is_some(),
                "Transform {t_id} points to missing transform parent {}",
                t.parent()
            );
        }
        let mut visited = HashSet::new();
        visited.insert(t_id.clone());
        let mut curr = t.parent();
        while !curr.is_empty() {
            assert!(
                visited.insert(curr.to_owned()),
                "Cycle in transform hierarchy at {label}"
            );
            curr = doc
                .transform(curr)
                .expect("transform ancestor must exist")
                .parent();
        }
    }

    // Invariant 3: Mesh integrity
    for m_id in doc.mesh_order() {
        let m = doc.mesh(m_id).expect("mesh in order must exist");
        assert!(
            doc.asset(&m.texture_asset_id).is_some(),
            "Mesh {m_id} has invalid asset {}",
            m.texture_asset_id
        );
        assert!(
            doc.part(&m.part_id).is_some(),
            "Mesh {m_id} has invalid part {}",
            m.part_id
        );
        if !m.deformer_id.is_empty() {
            assert!(
                doc.transform(&m.deformer_id).is_some(),
                "Mesh {m_id} has invalid deformer {}",
                m.deformer_id
            );
        }
        for mask_id in &m.masks {
            assert!(
                doc.mesh(mask_id).is_some(),
                "Mesh {m_id} has invalid mask {mask_id}"
            );
        }
        assert_eq!(
            m.vertex_ids.len(),
            m.base_positions.len(),
            "Mesh {m_id} vertex/position count mismatch"
        );
        assert_eq!(
            m.vertex_ids.len(),
            m.uvs.len(),
            "Mesh {m_id} vertex/uv count mismatch"
        );
    }

    // Invariant 4: Parameter & Bindings integrity
    for b_id in doc.binding_order() {
        let b = doc.binding(b_id).expect("binding in order must exist");
        let mesh = doc
            .mesh(&b.mesh_id)
            .expect("binding target mesh must exist");
        let mut total_keys = 1usize;
        for axis in &b.axes {
            let param = doc
                .parameter(&axis.parameter_id)
                .expect("binding axis param must exist");
            total_keys *= axis.keys.len();
            for &k in &axis.keys {
                assert!(
                    k >= param.minimum && k <= param.maximum,
                    "Binding axis key {k} out of bounds [{min}, {max}] at {label}",
                    min = param.minimum,
                    max = param.maximum
                );
            }
        }
        assert_eq!(
            b.keyforms.len(),
            total_keys,
            "Binding {b_id} keyform count mismatch"
        );
        for kf in &b.keyforms {
            assert_eq!(
                kf.positions.len(),
                mesh.base_positions.len(),
                "Keyform position count mismatch with mesh"
            );
        }
    }

    // Invariant 5: Scene Bindings integrity
    for sb_id in doc.scene_binding_order() {
        let sb = doc
            .scene_binding(sb_id)
            .expect("scene binding in order must exist");
        assert!(
            doc.transform(sb.target_id()).is_some() || doc.part(sb.target_id()).is_some(),
            "Scene binding target {} missing",
            sb.target_id()
        );
        let mut total_keys = 1usize;
        for axis in &sb.axes {
            let param = doc
                .parameter(&axis.parameter_id)
                .expect("scene binding axis param must exist");
            total_keys *= axis.keys.len();
            for &k in &axis.keys {
                assert!(
                    k >= param.minimum && k <= param.maximum,
                    "Scene binding axis key out of bounds at {label}"
                );
            }
        }
        assert_eq!(
            sb.track.len(),
            total_keys,
            "Scene binding keyform count mismatch"
        );
    }

    // Invariant 6: Persistence roundtrip
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sequence.zipzi");
    let io = ProjectIo::new(root.path(), ResourceStore::default());
    io.save(doc, resources, &path, None, || true).unwrap();
    let encoded = std::fs::read(&path).unwrap();
    let decoded = io.load(&path, || true).unwrap();
    let second = root.path().join("reopened.zipzi");
    io.save(&decoded.document, &decoded.resources, &second, None, || {
        true
    })
    .unwrap();
    assert_eq!(
        project_json(&encoded),
        project_json(&std::fs::read(second).unwrap()),
        "roundtrip at {label}"
    );

    // Invariant 7: Preview isolation
    let rev_before = doc.content_id();
    let mut preview_vals = HashMap::new();
    for pid in doc.parameter_order() {
        preview_vals.insert(pid.clone(), 0.5);
    }
    let mut frame = DrawableFrame::default();
    let eval_status = evaluate_frame(doc, &preview_vals, &mut frame);
    assert!(
        eval_status.is_ok(),
        "evaluate_frame must succeed at {label}: {:?}",
        eval_status
    );
    assert_eq!(
        doc.content_id(),
        rev_before,
        "evaluate_frame must not alter revision at {label}"
    );

    // Invariant 8: MOC3 exportability
    if !doc.mesh_order().is_empty() {
        let moc_res = encode_moc3(doc);
        assert!(
            moc_res.is_ok(),
            "encode_moc3 failed at {label}: {:?}",
            moc_res.err()
        );
        let artifact = moc_res.unwrap();
        assert!(
            artifact.bytes.starts_with(b"MOC3"),
            "Exported MOC3 header invalid at {label}"
        );
    }
}

#[test]
fn test_random_operation_sequences_and_invariants() {
    for seed in [0x1234_5678_u64, 0x9ABC_DEF0_u64, 0xCAFE_BABE_u64] {
        let mut rng = Rng::new(seed);
        let (mut doc, resources) = build_initial_document();
        let mut id_counter = 100u32;

        let mut pending: Option<(DocumentState, DocumentBuilder)> = None;

        for step in 0..150 {
            let label = format!("seed={seed:#x}, step={step}");
            let doc_before = doc.clone();
            let rev_before = doc.content_id();

            let mut builder = doc.builder();
            let mut special = false;
            let op_type = rng.next_range(0, 17);
            let mut edit_result = None;

            match op_type {
                0 => {
                    // Create Part
                    id_counter += 1;
                    let new_id = make_uuid(0x0004, id_counter);
                    let parent_id = if rng.next_bool() && !doc.part_order().is_empty() {
                        rng.choose(doc.part_order()).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let res = builder.create_part(Part {
                        id: new_id,
                        runtime_id: format!("Part_{id_counter}"),
                        name: format!("Part {id_counter}"),
                        parent_id,
                        enabled: true,
                        draw_order: rng.next_f32(-10.0, 10.0),
                    });
                    edit_result = Some(res);
                }
                1 => {
                    // Reparent Part
                    if !doc.part_order().is_empty() {
                        let part_id = rng.choose(doc.part_order()).cloned().unwrap();
                        let mut p = doc.part(&part_id).unwrap().clone();
                        p.parent_id = if rng.next_bool() && doc.part_order().len() > 1 {
                            rng.choose(doc.part_order()).cloned().unwrap()
                        } else {
                            String::new()
                        };
                        let res = builder.replace_part(p);
                        edit_result = Some(res);
                    }
                }
                2 => {
                    // Create Transform (Rotation)
                    id_counter += 1;
                    let new_id = make_uuid(0x0005, id_counter);
                    let part_id = rng.choose(doc.part_order()).cloned().unwrap();
                    let parent_id = if rng.next_bool() && !doc.transform_order().is_empty() {
                        rng.choose(doc.transform_order()).cloned().unwrap()
                    } else {
                        String::new()
                    };
                    let res = builder.create_transform(Transform {
                        id: new_id,
                        runtime_id: format!("Rot_{id_counter}"),
                        name: format!("Rot {id_counter}"),
                        part_id: zipzi_core::PartId::optional(part_id),
                        parent_id: zipzi_core::TransformId::optional(parent_id),
                        data: TransformData::Rotation(RotationTransform {
                            base_angle: 0.0,
                            pose: RotationPose {
                                origin: Vec2::new(
                                    rng.next_f32(0.0, 600.0),
                                    rng.next_f32(0.0, 400.0),
                                )
                                .into(),
                                angle: rng.next_f32(-45.0, 45.0),
                                scale: rng.next_f32(0.8, 1.2),
                                reflect_x: false,
                                reflect_y: false,
                            },
                        }),
                        ..Default::default()
                    });
                    edit_result = Some(res);
                }
                3 => {
                    // Reparent Transform (including cyclic check)
                    if !doc.transform_order().is_empty() {
                        let t_id = rng.choose(doc.transform_order()).cloned().unwrap();
                        let mut t = doc.transform(&t_id).unwrap().clone();
                        t.parent_id = if rng.next_bool() && doc.transform_order().len() > 1 {
                            Some(rng.choose(doc.transform_order()).cloned().unwrap().into())
                        } else {
                            None
                        };
                        let res = builder.replace_transform(t);
                        edit_result = Some(res);
                    }
                }
                4 => {
                    // Create Parameter
                    id_counter += 1;
                    let new_id = make_uuid(0x0003, id_counter);
                    let min = rng.next_f32(-2.0, 0.0);
                    let max = rng.next_f32(0.0, 2.0);
                    let res = builder.create_parameter(Parameter {
                        id: new_id,
                        runtime_id: format!("Param_{id_counter}"),
                        name: format!("Parameter {id_counter}"),
                        minimum: min,
                        maximum: max,
                        default_value: (min + max) * 0.5,
                        decimal_places: 4,
                        ..Default::default()
                    });
                    edit_result = Some(res);
                }
                5 => {
                    // Modify Parameter Range
                    if !doc.parameter_order().is_empty() {
                        let pid = rng.choose(doc.parameter_order()).cloned().unwrap();
                        let mut p = doc.parameter(&pid).unwrap().clone();
                        if rng.next_bool() {
                            // Expand
                            p.minimum -= 0.5;
                            p.maximum += 0.5;
                        } else {
                            // Shrink
                            p.minimum += 0.2;
                            p.maximum -= 0.2;
                        }
                        let res = builder.replace_parameter(p);
                        edit_result = Some(res);
                    }
                }
                6 => {
                    // Create Mesh
                    id_counter += 1;
                    let new_id = make_uuid(0x0006, id_counter);
                    let asset_id = rng.choose(doc.asset_order()).cloned().unwrap();
                    let part_id = rng.choose(doc.part_order()).cloned().unwrap();
                    let deformer_id = if rng.next_bool() && !doc.transform_order().is_empty() {
                        rng.choose(doc.transform_order()).cloned().unwrap()
                    } else {
                        String::new()
                    };
                    let res = builder.create_mesh(Mesh {
                        id: new_id,
                        runtime_id: format!("Mesh_{id_counter}"),
                        name: format!("Mesh {id_counter}"),
                        texture_asset_id: (asset_id).into(),
                        part_id,
                        deformer_id,
                        vertex_ids: vec![10, 20, 30],
                        base_positions: vec![
                            Vec2::new(0.0, 0.0),
                            Vec2::new(50.0, 0.0),
                            Vec2::new(25.0, 50.0),
                        ],
                        uvs: vec![
                            Vec2::new(0.0, 0.0),
                            Vec2::new(1.0, 0.0),
                            Vec2::new(0.5, 1.0),
                        ],
                        triangles: vec![[10, 20, 30]],
                        draw_order: Some(rng.next_f32(0.0, 10.0)),
                        ..Default::default()
                    });
                    edit_result = Some(res);
                }
                7 => {
                    // Reparent Mesh
                    if !doc.mesh_order().is_empty() {
                        let m_id = rng.choose(doc.mesh_order()).cloned().unwrap();
                        let mut m = doc.mesh(&m_id).unwrap().clone();
                        m.part_id = rng.choose(doc.part_order()).cloned().unwrap();
                        m.deformer_id = if rng.next_bool() && !doc.transform_order().is_empty() {
                            rng.choose(doc.transform_order()).cloned().unwrap()
                        } else {
                            String::new()
                        };
                        let res = builder.replace_mesh(m);
                        edit_result = Some(res);
                    }
                }
                8 => {
                    // Update Mesh Vertex Positions
                    if !doc.mesh_order().is_empty() {
                        let m_id = rng.choose(doc.mesh_order()).cloned().unwrap();
                        let m = doc.mesh(&m_id).unwrap();
                        let mut mesh = m.clone();
                        for point in &mut mesh.base_positions {
                            point.x += rng.next_f32(-2., 2.);
                            point.y += rng.next_f32(-2., 2.);
                        }
                        let res = builder.replace_mesh(mesh);
                        edit_result = Some(res);
                    }
                }
                9 => {
                    // Modify Mesh Keyform
                    if !doc.binding_order().is_empty() {
                        let b_id = rng.choose(doc.binding_order()).cloned().unwrap();
                        let b = doc.binding(&b_id).unwrap();
                        let mut form = b.keyforms[0].clone();
                        for pt in &mut form.positions {
                            pt.x += rng.next_f32(-1.0, 1.0);
                        }
                        let res = builder.set_mesh_keyform(&b_id, form);
                        edit_result = Some(res);
                    }
                }
                10 => {
                    // Erase random object (could be referenced, testing reference safety)
                    let kind = rng.next_range(0, 4);
                    let to_erase = match kind {
                        0 if !doc.part_order().is_empty() => rng.choose(doc.part_order()).cloned(),
                        1 if !doc.transform_order().is_empty() => {
                            rng.choose(doc.transform_order()).cloned()
                        }
                        2 if !doc.mesh_order().is_empty() => rng.choose(doc.mesh_order()).cloned(),
                        3 if !doc.parameter_order().is_empty() => {
                            rng.choose(doc.parameter_order()).cloned()
                        }
                        _ => None,
                    };
                    if let Some(id) = to_erase {
                        let res = match kind {
                            0 => builder.remove_part(&id),
                            1 => builder.remove_transform(&id),
                            2 => builder.remove_mesh(&id),
                            3 => builder.remove_parameter(&id),
                            _ => unreachable!(),
                        };
                        edit_result = Some(res);
                    }
                }
                11 => {
                    special = true;
                    if pending.is_none() {
                        pending = Some((doc.clone(), doc.builder()));
                    }
                }
                12 => {
                    special = true;
                    if let Some((_, candidate)) = &mut pending {
                        if let Some(id) = candidate.mesh_order().first().cloned() {
                            let mut mesh = candidate.mesh(&id).unwrap().clone();
                            mesh.base_positions[0].x += rng.next_f32(-1., 1.);
                            candidate.replace_mesh(mesh).unwrap();
                        }
                    }
                    assert_eq!(
                        doc.content_id(),
                        doc_before.content_id(),
                        "candidate isolation at {label}"
                    );
                }
                13 => {
                    special = true;
                    if let Some((base, candidate)) = pending.take() {
                        // A competing publication makes this source stale.
                        if base.content_id() == doc.content_id() {
                            match candidate.freeze() {
                                Ok(frozen) => doc = frozen.state,
                                Err(_) => assert_eq!(doc.content_id(), doc_before.content_id()),
                            }
                        } else {
                            assert_eq!(doc.content_id(), doc_before.content_id());
                        }
                    }
                }
                14 => {
                    special = true;
                    drop(pending.take());
                    assert_eq!(
                        doc.content_id(),
                        doc_before.content_id(),
                        "discard cannot overwrite a newer snapshot at {label}"
                    );
                }
                15 => {
                    // Negative check: Attempt cyclic transform parent
                    if doc.transform_order().len() >= 2 {
                        let t0 = doc.transform_order()[0].clone();
                        let _t1 = doc.transform_order()[1].clone();
                        let mut t = doc.transform(&t0).unwrap().clone();
                        t.parent_id = Some(t0.clone().into()); // self-cycle
                        let res = builder.replace_transform(t);

                        edit_result = Some(res);
                    }
                }
                16 => {
                    // Negative check: Add invalid opacity
                    if !doc.scene_binding_order().is_empty() {
                        let sb_id = rng.choose(doc.scene_binding_order()).cloned().unwrap();
                        let sb = doc.scene_binding(&sb_id).unwrap();
                        let f = match &sb.track {
                            zipzi_core::SceneTrack::Warp { keyforms, .. } => {
                                let mut f = keyforms[0].clone();
                                f.appearance.opacity = f32::NAN;
                                zipzi_core::SceneKeyform::Warp(f)
                            }
                            zipzi_core::SceneTrack::Rotation { keyforms, .. } => {
                                let mut f = keyforms[0].clone();
                                f.appearance.opacity = f32::NAN;
                                zipzi_core::SceneKeyform::Rotation(f)
                            }
                            zipzi_core::SceneTrack::Part { keyforms, .. } => {
                                let mut f = keyforms[0].clone();
                                f.draw_order = f32::NAN;
                                zipzi_core::SceneKeyform::Part(f)
                            }
                        };
                        let res = builder.set_scene_keyform(&sb_id, f);

                        edit_result = Some(res);
                    }
                }
                17 => {
                    // A no-op freeze reuses identity and has no write set.
                    let frozen = doc.builder().freeze().unwrap();
                    assert_eq!(frozen.state.content_id(), doc.content_id());
                }
                _ => {}
            }

            if !special {
                if let Some(result) = edit_result {
                    let frozen = result.and_then(|()| builder.freeze());
                    let must_succeed = matches!(op_type, 0 | 2 | 4 | 6 | 7 | 8 | 9);
                    if must_succeed {
                        assert!(
                            frozen.is_ok(),
                            "valid operation {op_type} at {label}: {frozen:?}"
                        );
                    }
                    if matches!(op_type, 15 | 16) {
                        assert!(frozen.is_err(), "invalid operation {op_type} at {label}");
                    }
                    match frozen {
                        Ok(frozen) => {
                            if matches!(op_type, 0 | 2 | 4 | 6 | 8 | 9) {
                                assert_ne!(
                                    frozen.state.content_id(),
                                    rev_before,
                                    "edit had no effect at {label}"
                                );
                            }
                            doc = frozen.state;
                        }
                        Err(_) => assert_eq!(
                            doc.content_id(),
                            doc_before.content_id(),
                            "failure isolation at {label}"
                        ),
                    }
                }
            }
            verify_invariants(&doc, &resources, &label);
        }
    }
}
