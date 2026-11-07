use serde_json::{json, Value};
use zipzi_contract::{
    authoring::ObjectKind,
    kernel_commands::{Command, ObjectAddress},
    kernel_tasks::{SampleInput, SampleRequest},
    kernel_wire::{Operation, Query},
};
use zipzi_core::{MeshConstraint, MeshConstraintKind, MeshMeshing, Vec2};
use zipzi_runtime::*;
use zipzi_sdk::{
    ManualMeshOperation, ManualVertexTarget, MeshCreateSpec, MeshGeometry, ParameterCreateSpec,
    ParameterPatch,
};

fn id(n: u32) -> String {
    format!("79100000-0000-4000-8000-{n:012x}")
}

fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    (rt, client, version)
}

fn quad() -> MeshGeometry {
    let points = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(1., 1.),
        Vec2::new(0., 1.),
    ];
    MeshGeometry {
        vertex_ids: vec![0, 1, 2, 3],
        positions: points.clone(),
        uvs: points,
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        meshing: None,
    }
}

fn mesh(geometry: MeshGeometry) -> Command {
    Command::CreateMesh {
        spec: MeshCreateSpec {
            id: Some(id(2)),
            name: "Quad".into(),
            texture_asset_id: String::new(),
            geometry,
        },
    }
}

fn parameter() -> Command {
    Command::CreateParameter {
        spec: ParameterCreateSpec {
            id: Some(id(4)),
            name: "Angle".into(),
            minimum: -30.,
            maximum: 30.,
            default_value: 0.,
            runtime_id: None,
            kind: None,
            decimal_places: None,
            repeat: None,
        },
    }
}

fn patch_parameter(name: String) -> Command {
    Command::PatchParameter {
        id: id(4),
        patch: ParameterPatch {
            name: Some(name),
            ..Default::default()
        },
    }
}

fn query(kind: ObjectKind, ids: impl IntoIterator<Item = String>) -> Query {
    Query::Objects {
        addresses: ids
            .into_iter()
            .map(|id| ObjectAddress { kind, id })
            .collect(),
    }
}

fn sample(values: Vec<zipzi_core::PreviewValues>) -> SampleRequest {
    SampleRequest {
        input: SampleInput::Parameters { values },
        queries: vec![],
        quality: false,
    }
}

fn complete_read(rt: &mut Runtime, client: ClientId, job: JobId) -> Result<ReadResult> {
    while !rt.job(client, job).unwrap().state.is_terminal() {
        let work = rt.take_work(job).unwrap();
        rt.complete(work.run()).unwrap();
    }
    let result = rt.job(client, job).unwrap().result.clone().unwrap();
    rt.release_job(client, job).unwrap();
    let JobValue::Read(result) = result? else {
        panic!("expected read result")
    };
    Ok(result)
}

#[test]
fn manual_mesh_preview_requires_the_exact_candidate_owner_and_sequence() {
    let (mut rt, client, version) = fixture(Default::default());
    let create = rt.begin_draft(client, version).unwrap();
    let (create, _) = rt.commands(client, create, vec![mesh(quad())]).unwrap();
    let version = rt.publish(client, create, "Mesh").unwrap().version;
    let draft = rt.begin_draft(client, version).unwrap();
    let peer = rt.register_client().unwrap();
    rt.attach(peer, version.session).unwrap();
    let constrain = || {
        vec![Command::ManualMesh {
            mesh: id(2),
            operation: ManualMeshOperation::ConstrainEdge { a: 0, b: 2 },
        }]
    };
    assert_eq!(
        rt.preview_commands(peer, draft, constrain())
            .unwrap_err()
            .code,
        "NOT_OWNER"
    );
    let (current, _) = rt.preview_commands(client, draft, constrain()).unwrap();
    assert_eq!(current.sequence, 1);
    let candidate = rt.read_draft(client, current).unwrap();
    let constraints = &candidate
        .snapshot
        .document()
        .mesh(&id(2))
        .unwrap()
        .meshing
        .as_ref()
        .unwrap()
        .constraints;
    assert_eq!(constraints[0].endpoints, [0, 2]);
    assert!(rt
        .read_snapshot(client, version.session)
        .unwrap()
        .snapshot
        .document()
        .mesh(&id(2))
        .unwrap()
        .meshing
        .is_none());
    assert_eq!(
        rt.preview_commands(client, draft, constrain())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        rt.read_draft(client, current).unwrap().source,
        QuerySource::Draft(current)
    );
    let (current, _) = rt
        .preview_commands(
            client,
            current,
            vec![Command::ManualMesh {
                mesh: id(2),
                operation: ManualMeshOperation::ReleaseConstraint { a: 0, b: 2 },
            }],
        )
        .unwrap();
    assert_eq!(current.sequence, 2);
    rt.publish(client, current, "Manual mesh").unwrap();
    assert!(rt
        .read_snapshot(client, version.session)
        .unwrap()
        .snapshot
        .document()
        .mesh(&id(2))
        .unwrap()
        .meshing
        .as_ref()
        .unwrap()
        .constraints
        .is_empty());
}

