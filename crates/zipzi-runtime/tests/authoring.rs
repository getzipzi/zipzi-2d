use zipzi_core::{ModelSetup, Vec2};
use zipzi_runtime::authoring::{EditOperation, ParameterQuery};
use zipzi_runtime::authoring::{MeshBindingQuery, ModelingSampleQuery};
use zipzi_runtime::*;
use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};
use zipzi_sdk::{ParameterCreateSpec, ParameterOperation, ParameterPatch};

#[test]
fn manual_mesh_command_requires_exact_candidate_identity_and_sequence() {
    use zipzi_core::Mesh;
    use zipzi_runtime::modeling::ModelingCommand;
    use zipzi_sdk::ManualMeshOperation;
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let create = rt.begin_edit(client, session, "create mesh", None).unwrap();
    rt.apply_edit(client, create, |edit| {
        edit.create_mesh(Mesh {
            id: id(2),
            vertex_ids: vec![0, 1, 2, 3],
            base_positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            ..Default::default()
        })
    })
    .unwrap();
    rt.commit_edit(client, create).unwrap();
    let a = rt.begin_edit(client, session, "manual", None).unwrap();
    let other = rt.register_client().unwrap();
    let b = rt.begin_edit(other, session, "other", None).unwrap();
    let source_a = rt.edit_snapshot(client, a).unwrap().source;
    let source_b = rt.edit_snapshot(other, b).unwrap().source;
    let command = |expected, operation| ModelingCommand::ManualMesh {
        mesh_id: id(2),
        expected,
        operation,
    };
    assert_eq!(
        rt.apply_modeling(
            client,
            a,
            vec![command(
                source_b,
                ManualMeshOperation::ConstrainEdge { a: 0, b: 2 }
            )]
        ),
        Err(RuntimeError::InvalidReference)
    );
    assert_eq!(rt.edit_sequence(client, a).unwrap(), 0);
    rt.apply_modeling(
        client,
        a,
        vec![command(
            source_a,
            ManualMeshOperation::ConstrainEdge { a: 0, b: 2 },
        )],
    )
    .unwrap();
    assert_eq!(rt.edit_sequence(client, a).unwrap(), 1);
    let candidate = rt.mesh_snapshot(client, session, Some(a), &id(2)).unwrap();
    assert_eq!(
        candidate.geometry.meshing.as_ref().unwrap().constraints[0].endpoints,
        [0, 2]
    );
    assert!(rt
        .mesh_snapshot(client, session, None, &id(2))
        .unwrap()
        .geometry
        .meshing
        .is_none());
    assert_eq!(
        rt.apply_modeling(
            client,
            a,
            vec![command(
                source_a,
                ManualMeshOperation::ReleaseConstraint { a: 0, b: 2 }
            )]
        ),
        Err(RuntimeError::InvalidReference)
    );
    assert_eq!(rt.edit_sequence(client, a).unwrap(), 1);
    let source_next = rt.edit_snapshot(client, a).unwrap().source;
    rt.apply_modeling(
        client,
        a,
        vec![command(
            source_next,
            ManualMeshOperation::ReleaseConstraint { a: 0, b: 2 },
        )],
    )
    .unwrap();
    assert_eq!(rt.edit_sequence(client, a).unwrap(), 2);
    rt.commit_edit(client, a).unwrap();
    assert!(rt
        .mesh_snapshot(client, session, None, &id(2))
        .unwrap()
        .geometry
        .meshing
        .is_some());
}

