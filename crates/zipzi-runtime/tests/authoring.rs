use zipzi_core::{ModelSetup, Vec2};
use zipzi_runtime::authoring::{EditOperation, ParameterQuery};
use zipzi_runtime::authoring::{MeshBindingQuery, ModelingSampleQuery};
use zipzi_runtime::*;
use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};
use zipzi_sdk::{ParameterCreateSpec, ParameterOperation, ParameterPatch};

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
    assert!(
        rt.mesh_snapshot(client, session, Some(edit), &id(3))
            .is_ok()
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