#[test]
fn selected_mesh_queries_keep_constraints_and_frozen_candidate_sources() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let mut geometry = quad();
    geometry.meshing = Some(MeshMeshing {
        boundary: vec![0, 1, 2, 3],
        constraints: vec![MeshConstraint {
            endpoints: [0, 2],
            kind: MeshConstraintKind::User,
        }],
    });
    let (draft, _) = rt.commands(client, draft, vec![mesh(geometry)]).unwrap();
    let source = QuerySource::Draft(draft);
    let selected = rt
        .query(client, source, query(ObjectKind::Mesh, [id(2)]))
        .unwrap();
    assert_eq!(selected["source"], serde_json::to_value(source).unwrap());
    assert_eq!(
        selected["value"]["records"][0]["value"]["meshing"]["constraints"][0]["endpoints"],
        json!([0, 2])
    );
    assert_eq!(
        rt.query(
            client,
            QuerySource::Published(version),
            query(ObjectKind::Mesh, [id(2)])
        )
        .unwrap_err()
        .code,
        "MISSING_OBJECT"
    );
    let publication = rt.publish(client, draft, "Mesh").unwrap();
    let formal = rt
        .query(
            client,
            QuerySource::Published(publication.version),
            query(ObjectKind::Mesh, [id(2)]),
        )
        .unwrap();
    assert_eq!(formal["value"], selected["value"]);
    assert_eq!(
        rt.query(client, source, query(ObjectKind::Mesh, [id(2)]))
            .unwrap_err()
            .code,
        "DRAFT_ENDED"
    );
}

#[test]
fn manual_mesh_constraint_only_publication_has_history_and_no_geometry_change() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let mut geometry = quad();
    geometry.meshing = Some(MeshMeshing {
        boundary: vec![0, 1, 2, 3],
        constraints: vec![],
    });
    let (draft, _) = rt.commands(client, draft, vec![mesh(geometry)]).unwrap();
    let original = rt.publish(client, draft, "Mesh").unwrap();
    let before = rt.read_snapshot(client, version.session).unwrap();
    let draft = rt.begin_draft(client, original.version).unwrap();
    let (draft, _) = rt
        .preview_commands(
            client,
            draft,
            vec![Command::ManualMesh {
                mesh: id(2),
                operation: ManualMeshOperation::ConstrainEdge { a: 0, b: 2 },
            }],
        )
        .unwrap();
    let candidate = rt.read_draft(client, draft).unwrap();
    let a = before.snapshot.document().mesh(&id(2)).unwrap();
    let b = candidate.snapshot.document().mesh(&id(2)).unwrap();
    assert_eq!(a.base_positions, b.base_positions);
    assert_eq!(a.triangles, b.triangles);
    assert_ne!(a.meshing, b.meshing);
    let published = rt.publish(client, draft, "Constraint").unwrap();
    assert_eq!(published.history.undo_steps, 2);
    rt.undo(client, published.version).unwrap();
    assert_eq!(
        rt.read_snapshot(client, version.session)
            .unwrap()
            .snapshot
            .document()
            .mesh(&id(2))
            .unwrap(),
        a
    );
}

#[test]
fn sample_admission_checks_source_and_bounds_without_poisoning_the_draft() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![parameter()]).unwrap();
    let source = QuerySource::Draft(draft);
    let request = sample(vec![Default::default(); 2]);
    let job = rt.request_samples(client, source, request.clone()).unwrap();
    let result = complete_read(&mut rt, client, job).unwrap();
    assert_eq!(result.source, source);
    let data: Value = serde_json::from_slice(result.bytes()).unwrap();
    assert_eq!(data["frames"].as_array().unwrap().len(), 2);
    assert_eq!(
        rt.request_samples(client, source, sample(vec![Default::default(); 65]))
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    let values = (0..257).map(|n| (id(n), 0.)).collect();
    assert_eq!(
        rt.request_samples(client, source, sample(vec![values]))
            .unwrap_err()
            .code,
        "INVALID_SAMPLES"
    );
    let (current, _) = rt.commands(client, draft, vec![]).unwrap();
    assert_eq!(
        rt.request_samples(client, source, request)
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        rt.publish(client, current, "Parameter")
            .unwrap()
            .history
            .undo_steps,
        1
    );
}