#[test]
fn manual_mesh_candidate_and_formal_reads_preserve_constraints_in_borrowed_output() {
    use zipzi_core::{MeshConstraint, MeshConstraintKind, MeshMeshing};
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "manual mesh", None).unwrap();
    let structure = MeshMeshing {
        boundary: vec![7, 99, 42],
        constraints: vec![],
    };
    let geometry = MeshGeometry {
        vertex_ids: vec![42, 7, 99],
        positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        triangles: vec![[42, 7, 99]],
        meshing: Some(structure.clone()),
    };
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Mesh {
            operation: MeshOperation::Create {
                spec: MeshCreateSpec {
                    name: "mesh".repeat(4096),
                    geometry,
                    texture_asset_id: String::new(),
                    id: Some(id(2)),
                },
            },
        }],
    )
    .unwrap();
    let preview = rt
        .mesh_snapshot(client, session, Some(edit), &id(2))
        .unwrap();
    assert_eq!(preview.geometry.meshing, Some(structure));
    let borrowed = rt
        .mesh_response(client, session, Some(edit), vec![id(2)], true)
        .unwrap()
        .unwrap();
    let blob = serde_json::from_value(borrowed["blob"].clone()).unwrap();
    assert_eq!(
        &*rt.blob(client, blob).unwrap(),
        zipzi_core::binary_wire::encode(&vec![preview]).unwrap()
    );
    rt.release_blob(client, blob).unwrap();
    rt.commit_edit(client, edit).unwrap();
    let formal = rt.mesh_snapshot(client, session, None, &id(2)).unwrap();
    let edit = rt
        .begin_edit(client, session, "invalid constraint", None)
        .unwrap();
    let mut mesh = rt.session(session).unwrap().mesh(&id(2)).unwrap();
    mesh.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [7, 99],
            kind: MeshConstraintKind::User,
        });
    assert!(rt
        .apply_authoring(
            client,
            edit,
            vec![EditOperation::Modeling {
                operation: Box::new(modeling::ModelingCommand::ReplaceMesh { mesh })
            }]
        )
        .is_err());
    assert_eq!(
        rt.mesh_snapshot(client, session, None, &id(2))
            .unwrap()
            .geometry,
        formal.geometry
    );
}

#[test]
fn manual_mesh_constraint_only_publication_has_history_and_source_sequence() {
    use zipzi_core::{Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing};
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let mut mesh = Mesh {
        id: id(2),
        name: "mesh".repeat(4096),
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        uvs: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        meshing: Some(MeshMeshing {
            boundary: vec![0, 1, 2, 3],
            constraints: vec![],
        }),
        ..Default::default()
    };
    let edit = rt.begin_edit(client, session, "create", None).unwrap();
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Modeling {
            operation: Box::new(modeling::ModelingCommand::CreateMesh { mesh: mesh.clone() }),
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    let evaluation = rt.session(session).unwrap().evaluation_revision();
    mesh = rt.session(session).unwrap().mesh(&id(2)).unwrap();
    let bytes_before = rt.mesh_read_bytes(client, session, None, &[id(2)]).unwrap();
    let edit = rt.begin_edit(client, session, "constrain", None).unwrap();
    mesh.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [0, 2],
            kind: MeshConstraintKind::EdgeSupport,
        });
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Modeling {
            operation: Box::new(modeling::ModelingCommand::ReplaceMesh { mesh }),
        }],
    )
    .unwrap();
    let support = rt
        .mesh_snapshot(client, session, Some(edit), &id(2))
        .unwrap();
    let mut mesh = rt
        .edit_snapshot(client, edit)
        .unwrap()
        .snapshot
        .document()
        .get_mesh(&id(2))
        .unwrap()
        .clone();
    mesh.meshing.as_mut().unwrap().constraints[0].kind = MeshConstraintKind::User;
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Modeling {
            operation: Box::new(modeling::ModelingCommand::ReplaceMesh { mesh }),
        }],
    )
    .unwrap();
    let user = rt
        .mesh_snapshot(client, session, Some(edit), &id(2))
        .unwrap();
    assert_ne!(user.source, support.source);
    assert!(
        rt.mesh_read_bytes(client, session, Some(edit), &[id(2)])
            .unwrap()
            > bytes_before
    );
    let borrowed = rt
        .mesh_response(client, session, Some(edit), vec![id(2)], true)
        .unwrap()
        .unwrap();
    let blob = serde_json::from_value(borrowed["blob"].clone()).unwrap();
    assert_eq!(
        &*rt.blob(client, blob).unwrap(),
        zipzi_core::binary_wire::encode(&vec![user.clone()]).unwrap()
    );
    rt.release_blob(client, blob).unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(rt.session(session).unwrap().history_lengths(), (2, 0));
    assert_eq!(
        rt.session(session).unwrap().evaluation_revision(),
        evaluation
    );
    assert_eq!(
        rt.mesh_snapshot(client, session, None, &id(2))
            .unwrap()
            .geometry,
        user.geometry
    );
}

