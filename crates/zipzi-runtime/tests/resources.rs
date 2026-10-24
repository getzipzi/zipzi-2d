use zipzi_core::{ModelSetup, Vec2};
use zipzi_runtime::{authoring::*, *};
use zipzi_sdk::{ImageCreateSpec, MeshCreateSpec, MeshGeometry, MeshOperation};

fn id(n: u32) -> String {
    format!("79400000-0000-4000-8000-{n:012x}")
}
fn setup() -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    (rt, client, session)
}
fn mesh() -> EditOperation {
    EditOperation::Mesh {
        operation: MeshOperation::Create {
            spec: MeshCreateSpec {
                id: Some(id(3)),
                name: "Face".into(),
                texture_asset_id: id(2),
                geometry: MeshGeometry {
                    vertex_ids: vec![42, 7, 99],
                    positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    triangles: vec![[42, 7, 99]],
                },
            },
        },
    }
}

#[test]
fn prepared_a_b_candidate_plan_and_history_hold_exact_pixels_until_last_owner() {
    let (mut rt, client, session) = setup();
    let a = rt
        .prepare_image(client, 1, 1, vec![12, 34, 56, 255])
        .unwrap()
        .image;
    let b = rt
        .prepare_image(client, 1, 1, vec![250, 128, 64, 255])
        .unwrap()
        .image;
    assert_eq!(rt.image_memory_usage().rgba_bytes, 8);
    let edit = rt.begin_edit(client, session, "pixels", None).unwrap();
    let result = rt
        .apply_authoring(
            client,
            edit,
            vec![
                EditOperation::CreateImage {
                    spec: ImageCreateSpec {
                        id: Some(id(2)),
                        name: "A".into(),
                    },
                    image: a,
                },
                mesh(),
            ],
        )
        .unwrap();
    assert!(matches!(result.source, SourceRef::Edit { sequence: 1, .. }));
    let plan = rt
        .plan_authoring(
            client,
            session,
            Some(edit),
            PlanIntent::ImageRename {
                id: id(2),
                expected_name: "A".into(),
                name: "Texture".into(),
            },
        )
        .unwrap();
    assert_eq!(plan.report.affected_ids, vec![id(2)]);
    rt.release_prepared_image(client, a).unwrap();
    assert_eq!(
        rt.image_snapshot(client, session, Some(edit), &id(2))
            .unwrap()
            .pixels
            .rgba
            .as_ref(),
        &[12, 34, 56, 255]
    );
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::ApplyPlan { plan: plan.plan }],
    )
    .unwrap();
    rt.release_modeling_plan(client, plan.plan).unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(
        rt.image_snapshot(client, session, None, &id(2))
            .unwrap()
            .name,
        "Texture"
    );
    assert_eq!(
        rt.prepared_image_pixels(client, b).unwrap().rgba.as_ref(),
        &[250, 128, 64, 255]
    );
    rt.release_prepared_image(client, b).unwrap();
    assert_eq!(rt.image_memory_usage().rgba_bytes, 4);
    rt.undo(client, session).unwrap();
    assert_eq!(rt.image_memory_usage().rgba_bytes, 4); // redo retains the image
    rt.redo(client, session).unwrap();
    assert_eq!(
        rt.image_snapshot(client, session, None, &id(2))
            .unwrap()
            .pixels
            .rgba
            .as_ref(),
        &[12, 34, 56, 255]
    );
    assert_eq!(rt.prepared_image_count(), 0);
    rt.close_session(client, session, true).unwrap();
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}

#[test]
fn bad_or_foreign_image_write_aborts_on_host_without_client_cleanup() {
    let (mut rt, client, session) = setup();
    let other = rt.register_client().unwrap();
    let image = rt
        .prepare_image(other, 1, 1, vec![1, 2, 3, 255])
        .unwrap()
        .image;
    let edit = rt.begin_edit(client, session, "foreign", None).unwrap();
    let result = rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::CreateImage {
            spec: ImageCreateSpec {
                id: Some(id(2)),
                name: "foreign".into(),
            },
            image,
        }],
    );
    assert!(matches!(result, Err(RuntimeError::WrongOwner)));
    assert_eq!(rt.commit_edit(client, edit), Err(RuntimeError::Aborted));
    rt.release_edit(client, edit).unwrap();
    rt.release_client(other);
    assert_eq!(rt.prepared_image_count(), 0);
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}