#[test]
fn sample_result_retains_its_lease_through_decoding_and_errors_release_budget() {
    let (mut rt, client, version) = fixture(Default::default());
    let source = QuerySource::Published(version);
    let job = rt
        .request_samples(client, source, sample(vec![Default::default()]))
        .unwrap();
    let result = complete_read(&mut rt, client, job).unwrap();
    let reserved = rt.task_bytes();
    assert!(reserved > 0);
    let value: Value = serde_json::from_slice(result.bytes()).unwrap();
    assert_eq!(value["source"], serde_json::to_value(source).unwrap());
    assert_eq!(value["frames"].as_array().unwrap().len(), 1);
    assert_eq!(rt.task_bytes(), reserved);
    drop(result);
    let job = rt
        .request_samples(client, source, sample(vec![[(id(99), 0.)].into()]))
        .unwrap();
    assert!(complete_read(&mut rt, client, job).is_err());
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn binding_selection_and_sample_reservations_refuse_without_ending_the_draft() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        task_bytes: 256 << 20,
        ..Default::default()
    });
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![parameter()]).unwrap();
    let source = QuerySource::Draft(draft);
    let competing = rt.reserve_transfer(200 << 20).unwrap();
    assert_eq!(
        rt.request_samples(client, source, sample(vec![Default::default()]))
            .unwrap_err()
            .code,
        "BUDGET_EXCEEDED"
    );
    drop(competing);
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.query(
            client,
            source,
            query(ObjectKind::MeshBinding, vec![id(99); 257])
        )
        .unwrap_err()
        .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        rt.query(client, source, query(ObjectKind::MeshBinding, [id(99)]))
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        rt.publish(client, draft, "Parameter")
            .unwrap()
            .history
            .undo_steps,
        1
    );
}

#[test]
fn typed_parameter_queries_check_counts_bytes_and_exact_source() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, outcomes) = rt
        .commands(
            client,
            draft,
            vec![parameter(), patch_parameter("Final".into())],
        )
        .unwrap();
    assert_eq!(outcomes[0].created.as_deref(), Some(id(4).as_str()));
    let select = |ids| query(ObjectKind::Parameter, ids);
    let source = QuerySource::Draft(draft);
    let value = rt
        .query(client, source, select(vec![id(4), id(4)]))
        .unwrap();
    assert_eq!(value["value"]["records"][0], value["value"]["records"][1]);
    let parameter = &value["value"]["records"][0]["value"];
    assert_eq!(parameter["runtime_id"], id(4));
    assert_eq!(parameter["decimal_places"], 6);
    assert_eq!(
        rt.query(client, source, select(vec![id(4); 257]))
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        rt.query(client, source, select(vec![id(99)]))
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    let (large, _) = rt
        .commands(client, draft, vec![patch_parameter("x".repeat(1 << 20))])
        .unwrap();
    assert_eq!(
        rt.query(client, source, select(vec![id(4)]))
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        rt.query(client, QuerySource::Draft(large), select(vec![id(4)]))
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    let (valid, _) = rt
        .commands(client, large, vec![patch_parameter("Admitted".into())])
        .unwrap();
    let published = rt.publish(client, valid, "Parameter").unwrap();
    let value = rt
        .query(
            client,
            QuerySource::Published(published.version),
            select(vec![id(4)]),
        )
        .unwrap();
    assert_eq!(value["value"]["records"][0]["value"]["name"], "Admitted");
}

#[test]
fn publication_reads_are_non_consuming_and_old_cursors_require_resubscription() {
    let (mut rt, client, initial) = fixture(RuntimeLimits {
        events_per_session: 2,
        ..Default::default()
    });
    let peer = rt.register_client().unwrap();
    rt.attach(peer, initial.session).unwrap();
    let draft = rt.begin_draft(client, initial).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![parameter()]).unwrap();
    assert!(rt.publications_since(client, initial).unwrap().is_empty());
    let first = rt.publish(client, draft, "Parameter").unwrap();
    let a = rt.publications_since(client, initial).unwrap();
    let b = rt.publications_since(peer, initial).unwrap();
    assert_eq!(a[0].version, first.version);
    assert!(std::sync::Arc::ptr_eq(&a[0].writes, &b[0].writes));
    let mut version = first.version;
    for n in 0..3 {
        let draft = rt.begin_draft(client, version).unwrap();
        let (draft, _) = rt
            .commands(client, draft, vec![patch_parameter(format!("Label {n}"))])
            .unwrap();
        version = rt.publish(client, draft, "Name").unwrap().version;
    }
    assert_eq!(
        rt.publications_since(peer, initial).unwrap_err().code,
        "EVENT_GAP"
    );
    let fresh = rt.subscribe(peer, initial.session).unwrap();
    assert_eq!(fresh.cursor, version);
    assert!(rt
        .publications_since(client, fresh.cursor)
        .unwrap()
        .is_empty());
    let mut future = version;
    future.revision += 1;
    assert_eq!(
        rt.publications_since(client, future).unwrap_err().code,
        "INVALID_CURSOR"
    );
}