#[test]
fn modeling_sample_admission_checks_exact_source_before_snapshot_and_keeps_edit_usable() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let other = rt
        .create_session(client, &id(2), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "sample", None).unwrap();
    let write = rt
        .apply_authoring(
            client,
            edit,
            vec![EditOperation::Parameter {
                operation: ParameterOperation::Create { spec: parameter() },
            }],
        )
        .unwrap();
    let query = |session, expected, count| ModelingSampleQuery {
        session,
        edit: Some(edit),
        parameters: vec![Default::default(); count],
        checks: Default::default(),
        expected,
    };
    let sampled = rt
        .sample_modeling(client, query(session, Some(write.source), 2))
        .unwrap();
    assert_eq!(sampled.source, write.source);
    assert_eq!(sampled.samples.frames.len(), 2);
    assert!(matches!(
        rt.sample_modeling(client, query(other, None, 1)),
        Err(RuntimeError::InvalidReference)
    ));
    assert!(matches!(
        rt.sample_modeling(client, query(session, None, 65)),
        Err(RuntimeError::Budget(_))
    ));
    let mut oversized = query(session, None, 1);
    oversized.parameters[0] = (0..257).map(|n| (id(n), 0.)).collect();
    assert!(matches!(
        rt.sample_modeling(client, oversized),
        Err(RuntimeError::Budget(_))
    ));
    rt.apply_authoring(client, edit, vec![]).unwrap();
    assert!(matches!(
        rt.sample_modeling(client, query(session, Some(write.source), 1)),
        Err(RuntimeError::InvalidReference)
    ));
    let mut invalid_checks = query(session, None, 1);
    invalid_checks.checks.min_triangle_area = -1.;
    assert!(matches!(
        rt.sample_modeling(client, invalid_checks),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 2);
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}

#[test]
fn binding_and_sample_response_expansion_refused_without_poisoning_candidate() {
    let mut rt = Runtime::new(RuntimeLimits {
        accounted_bytes: 1 << 20,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "admission", None).unwrap();
    let mut spec = parameter();
    spec.name = "large".repeat(10000);
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Parameter {
            operation: ParameterOperation::Create { spec },
        }],
    )
    .unwrap();
    // Typed frames no longer pay the former JSON expansion multiplier. A
    // concurrent transfer still consumes the same aggregate ceiling.
    rt.refresh_memory_budget();
    let competing = rt
        .reserve_transfer(
            (1usize << 20)
                .saturating_sub(rt.accounted_bytes())
                .saturating_sub(4096),
        )
        .unwrap();
    assert!(matches!(
        rt.sample_modeling(
            client,
            ModelingSampleQuery {
                session,
                edit: Some(edit),
                parameters: vec![Default::default()],
                checks: Default::default(),
                expected: None
            }
        ),
        Err(RuntimeError::Budget(_))
    ));
    drop(competing);
    assert!(matches!(
        rt.mesh_binding_snapshot(
            client,
            MeshBindingQuery {
                session,
                edit: Some(edit),
                ids: Some(vec![id(99); 257])
            }
        ),
        Err(RuntimeError::Budget(_))
    ));
    assert!(matches!(
        rt.mesh_binding_snapshot(
            client,
            MeshBindingQuery {
                session,
                edit: Some(edit),
                ids: Some(vec![id(99)])
            }
        ),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 1);
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}

fn id(n: u32) -> String {
    format!("79100000-0000-4000-8000-{n:012x}")
}

fn parameter() -> ParameterCreateSpec {
    ParameterCreateSpec {
        id: Some(id(4)),
        name: "Angle".into(),
        minimum: -30.,
        maximum: 30.,
        default_value: 0.,
        runtime_id: None,
        kind: None,
        decimal_places: None,
        repeat: None,
    }
}

