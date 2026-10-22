use kasane_core::{document::*, *};
use kasane_sdk::{
    AuthoringSession, ObjectKind, SceneContentEdit as E, SceneContentTarget as T,
    SceneContentValue as V,
};
fn id(n: u32) -> String {
    format!("20000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}

#[test]
fn warp_content_addresses_expire_after_organization_context_roundtrip() {
    let mut s = session();
    let create = s
        .plan_scene_edit(SceneEdit::Create {
            id: id(2),
            name: "Warp".into(),
            object: SceneCreate::Warp { bounds: None },
            part_id: None,
            parent_id: None,
            before: None,
        })
        .unwrap();
    s.execute_scene_plan(&create).unwrap();
    s.edit("Part", None, |e| {
        e.create_part(Part {
            id: id(3),
            ..Default::default()
        })
    })
    .unwrap();
    let address = s
        .scene_content_address(
            &id(2),
            T::WarpPoint {
                row: 0,
                column: 0,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    let plan = s
        .plan_organization_edit(OrganizationEdit::Move {
            ids: vec![id(2)],
            destination: OrganizationCollection::PartMembers {
                part_id: Some(id(3)),
            },
            before: None,
        })
        .unwrap();
    s.execute_organization_plan(&plan).unwrap();
    assert!(s
        .plan_scene_content_patch(&address, address.expected().clone())
        .is_err());
    s.undo().unwrap();
    assert!(s
        .plan_scene_content_patch(&address, address.expected().clone())
        .is_err());
}

#[test]
fn unchanged_synthetic_bezier_control_does_not_materialize_controller() {
    let mut s = session();
    let create = s
        .plan_scene_edit(SceneEdit::Create {
            id: id(2),
            name: "Warp".into(),
            object: SceneCreate::Warp { bounds: None },
            part_id: None,
            parent_id: None,
            before: None,
        })
        .unwrap();
    s.execute_scene_plan(&create).unwrap();
    mesh(&mut s, 3);
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    let address = s
        .scene_content_address(
            &id(2),
            T::WarpBezierControl {
                node: 0,
                component: 0,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    let plan = s
        .plan_scene_content_patch(&address, address.expected().clone())
        .unwrap();
    assert!(plan.is_noop());
    assert!(!s.execute_scene_content_plan(&plan).unwrap().changed);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s
        .document()
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_none());
    let plan = s
        .plan_scene_content_patch(&address, V::Point(Vec2::new(2., 3.)))
        .unwrap();
    assert!(!plan.is_noop());
    s.execute_scene_content_plan(&plan).unwrap();
    assert!(s
        .document()
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_some());
}

#[test]
fn unchanged_implicit_offscreen_mapping_preserves_redo() {
    let mut s = session();
    offscreen(&mut s, false);
    mesh(&mut s, 30);
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    let address = s
        .scene_content_address(
            &id(23),
            T::OffscreenMapping {
                slot: PartKeyformSlot::Static,
            },
        )
        .unwrap();
    let plan = s
        .plan_scene_content_patch(&address, address.expected().clone())
        .unwrap();
    assert!(plan.is_noop());
    assert!(!s.execute_scene_content_plan(&plan).unwrap().changed);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s
        .document()
        .get_offscreen(&id(23))
        .unwrap()
        .part_keyform_indices
        .is_empty());
    s.redo().unwrap();
    assert!(s.document().get_mesh(&id(30)).is_some());
}

#[test]
fn glue_endpoint_changes_publish_pair_references_in_both_history_directions() {
    let mut s = session();
    for n in [2, 3, 4] {
        mesh(&mut s, n);
    }
    glue(&mut s, 5, 2, 3, vec![pair(6, Some(1), Some(2))]);
    let expected = s
        .document()
        .scene_field(&id(5), SceneField::GlueEndpoints)
        .unwrap();
    let plan = s
        .plan_scene_edit(SceneEdit::Patch {
            id: id(5),
            expected,
            value: SceneFieldValue::GlueEndpoints {
                a: id(4).into(),
                b: id(3).into(),
            },
            clear_glue_pairs: false,
        })
        .unwrap();
    let check = |receipt: &kasane_sdk::EditReceipt, before: u32, after: u32| {
        let change = receipt
            .changes
            .content_references
            .iter()
            .find(|c| c.entity.id == id(6))
            .expect("pair target change");
        let kasane_sdk::SceneContentReferenceValue::Pair { mesh_a, .. } = &change.before else {
            panic!("before pair")
        };
        assert_eq!(mesh_a.resolved_id(), Some(id(before).as_str()));
        let kasane_sdk::SceneContentReferenceValue::Pair { mesh_a, .. } = &change.after else {
            panic!("after pair")
        };
        assert_eq!(mesh_a.resolved_id(), Some(id(after).as_str()));
        assert!(receipt.changes.modified.iter().any(|e| e.id == id(6)));
    };
    check(&s.execute_scene_plan(&plan).unwrap(), 2, 4);
    check(&s.undo().unwrap(), 4, 2);
    check(&s.redo().unwrap(), 2, 4);
}

#[test]
fn shape_address_rejects_recreated_binding_with_identical_content() {
    let mut s = session();
    mesh(&mut s, 2);
    let positions = s
        .document()
        .get_mesh(&id(2))
        .unwrap()
        .base_positions
        .clone();
    s.edit("Binding", None, |e| {
        e.create_parameter(Parameter {
            id: id(3),
            ..Default::default()
        })?;
        e.create_binding(MeshBinding {
            id: id(4),
            mesh_id: id(2),
            axes: vec![BindingAxis::new(id(3), vec![0.])],
            keyforms: vec![MeshKeyform {
                keys: vec![0.],
                positions,
                ..Default::default()
            }],
        })
    })
    .unwrap();
    let binding = s.document().get_binding(&id(4)).unwrap().clone();
    let address = s
        .scene_content_address(
            &id(2),
            T::VertexPosition {
                vertex_id: 1,
                shape: ShapeTarget::Ordinary {
                    binding_id: id(4),
                    samples: binding.keyforms[0].samples.clone(),
                },
            },
        )
        .unwrap();
    let handle = s.handle(ObjectKind::MeshBinding, &id(4)).unwrap();
    s.edit("Recreate binding", None, |e| {
        e.erase_object(&id(4))?;
        e.create_binding(binding)
    })
    .unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert!(s
        .plan_scene_content_patch(&address, V::Point(Vec2::new(8., 9.)))
        .is_err());
}
fn run(s: &mut AuthoringSession, e: E) -> kasane_sdk::EditReceipt {
    let p = s.plan_scene_content(e).unwrap();
    s.execute_scene_content_plan(&p).unwrap()
}
fn mesh(s: &mut AuthoringSession, n: u32) {
    s.edit("Mesh", None, |e| {
        e.create_mesh(Mesh {
            id: id(n),
            vertex_ids: vec![1, 2, 3],
            base_positions: vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)],
            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        })
    })
    .unwrap();
}
fn lock(s: &mut AuthoringSession, id: &str) {
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id.into(),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("Lock", None, |e| e.replace_editor_state(state))
        .unwrap();
}
fn glue(s: &mut AuthoringSession, n: u32, a: u32, b: u32, pairs: Vec<GlueVertexPair>) {
    s.edit("Glue", None, |e| {
        e.create_glue(Glue {
            id: id(n),
            mesh_a_id: id(a).into(),
            mesh_b_id: id(b).into(),
            pairs,
            ..Default::default()
        })
    })
    .unwrap();
}
fn pair(n: u32, a: Option<u32>, b: Option<u32>) -> GlueVertexPair {
    GlueVertexPair {
        id: id(n),
        vertex_a: a,
        vertex_b: b,
        ..Default::default()
    }
}
fn offscreen(s: &mut AuthoringSession, bound: bool) {
    s.edit("Offscreen", None, |e| {
        e.create_part(Part {
            id: id(20),
            ..Default::default()
        })?;
        if bound {
            e.create_parameter(Parameter {
                id: id(21),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })?;
            e.create_scene_binding(SceneBinding {
                id: id(22),
                axes: vec![BindingAxis::new(id(21), vec![0., 1.])],
                track: SceneTrack::Part {
                    target_id: id(20).into(),
                    keyforms: vec![
                        PartKeyform {
                            keys: vec![0.],
                            ..Default::default()
                        },
                        PartKeyform {
                            keys: vec![1.],
                            ..Default::default()
                        },
                    ],
                },
            })?;
        }
        e.create_offscreen(Offscreen {
            id: id(23),
            part_id: id(20),
            keyforms: vec![
                OffscreenKeyform {
                    opacity: 0.2,
                    ..Default::default()
                },
                OffscreenKeyform {
                    opacity: 0.8,
                    ..Default::default()
                },
            ],
            part_keyform_indices: if bound { vec![0, 1] } else { vec![] },
            ..Default::default()
        })
    })
    .unwrap();
}

#[test]
fn unchanged_warp_points_preserve_bezier_controller_and_redo() {
    let mut s = session();
    let points = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(0., 1.),
        Vec2::new(1., 1.),
    ];
    let controller = WarpBezier::from_lattice(1, 1, false, &points);
    s.edit("Warp", None, |e| {
        e.create_transform(Transform {
            id: id(2),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: false,
                points: points.clone(),
                bezier: Some(controller.clone()),
            }),
            ..Default::default()
        })
    })
    .unwrap();
    mesh(&mut s, 3);
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    let (_, receipt) = s
        .edit("Unchanged points", None, |e| {
            e.update_warp_points(&id(2), points)
        })
        .unwrap();
    assert!(!receipt.changed);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert_eq!(
        s.document()
            .get_transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .bezier
            .as_ref(),
        Some(&controller)
    );
}
#[test]
fn mesh_topology_seeds_every_shape_and_keeps_vertex_identity() {
    let mut s = session();
    mesh(&mut s, 2);
    s.edit("Binding", None, |e| {
        e.create_parameter(Parameter {
            id: id(3),
            ..Default::default()
        })?;
        e.create_binding(MeshBinding {
            id: id(4),
            mesh_id: id(2),
            axes: vec![BindingAxis::new(id(3), vec![-1., 1.])],
            keyforms: vec![
                MeshKeyform {
                    keys: vec![-1.],
                    positions: vec![Vec2::new(0., 2.), Vec2::new(20., 2.), Vec2::new(0., 20.)],
                    ..Default::default()
                },
                MeshKeyform {
                    keys: vec![1.],
                    positions: vec![Vec2::new(4., 2.), Vec2::new(24., 2.), Vec2::new(4., 20.)],
                    ..Default::default()
                },
            ],
        })
    })
    .unwrap();
    let old = s.document().get_binding(&id(4)).unwrap().clone();
    run(
        &mut s,
        E::InsertMeshVertices {
            mesh_id: id(2),
            vertices: vec![NewMeshVertex {
                vertex_id: 40,
                position: Vec2::new(5., 0.),
                uv: Vec2::new(0.5, 0.),
                seed: VertexSeed::Interpolate {
                    weights: vec![
                        VertexWeight {
                            vertex_id: 1,
                            weight: 0.5,
                        },
                        VertexWeight {
                            vertex_id: 2,
                            weight: 0.5,
                        },
                    ],
                },
            }],
            before: Some(3),
        },
    );
    assert_eq!(
        s.document().get_mesh(&id(2)).unwrap().vertex_ids,
        [1, 2, 40, 3]
    );
    assert_eq!(
        s.document().get_mesh(&id(2)).unwrap().base_positions[2],
        Vec2::new(5., 0.)
    );
    assert_eq!(
        s.document().get_binding(&id(4)).unwrap().keyforms[0].positions[2],
        Vec2::new(10., 2.)
    );
    assert_eq!(s.document().get_binding(&id(4)).unwrap().axes, old.axes);
    run(
        &mut s,
        E::ReorderMeshVertices {
            mesh_id: id(2),
            order: vec![40, 3, 2, 1],
        },
    );
    assert_eq!(
        s.document().get_mesh(&id(2)).unwrap().triangles,
        [[1, 2, 3]]
    );
    assert_eq!(
        s.document().get_binding(&id(4)).unwrap().keyforms[1].positions[0],
        Vec2::new(14., 2.)
    );
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(s.document().get_binding(&id(4)), Some(&old));
}
#[test]
fn vertex_deletion_requires_policy_and_cleans_dependent_pairs() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(&mut s, 4, 2, 3, vec![pair(5, Some(1), Some(2))]);
    let old = s.handle(ObjectKind::GlueVertexPair, &id(5)).unwrap();
    let version = s.version();
    assert_eq!(
        s.plan_scene_content(E::DeleteMeshVertices {
            mesh_id: id(2),
            vertices: vec![1],
            policy: VertexDeletePolicy::RejectReferences
        })
        .unwrap_err()
        .code
        .as_ref(),
        "VERTEX_REFERENCED"
    );
    assert_eq!(s.version(), version);
    let p = s
        .plan_scene_content(E::DeleteMeshVertices {
            mesh_id: id(2),
            vertices: vec![1],
            policy: VertexDeletePolicy::RemoveDependentContent,
        })
        .unwrap();
    assert_eq!(p.report().removed_pair_ids, [id(5)]);
    assert_eq!(p.report().removed_triangle_slots, [0]);
    s.execute_scene_content_plan(&p).unwrap();
    assert!(s.document().get_glue(&id(4)).unwrap().pairs.is_empty());
    assert!(s.document().get_mesh(&id(2)).unwrap().triangles.is_empty());
    s.undo().unwrap();
    assert!(s.resolve_handle(&old).is_err());
    assert!(s.document().get_glue_pair(&id(5)).is_some());
}
#[test]
fn pair_pending_configuration_and_reordering_keep_ids_and_runtime_slots() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(
        &mut s,
        4,
        2,
        3,
        vec![pair(5, None, None), pair(6, Some(1), Some(1))],
    );
    let h = s.handle(ObjectKind::GlueVertexPair, &id(5)).unwrap();
    assert_eq!(s.scene_projection().pair_slots[&id(6)].source_slot, 1);
    assert!(!s.scene_projection().pair_slots.contains_key(&id(5)));
    assert!(s
        .scene_projection()
        .disabled
        .iter()
        .any(|d| d.id == id(5) && d.usage == SceneUse::GluePair));
    assert!(s
        .document()
        .scene_export_omissions()
        .iter()
        .any(|d| d.id == id(5)));
    let address = s
        .scene_content_address(&id(4), T::GluePair { pair_id: id(5) })
        .unwrap();
    run(
        &mut s,
        E::MoveGluePairs {
            glue_id: id(4),
            ids: vec![id(5)],
            before: None,
        },
    );
    s.resolve_handle(&h).unwrap();
    assert!(s
        .plan_scene_content_patch(&address, V::GluePair(pair(5, Some(2), Some(2))))
        .is_err());
    let a = s
        .scene_content_address(&id(4), T::GluePair { pair_id: id(5) })
        .unwrap();
    let p = s
        .plan_scene_content_patch(&a, V::GluePair(pair(5, Some(2), Some(2))))
        .unwrap();
    s.execute_scene_content_plan(&p).unwrap();
    assert!(!s.scene_projection().disabled.iter().any(|d| d.id == id(5)));
    assert!(s.evaluate(&Default::default()).is_ok());
    let encoded = kasane_project::encode_project(s.document()).unwrap();
    let restored = kasane_project::decode_project(&encoded).unwrap();
    assert_eq!(
        restored.get_glue_pair(&id(5)),
        s.document().get_glue_pair(&id(5))
    );
    run(
        &mut s,
        E::DuplicateGluePairs {
            glue_id: id(4),
            ids: vec![id(5)],
            before: None,
        },
    );
    assert_eq!(s.glue_pairs(&id(4)).unwrap().len(), 3);
    assert!(s.document().validate_structure().is_empty());
}
#[test]
fn pair_transfer_keeps_handle_but_expires_owner_address() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(&mut s, 4, 2, 3, vec![pair(5, Some(1), Some(2))]);
    glue(&mut s, 6, 3, 2, vec![]);
    let h = s.handle(ObjectKind::GlueVertexPair, &id(5)).unwrap();
    let a = s
        .scene_content_address(&id(4), T::GluePair { pair_id: id(5) })
        .unwrap();
    run(
        &mut s,
        E::TransferGluePairs {
            glue_id: id(4),
            destination: id(6),
            pairs: vec![pair(5, Some(2), Some(1))],
            before: None,
        },
    );
    s.resolve_handle(&h).unwrap();
    assert_eq!(s.document().glue_pair_owner(&id(5)), Some(id(6).as_str()));
    s.undo().unwrap();
    assert!(s
        .plan_scene_content_patch(&a, a.expected().clone())
        .is_err());
}
#[test]
fn uv_and_triangle_addresses_expire_after_topology_and_undo() {
    let mut s = session();
    mesh(&mut s, 2);
    let a = s
        .scene_content_address(&id(2), T::VertexUv { vertex_id: 2 })
        .unwrap();
    let p = s
        .plan_scene_content_patch(&a, V::Point(Vec2::new(0.6, 0.3)))
        .unwrap();
    s.execute_scene_content_plan(&p).unwrap();
    assert_eq!(
        s.document().get_mesh(&id(2)).unwrap().uvs[1],
        Vec2::new(0.6, 0.3)
    );
    let a = s
        .scene_content_address(&id(2), T::Triangle { slot: 0 })
        .unwrap();
    run(
        &mut s,
        E::DeleteTriangles {
            mesh_id: id(2),
            slots: vec![0],
        },
    );
    s.undo().unwrap();
    assert!(s
        .plan_scene_content_patch(&a, V::Triangle([1, 3, 2]))
        .is_err());
    assert!(s
        .plan_scene_content(E::ReplaceTriangles {
            mesh_id: id(2),
            triangles: vec![[1, 2, 99]]
        })
        .is_err());
    let a = s
        .scene_content_address(
            &id(2),
            T::VertexPosition {
                vertex_id: 1,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    s.edit("Label", None, |e| e.rename_mesh(&id(2), "Face"))
        .unwrap();
    assert!(s
        .plan_scene_content_patch(&a, V::Point(Vec2::new(2., 3.)))
        .is_ok());
}
#[test]
fn offscreen_reorders_insertions_and_deletion_remap_part_sampling() {
    let mut s = session();
    offscreen(&mut s, true);
    let a = s
        .scene_content_address(&id(23), T::OffscreenKeyform { slot: 0 })
        .unwrap();
    run(
        &mut s,
        E::ReorderOffscreenKeyforms {
            offscreen_id: id(23),
            order: vec![1, 0],
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [1, 0]
    );
    assert!(s
        .plan_scene_content_patch(&a, a.expected().clone())
        .is_err());
    run(
        &mut s,
        E::InsertOffscreenKeyform {
            offscreen_id: id(23),
            value: OffscreenKeyform {
                opacity: 0.4,
                ..Default::default()
            },
            before: Some(0),
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [2, 1]
    );
    assert!(s
        .plan_scene_content(E::DeleteOffscreenKeyforms {
            offscreen_id: id(23),
            slots: vec![2],
            policy: OffscreenDeletePolicy::RejectReferences
        })
        .is_err());
    run(
        &mut s,
        E::DeleteOffscreenKeyforms {
            offscreen_id: id(23),
            slots: vec![2],
            policy: OffscreenDeletePolicy::Remap {
                replacements: [(2, Some(0))].into(),
            },
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [0, 1]
    );
    let SceneTrack::Part { keyforms, .. } = &s.document().get_scene_binding(&id(22)).unwrap().track
    else {
        unreachable!()
    };
    let samples = keyforms[1].samples.clone();
    run(
        &mut s,
        E::SetOffscreenMapping {
            offscreen_id: id(23),
            updates: vec![OffscreenMappingPatch {
                slot: PartKeyformSlot::Samples { samples },
                keyform: -7,
            }],
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [0, -7]
    );
    assert!(s.evaluate(&Default::default()).is_ok());
}
#[test]
fn static_offscreen_implicit_zero_is_preserved_by_structural_edits() {
    let mut s = session();
    offscreen(&mut s, false);
    run(
        &mut s,
        E::InsertOffscreenKeyform {
            offscreen_id: id(23),
            value: OffscreenKeyform {
                opacity: 1.,
                ..Default::default()
            },
            before: Some(0),
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [1]
    );
    run(
        &mut s,
        E::DeleteOffscreenKeyforms {
            offscreen_id: id(23),
            slots: vec![1],
            policy: OffscreenDeletePolicy::Neutral,
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [-1]
    );
}
#[test]
fn warp_control_resize_updates_grid_shapes_and_expires_old_controls() {
    let mut s = session();
    s.edit("Warp", None, |e| {
        e.create_transform(Transform {
            id: id(2),
            runtime_id: "Warp".into(),
            name: "Warp".into(),
            part_id: None,
            parent_id: None,
            appearance: Default::default(),
            enabled: true,
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: false,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(10., 0.),
                    Vec2::new(0., 10.),
                    Vec2::new(10., 10.),
                ],
            }),
        })
    })
    .unwrap();
    let a = s
        .scene_content_address(
            &id(2),
            T::WarpBezierControl {
                node: 0,
                component: 2,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    let p = s
        .plan_scene_content_patch(&a, V::Point(Vec2::new(6., 2.)))
        .unwrap();
    s.execute_scene_content_plan(&p).unwrap();
    assert!(s
        .document()
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_some());
    let a = s
        .scene_content_address(
            &id(2),
            T::WarpPoint {
                row: 1,
                column: 1,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    assert!(s
        .plan_scene_content_patch(&a, V::Point(Vec2::new(12., 12.)))
        .is_err());
    let p = s
        .plan_scene_content(E::ResizeWarp {
            transform_id: id(2),
            rows: 3,
            columns: 2,
            quad: true,
        })
        .unwrap();
    assert_eq!(p.report().resampled_shapes, 1);
    s.execute_scene_content_plan(&p).unwrap();
    assert_eq!(
        s.document()
            .get_transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .points
            .len(),
        12
    );
    s.undo().unwrap();
    assert!(s
        .plan_scene_content_patch(&a, a.expected().clone())
        .is_err());
}
#[test]
fn content_failures_locks_noops_budget_and_cancellation_are_atomic() {
    let mut s = session();
    mesh(&mut s, 2);
    let v = s.version();
    let p = s
        .plan_scene_content(E::ReorderMeshVertices {
            mesh_id: id(2),
            order: vec![1, 2, 3],
        })
        .unwrap();
    assert!(p.is_noop());
    assert!(!s.execute_scene_content_plan(&p).unwrap().changed);
    assert_eq!(s.version(), v);
    let control = ConversionControl::default();
    control.cancel();
    assert_eq!(
        s.plan_scene_content_with_control(
            E::DeleteTriangles {
                mesh_id: id(2),
                slots: vec![0]
            },
            &control
        )
        .unwrap_err()
        .code
        .as_ref(),
        "AUTHORING_CANCELLED"
    );
    let p = s
        .plan_scene_content(E::DeleteTriangles {
            mesh_id: id(2),
            slots: vec![0],
        })
        .unwrap();
    s.set_authoring_limits(AuthoringLimits {
        max_conversion_bytes: 1,
        ..Default::default()
    })
    .unwrap();
    assert!(s.execute_scene_content_plan(&p).is_err());
    assert_eq!(s.version(), v);
    s.set_authoring_limits(Default::default()).unwrap();
    lock(&mut s, &id(2));
    assert!(s
        .plan_scene_content(E::DeleteTriangles {
            mesh_id: id(2),
            slots: vec![0]
        })
        .is_err());
}

#[test]
fn ordinary_and_blend_shapes_follow_topology_without_recreating_shared_tables() {
    let mut s = session();
    mesh(&mut s, 2);
    let p = s
        .plan_scene_edit(SceneEdit::Create {
            id: id(9),
            name: "Warp".into(),
            object: SceneCreate::Warp {
                bounds: Some([0., 0., 10., 10.]),
            },
            part_id: None,
            parent_id: None,
            before: None,
        })
        .unwrap();
    s.execute_scene_plan(&p).unwrap();
    let points = s
        .document()
        .get_transform(&id(9))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    s.edit("Shapes", None, |e| {
        e.create_parameter(Parameter {
            id: id(3),
            ..Default::default()
        })?;
        e.create_scene_binding(SceneBinding {
            id: id(4),
            axes: vec![BindingAxis::new(id(3), vec![-1., 1.])],
            track: SceneTrack::Warp {
                target_id: id(9).into(),
                keyforms: vec![
                    WarpKeyform {
                        keys: vec![-1.],
                        positions: points.clone(),
                        ..Default::default()
                    },
                    WarpKeyform {
                        keys: vec![1.],
                        positions: points
                            .iter()
                            .map(|p| Vec2::new(p.x + 4., p.y + 3.))
                            .collect(),
                        ..Default::default()
                    },
                ],
            },
        })?;
        e.create_parameter(Parameter {
            id: id(5),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(6),
            parameter_id: id(5),
            samples: vec![],
            base_sample_id: String::new(),
            keys: vec![0., 1.],
            base_key_idx: 0,
        })?;
        e.create_blend_binding(BlendShapeBinding {
            id: id(7),
            target_id: id(2),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: id(6),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::default(); 3],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(2., 3.), Vec2::new(6., 7.), Vec2::new(10., 11.)],
                    ..Default::default()
                },
            ]),
        })?;
        e.create_blend_binding(BlendShapeBinding {
            id: id(8),
            target_id: id(9),
            target_kind: BlendShapeTargetKind::Warp,
            key_table_id: id(6),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Warp(vec![
                DeltaWarpKeyform {
                    points: vec![Vec2::default(); 4],
                    ..Default::default()
                },
                DeltaWarpKeyform {
                    points: vec![Vec2::new(2., 3.); 4],
                    ..Default::default()
                },
            ]),
        })
    })
    .unwrap();
    let table = s.document().get_blend_key_table(&id(6)).unwrap().clone();
    run(
        &mut s,
        E::InsertMeshVertices {
            mesh_id: id(2),
            vertices: vec![NewMeshVertex {
                vertex_id: 4,
                position: Vec2::new(5., 0.),
                uv: Vec2::new(0.5, 0.),
                seed: VertexSeed::Interpolate {
                    weights: vec![
                        VertexWeight {
                            vertex_id: 1,
                            weight: 0.5,
                        },
                        VertexWeight {
                            vertex_id: 2,
                            weight: 0.5,
                        },
                    ],
                },
            }],
            before: None,
        },
    );
    let DeltaKeyforms::Mesh(forms) = &s.document().get_blend_binding(&id(7)).unwrap().keyforms
    else {
        unreachable!()
    };
    assert_eq!(forms[1].positions[3], Vec2::new(4., 5.));
    let address = s
        .scene_content_address(
            &id(2),
            T::VertexPosition {
                vertex_id: 4,
                shape: ShapeTarget::Blend {
                    binding_id: id(7),
                    sample_id: table.samples[1].id.clone(),
                },
            },
        )
        .unwrap();
    let p = s
        .plan_scene_content_patch(&address, V::Point(Vec2::new(8., 9.)))
        .unwrap();
    s.execute_scene_content_plan(&p).unwrap();
    run(
        &mut s,
        E::ResizeWarp {
            transform_id: id(9),
            rows: 3,
            columns: 2,
            quad: false,
        },
    );
    let SceneTrack::Warp { keyforms, .. } = &s.document().get_scene_binding(&id(4)).unwrap().track
    else {
        unreachable!()
    };
    assert_eq!(keyforms[1].positions.len(), 12);
    assert_eq!(keyforms[1].positions[11], Vec2::new(14., 13.));
    let samples = keyforms[1].samples.clone();
    let DeltaKeyforms::Warp(forms) = &s.document().get_blend_binding(&id(8)).unwrap().keyforms
    else {
        unreachable!()
    };
    assert_eq!(forms[1].points.len(), 12);
    assert!(forms[1]
        .points
        .iter()
        .all(|p| (p.x - 2.).abs() < 1e-5 && (p.y - 3.).abs() < 1e-5));
    assert_eq!(s.document().get_blend_key_table(&id(6)), Some(&table));
    run(
        &mut s,
        E::WarpPoints {
            transform_id: id(9),
            target: ShapeTarget::Ordinary {
                binding_id: id(4),
                samples,
            },
            updates: vec![WarpPointPatch {
                row: 3,
                column: 2,
                point: Vec2::new(16., 17.),
            }],
            clear_bezier: false,
        },
    );
    assert!(s.document().validate_structure().is_empty());
}
#[test]
fn content_reference_receipts_invert_and_locked_pair_flags_are_not_dropped() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(&mut s, 4, 2, 3, vec![pair(5, Some(1), Some(2))]);
    let r = run(
        &mut s,
        E::PatchGluePair {
            glue_id: id(4),
            pair: pair(5, Some(2), Some(3)),
        },
    );
    assert_eq!(r.changes.content_references.len(), 1);
    let edge = r.changes.content_references[0].clone();
    let undo = s.undo().unwrap();
    assert_eq!(undo.changes.content_references[0].before, edge.after);
    assert_eq!(undo.changes.content_references[0].after, edge.before);
    lock(&mut s, &id(5));
    let v = s.version();
    assert!(s
        .plan_scene_content(E::DeleteMeshVertices {
            mesh_id: id(2),
            vertices: vec![1],
            policy: VertexDeletePolicy::RemoveDependentContent
        })
        .is_err());
    assert_eq!(s.version(), v);
    let mut g = s.document().get_glue(&id(4)).unwrap().clone();
    g.pairs.clear();
    assert_eq!(
        s.edit("Clear locked", None, |e| e.replace_glue(g))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert!(s
        .plan_scene_content(E::InsertGluePair {
            glue_id: id(4),
            pair: pair(2, None, None),
            before: None
        })
        .is_err());
}
#[test]
fn bezier_controller_topology_can_change_without_resetting_the_runtime_lattice() {
    let mut s = session();
    let p = s
        .plan_scene_edit(SceneEdit::Create {
            id: id(2),
            name: "Warp".into(),
            object: SceneCreate::Warp {
                bounds: Some([0., 0., 10., 10.]),
            },
            part_id: None,
            parent_id: None,
            before: None,
        })
        .unwrap();
    s.execute_scene_plan(&p).unwrap();
    let old = s
        .document()
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    let address = s
        .scene_content_address(
            &id(2),
            T::WarpBezierControl {
                node: 0,
                component: 2,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    let grid: Vec<_> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 * 2.5, y as f32 * 2.5)))
        .collect();
    let controller = WarpBezier::from_lattice(4, 4, false, &grid);
    assert_eq!(controller.rows, 2);
    run(
        &mut s,
        E::WarpBezier {
            transform_id: id(2),
            target: ShapeTarget::Base,
            controller: Some(controller),
        },
    );
    let w = s.document().get_transform(&id(2)).unwrap().warp().unwrap();
    assert_eq!(w.bezier.as_ref().unwrap().rows, 2);
    assert_eq!(w.points, old);
    let value = s
        .scene_content_address(
            &id(2),
            T::WarpBezierControl {
                node: 4,
                component: 2,
                shape: ShapeTarget::Base,
            },
        )
        .unwrap();
    assert!(matches!(value.expected(), V::Point(_)));
    s.undo().unwrap();
    assert!(s
        .plan_scene_content_patch(&address, address.expected().clone())
        .is_err());
}