#[test]
fn wire_event_page_refuses_expansion_and_allows_resynchronizing() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        result_bytes: 1024,
        ..Default::default()
    });
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![parameter()]).unwrap();
    let publication = rt.publish(client, draft, "label".repeat(16_384)).unwrap();
    let mut protocol = zipzi_runtime::protocol::Protocol::default();
    let error = protocol
        .execute(&mut rt, client, Operation::Events { cursor: version })
        .unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        1
    );
    assert_eq!(
        protocol
            .execute(
                &mut rt,
                client,
                Operation::Events {
                    cursor: publication.version
                }
            )
            .unwrap(),
        json!([])
    );
}

#[test]
fn selected_mesh_query_bounds_the_whole_result_and_keeps_candidate_usable() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        result_bytes: 32 << 10,
        ..Default::default()
    });
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![mesh(quad())]).unwrap();
    let source = QuerySource::Draft(draft);
    let value = rt
        .query(client, source, query(ObjectKind::Mesh, [id(2), id(2)]))
        .unwrap();
    assert_eq!(value["value"]["records"][0], value["value"]["records"][1]);
    assert_eq!(
        rt.query(client, source, query(ObjectKind::Mesh, vec![id(2); 256]))
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        rt.query(client, source, query(ObjectKind::Mesh, [id(2), id(99)]))
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        rt.publish(client, draft, "Quad")
            .unwrap()
            .history
            .undo_steps,
        1
    );
}

#[test]
fn a_large_grid_can_be_published_after_its_query_exceeds_the_result_bound() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        result_bytes: 64 << 10,
        ..Default::default()
    });
    let draft = rt.begin_draft(client, version).unwrap();
    let grid = zipzi_sdk::rectangle_grid_geometry(&quad(), 100, 100).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![mesh(grid)]).unwrap();
    assert_eq!(
        rt.query(
            client,
            QuerySource::Draft(draft),
            query(ObjectKind::Mesh, [id(2)])
        )
        .unwrap_err()
        .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        rt.read_draft(client, draft)
            .unwrap()
            .snapshot
            .document()
            .mesh(&id(2))
            .unwrap()
            .vertex_ids
            .len(),
        10_201
    );
    assert_eq!(
        rt.publish(client, draft, "Grid")
            .unwrap()
            .history
            .undo_steps,
        1
    );
}

#[path = "../../../fixtures/mesh-edit-dependencies.rs"]
mod mesh_dependencies;

#[test]
fn baseline_preview_retains_the_last_valid_candidate_and_transfers_all_shapes() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt.batch(client, draft, mesh_dependencies::install).unwrap();
    let version = rt.publish(client, draft, "Fixture").unwrap().version;
    let mut draft = rt.begin_draft(client, version).unwrap();
    let preview = |c: &mut zipzi_sdk::Commands<'_>, uv| {
        c.edit_manual_mesh(
            &mesh_dependencies::id(2),
            ManualMeshOperation::MoveVertices {
                targets: vec![ManualVertexTarget { id: 120, uv }],
                motion_from: None,
            },
            Default::default(),
            &Default::default(),
        )
    };
    let mut last = rt.read_draft(client, draft).unwrap();
    for (uv, accepted) in [
        (Vec2::new(0.3, 0.4), true),
        (Vec2::new(1.1, 0.5), false),
        (Vec2::new(0.7, 0.6), true),
    ] {
        match rt.preview_from_baseline(client, draft, |c| preview(c, uv)) {
            Ok((updated, outcome)) => {
                assert!(accepted);
                assert_eq!(outcome.report.shape_count, 7);
                draft = updated;
                last = rt.read_draft(client, draft).unwrap();
            }
            Err(_) => {
                assert!(!accepted);
                assert_eq!(
                    rt.read_draft(client, draft).unwrap().snapshot.content_id(),
                    last.snapshot.content_id()
                );
            }
        }
    }
    let direct = rt.begin_draft(client, version).unwrap();
    let (direct, _) = rt
        .preview_from_baseline(client, direct, |c| preview(c, Vec2::new(0.7, 0.6)))
        .unwrap();
    let expected = rt.read_draft(client, direct).unwrap();
    assert_eq!(
        last.snapshot.document().binding(&mesh_dependencies::id(6)),
        expected
            .snapshot
            .document()
            .binding(&mesh_dependencies::id(6))
    );
    assert_eq!(
        rt.publish(client, draft, "Drag")
            .unwrap()
            .history
            .undo_steps,
        2
    );
}