#[test]
fn typed_parameter_queries_admit_count_bytes_and_exact_source_before_cloning() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let other = rt
        .create_session(client, &id(2), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "parameters", None).unwrap();
    let write = rt
        .apply_authoring(
            client,
            edit,
            vec![
                EditOperation::Parameter {
                    operation: ParameterOperation::Create { spec: parameter() },
                },
                EditOperation::Parameter {
                    operation: ParameterOperation::Patch {
                        id: id(4),
                        patch: ParameterPatch {
                            name: Some("Final".into()),
                            ..Default::default()
                        },
                    },
                },
            ],
        )
        .unwrap();
    assert_eq!(write.ids, [id(4), id(4)]);
    let query = |session, ids| ParameterQuery {
        session,
        edit: Some(edit),
        ids,
    };
    let batch = rt
        .parameter_snapshot(client, query(session, Some(vec![id(4), id(4)])))
        .unwrap();
    assert_eq!(batch.source, write.source);
    assert_eq!(batch.parameters.len(), 2);
    assert_eq!(batch.parameters[0].runtime_id, id(4));
    assert_eq!(batch.parameters[0].decimal_places, 6);
    assert!(matches!(
        rt.parameter_snapshot(client, query(other, None)),
        Err(RuntimeError::InvalidReference)
    ));
    assert!(matches!(
        rt.parameter_snapshot(client, query(session, Some(vec![id(4); 257]))),
        Err(RuntimeError::Budget(_))
    ));
    assert!(matches!(
        rt.parameter_snapshot(client, query(session, Some(vec![id(4), id(99)]))),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 1);
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Parameter {
            operation: ParameterOperation::Patch {
                id: id(4),
                patch: ParameterPatch {
                    name: Some("x".repeat(1 << 20)),
                    ..Default::default()
                },
            },
        }],
    )
    .unwrap();
    assert!(matches!(
        rt.parameter_snapshot(client, query(session, None)),
        Err(RuntimeError::Budget(_))
    ));
    assert_eq!(rt.edit_state(client, edit).unwrap(), EditState::Editing);
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Parameter {
            operation: ParameterOperation::Patch {
                id: id(4),
                patch: ParameterPatch {
                    name: Some("Admitted".into()),
                    ..Default::default()
                },
            },
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(
        rt.document(session)
            .unwrap()
            .get_parameter(&id(4))
            .unwrap()
            .name,
        "Admitted"
    );
}

#[test]
fn typed_event_pages_are_non_consuming_and_metadata_cursor_can_resync() {
    let mut rt = Runtime::new(RuntimeLimits {
        session_events: 2,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let initial = rt.subscribe_info(session).unwrap();
    let other_reader = rt.subscribe_info(session).unwrap();
    let edit = rt.begin_edit(client, session, "parameter", None).unwrap();
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Parameter {
            operation: ParameterOperation::Create { spec: parameter() },
        }],
    )
    .unwrap();
    assert!(rt.events_page(initial.cursor, 1).unwrap().is_empty());
    rt.commit_edit(client, edit).unwrap();
    let first = rt.events_page(initial.cursor, 1).unwrap();
    let same = rt.events_page(other_reader.cursor, 1).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first[0], &same[0]));
    let EventKind::Published(receipt) = &first[0].kind else {
        panic!()
    };
    assert_eq!(receipt.before, initial.info.version);
    for i in 0..3 {
        let edit = rt.begin_edit(client, session, "patch", None).unwrap();
        rt.apply_authoring(
            client,
            edit,
            vec![EditOperation::Parameter {
                operation: ParameterOperation::Patch {
                    id: id(4),
                    patch: ParameterPatch {
                        name: Some(format!("value {i}")),
                        ..Default::default()
                    },
                },
            }],
        )
        .unwrap();
        rt.commit_edit(client, edit).unwrap();
    }
    assert!(matches!(
        rt.events_page(initial.cursor, 1),
        Err(RuntimeError::ResyncRequired)
    ));
    let fresh = rt.subscribe_info(session).unwrap();
    assert_eq!(
        fresh.info.version,
        rt.session_info(session).unwrap().version
    );
    assert!(rt.events_page(fresh.cursor, 1).unwrap().is_empty());
    for limit in [0, 257] {
        assert!(matches!(
            rt.events_page(fresh.cursor, limit),
            Err(RuntimeError::Budget(_))
        ));
    }
    let mut future = fresh.cursor;
    future.sequence += 1;
    assert!(matches!(
        rt.events_page(future, 1),
        Err(RuntimeError::ResyncRequired)
    ));
    let foreign = Runtime::default().subscribe_registry().cursor;
    assert!(matches!(
        rt.events_page(foreign, 1),
        Err(RuntimeError::ResyncRequired)
    ));
    let mut foreign = Runtime::default();
    let owner = foreign.register_client().unwrap();
    let s = foreign
        .create_session(owner, &id(2), ModelSetup::default())
        .unwrap();
    assert!(matches!(
        rt.events_page(foreign.subscribe_info(s).unwrap().cursor, 1),
        Err(RuntimeError::ResyncRequired)
    ));
}