#[test]
fn preparation_failure_is_independent_and_hundred_cancelled_sources_reclaim_leases() {
    let (mut rt, client, session) = setup();
    for index in 0..100 {
        let edit = rt.begin_edit(client, session, "cancel", None).unwrap();
        assert!(rt.prepare_image(client, 1, 1, vec![1, 2, 3]).is_err());
        assert_eq!(rt.edit_state(client, edit).unwrap(), EditState::Editing);
        let image = rt
            .prepare_image(client, 1, 1, vec![index, 2, 3, 255])
            .unwrap()
            .image;
        rt.apply_authoring(
            client,
            edit,
            vec![EditOperation::CreateImage {
                spec: ImageCreateSpec {
                    id: Some(id(2)),
                    name: "draft".into(),
                },
                image,
            }],
        )
        .unwrap();
        let snapshot = rt.edit_snapshot(client, edit).unwrap();
        let plan = rt
            .plan_authoring(
                client,
                session,
                Some(edit),
                PlanIntent::ImageRename {
                    id: id(2),
                    expected_name: "draft".into(),
                    name: "planned".into(),
                },
            )
            .unwrap();
        rt.release_prepared_image(client, image).unwrap();
        rt.release_edit(client, edit).unwrap();
        rt.release_modeling_plan(client, plan.plan).unwrap();
        assert_eq!(
            snapshot
                .snapshot
                .memory_image(&id(2))
                .unwrap()
                .texture()
                .rgba[0],
            index
        );
        assert_eq!(rt.image_memory_usage().rgba_bytes, 4);
        drop(snapshot);
        assert_eq!(rt.image_memory_usage().total_bytes(), 0);
        assert_eq!(rt.prepared_image_count(), 0);
        assert_eq!(rt.modeling_plan_count(), 0);
    }
    assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
}

#[test]
fn plan_from_exact_source_is_stale_after_a_prior_batch_operation() {
    let (mut rt, client, session) = setup();
    let edit = rt.begin_edit(client, session, "image", None).unwrap();
    let image = rt
        .prepare_image(client, 1, 1, vec![1, 2, 3, 255])
        .unwrap()
        .image;
    rt.apply_authoring(
        client,
        edit,
        vec![
            EditOperation::CreateImage {
                spec: ImageCreateSpec {
                    id: Some(id(2)),
                    name: "image".into(),
                },
                image,
            },
            mesh(),
        ],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    let plan = rt
        .plan_authoring(
            client,
            session,
            None,
            PlanIntent::MeshRename {
                id: id(3),
                expected_name: "Face".into(),
                name: "Planned".into(),
            },
        )
        .unwrap();
    let edit = rt.begin_edit(client, session, "batch", None).unwrap();
    let error = rt
        .apply_authoring(
            client,
            edit,
            vec![
                EditOperation::Mesh {
                    operation: MeshOperation::Rename {
                        id: id(3),
                        name: "Partial".into(),
                    },
                },
                EditOperation::ApplyPlan { plan: plan.plan },
            ],
        )
        .unwrap_err();
    assert!(matches!(error, RuntimeError::Sdk(e) if e.code.as_ref() == "STALE_PLAN"));
    assert_eq!(rt.commit_edit(client, edit), Err(RuntimeError::Aborted));
    assert_eq!(
        rt.document(session).unwrap().get_mesh(&id(3)).unwrap().name,
        "Face"
    );
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}

#[test]
fn preparation_admission_and_client_expiry_reclaim_only_owned_leases() {
    let mut rt = Runtime::new(RuntimeLimits {
        prepared_images_per_client: 1,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let other = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "independent", None).unwrap();
    let a = rt
        .prepare_image(client, 1, 1, vec![1, 2, 3, 255])
        .unwrap()
        .image;
    assert!(matches!(
        rt.prepare_image(client, 1, 1, vec![1, 2, 3, 255]),
        Err(RuntimeError::Budget("prepared images"))
    ));
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 0);
    assert_eq!(rt.edit_state(client, edit).unwrap(), EditState::Editing);
    let b = rt
        .prepare_image(other, 1, 1, vec![4, 5, 6, 255])
        .unwrap()
        .image;
    assert_eq!(rt.image_memory_usage().rgba_bytes, 8);
    rt.release_client(client);
    assert_eq!(rt.prepared_image_count(), 1);
    assert_eq!(rt.image_memory_usage().rgba_bytes, 4);
    assert!(rt.prepared_image_pixels(other, a).is_err());
    assert_eq!(
        rt.prepared_image_pixels(other, b).unwrap().rgba.as_ref(),
        &[4, 5, 6, 255]
    );
    rt.release_client(other);
    assert_eq!(rt.prepared_image_count(), 0);
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}