#[test]
fn typed_event_expansion_refuses_a_large_payload_before_adapter_conversion() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let initial = rt.subscribe_info(session).unwrap();
    let edit = rt
        .begin_edit(client, session, &"large label".repeat(120_000), None)
        .unwrap();
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Parameter {
            operation: ParameterOperation::Create { spec: parameter() },
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert!(matches!(
        rt.events_page(initial.cursor, 1),
        Err(RuntimeError::Budget(_))
    ));
    let fresh = rt.subscribe_info(session).unwrap();
    assert_eq!(fresh.info.history_lengths, (1, 0));
    assert!(rt.events_page(fresh.cursor, 1).unwrap().is_empty());
}

#[test]
fn selected_mesh_batch_admits_total_expansion_before_any_snapshot_copy() {
    use zipzi_runtime::authoring::MeshBatchQuery;
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "mesh batch", None).unwrap();
    let geometry = MeshGeometry {
        vertex_ids: vec![0, 1, 2],
        positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        triangles: vec![[0, 1, 2]],
        meshing: None,
    };
    rt.edit_meshes(
        client,
        edit,
        vec![MeshOperation::Create {
            spec: MeshCreateSpec {
                name: "A".into(),
                geometry,
                id: Some(id(5)),
                texture_asset_id: String::new(),
            },
        }],
    )
    .unwrap();
    let query = |ids| MeshBatchQuery {
        session,
        edit: Some(edit),
        ids,
    };
    let values = rt
        .mesh_snapshots(client, query(vec![id(5), id(5)]))
        .unwrap();
    assert_eq!(values[0].source, values[1].source);
    assert_eq!(
        rt.mesh_snapshots(client, query(vec![id(5); 256]))
            .unwrap()
            .len(),
        256
    );
    rt.refresh_memory_budget();
    let competing = rt
        .reserve_transfer(
            RuntimeLimits::default()
                .accounted_bytes
                .saturating_sub(rt.accounted_bytes())
                .saturating_sub(4096),
        )
        .unwrap();
    assert!(matches!(
        rt.mesh_snapshots(client, query(vec![id(5); 256])),
        Err(RuntimeError::Budget(_))
    ));
    drop(competing);
    assert!(matches!(
        rt.mesh_snapshots(client, query(vec![id(5), id(99)])),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 1);
    rt.edit_meshes(
        client,
        edit,
        vec![MeshOperation::Rename {
            expected_name: None,
            id: id(5),
            name: "After refusal".into(),
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(
        rt.mesh_snapshot(client, session, None, &id(5))
            .unwrap()
            .name,
        "After refusal"
    );
}

#[test]
fn selected_mesh_query_checks_exact_session_and_admits_before_copy_without_poisoning() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let other = rt
        .create_session(client, &id(2), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "large", None).unwrap();
    let base = MeshGeometry {
        vertex_ids: vec![0, 1, 2, 3],
        positions: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        uvs: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        meshing: None,
    };
    let geometry = zipzi_sdk::rectangle_grid_geometry(&base, 100, 100).unwrap();
    rt.edit_meshes(
        client,
        edit,
        vec![MeshOperation::Create {
            spec: MeshCreateSpec {
                id: Some(id(3)),
                name: "Grid".into(),
                texture_asset_id: String::new(),
                geometry,
            },
        }],
    )
    .unwrap();
    assert!(matches!(
        rt.mesh_snapshot(client, other, Some(edit), &id(3)),
        Err(RuntimeError::InvalidReference)
    ));
    assert!(rt
        .mesh_snapshot(client, session, Some(edit), &id(3))
        .is_ok());
    rt.refresh_memory_budget();
    let competing = rt
        .reserve_transfer(
            RuntimeLimits::default()
                .accounted_bytes
                .saturating_sub(rt.accounted_bytes())
                .saturating_sub(4096),
        )
        .unwrap();
    assert!(matches!(
        rt.mesh_snapshot(client, session, Some(edit), &id(3)),
        Err(RuntimeError::Budget(_))
    ));
    drop(competing);
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 1);
    rt.edit_meshes(
        client,
        edit,
        vec![MeshOperation::Rename {
            expected_name: None,
            id: id(3),
            name: "Renamed".into(),
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(
        rt.document(session).unwrap().get_mesh(&id(3)).unwrap().name,
        "Renamed"
    );
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}

#[path = "../../../fixtures/mesh-edit-dependencies.rs"]
mod mesh_dependencies;
#[test]
fn manual_mesh_mixed_candidate_rejection_preserves_sequence_baseline_and_history() {
    use zipzi_runtime::modeling::ModelingCommand;
    use zipzi_sdk::{ManualMeshOperation as Op, ManualVertexTarget};
    let mut rt = Runtime::default();
    let c = rt.register_client().unwrap();
    let id = mesh_dependencies::id;
    let s = rt.create_session(c, &id(1), ModelSetup::default()).unwrap();
    let e = rt.begin_edit(c, s, "fixture", None).unwrap();
    rt.apply_edit(c, e, mesh_dependencies::install).unwrap();
    rt.commit_edit(c, e).unwrap();
    let history = rt.session(s).unwrap().history_lengths();
    let e = rt.begin_edit(c, s, "drag", None).unwrap();
    for (uv, ok) in [
        (Vec2::new(0.3, 0.4), true),
        (Vec2::new(1., 0.5), false),
        (Vec2::new(0.7, 0.6), true),
    ] {
        let source = rt.edit_snapshot(c, e).unwrap().source;
        let sequence = rt.edit_sequence(c, e).unwrap();
        let result = rt.apply_authoring(
            c,
            e,
            vec![EditOperation::Modeling {
                operation: Box::new(ModelingCommand::ManualMesh {
                    mesh_id: id(2),
                    expected: source,
                    operation: Op::MoveVertices {
                        targets: vec![ManualVertexTarget { id: 120, uv }],
                        motion_from: None,
                    },
                }),
            }],
        );
        assert_eq!(result.is_ok(), ok);
        if let Ok(r) = result {
            assert_eq!(r.mesh_transfer.unwrap().shape_count, 7);
        } else {
            assert_eq!(rt.edit_sequence(c, e).unwrap(), sequence);
        }
    }
    let candidate = rt.edit_snapshot(c, e).unwrap();
    let mut expected = rt
        .session(s)
        .unwrap()
        .begin_owned_edit("direct", None)
        .unwrap();
    expected
        .edit_manual_mesh(
            &id(2),
            Op::MoveVertices {
                targets: vec![ManualVertexTarget {
                    id: 120,
                    uv: Vec2::new(0.7, 0.6),
                }],
                motion_from: None,
            },
        )
        .unwrap();
    assert_eq!(
        candidate.snapshot.document().get_binding(&id(6)),
        expected.candidate_document().get_binding(&id(6))
    );
    rt.commit_edit(c, e).unwrap();
    assert_eq!(rt.session(s).unwrap().history_lengths(), (history.0 + 1, 0));
}
