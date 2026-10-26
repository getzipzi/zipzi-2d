use std::sync::Arc;
use zipzi_core::ModelSetup;
use zipzi_runtime::*;
use zipzi_sdk::{AuthoringSession, ModelingIntent, ResourceEdit};
fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    (rt, client, session)
}
#[test]
fn image_preparation_failure_poisoning_is_shared_by_typed_and_blob_writes() {
    for from_blob in [false, true] {
        let (mut rt, client, session) = fixture(Default::default());
        let edit = rt.begin_edit(client, session, "bad image", None).unwrap();
        let parameter_id = uuid::Uuid::new_v4().to_string();
        rt.apply_modeling(
            client,
            edit,
            vec![zipzi_runtime::modeling::ModelingCommand::CreateParameter {
                parameter: zipzi_core::Parameter {
                    id: parameter_id.clone(),
                    ..Default::default()
                },
            }],
        )
        .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let result = if from_blob {
            let blob = rt.create_blob(client, 3).unwrap();
            rt.append_blob(client, blob, 0, &[1, 2, 3]).unwrap();
            rt.seal_blob(client, blob).unwrap();
            let result = rt.apply_rgba_blob(client, edit, id, "bad".into(), 1, 1, blob, false);
            rt.release_blob(client, blob).unwrap();
            result
        } else {
            rt.apply_rgba_image(client, edit, id, "bad".into(), 1, 1, vec![1, 2, 3], false)
        };
        assert!(matches!(result, Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "INVALID_IMAGE"));
        assert_eq!(rt.commit_edit(client, edit), Err(RuntimeError::Aborted));
        assert!(rt
            .document(session)
            .unwrap()
            .get_parameter(&parameter_id)
            .is_none());
        assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
        rt.release_edit(client, edit).unwrap();
        assert_eq!(rt.image_memory_usage().total_bytes(), 0);
    }
}

#[test]
fn batch_admission_retains_source_but_mid_batch_sdk_failure_aborts_everything() {
    let (mut rt, client, session) = fixture(Default::default());
    let edit = rt.begin_edit(client, session, "batch", None).unwrap();
    let commands = vec![
        zipzi_runtime::modeling::ModelingCommand::RenameMesh {
            id: "missing".into(),
            name: "x".into()
        };
        1025
    ];
    assert_eq!(
        rt.apply_modeling(client, edit, commands),
        Err(RuntimeError::Budget("modeling commands"))
    );
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), 0);
    let parameter_id = uuid::Uuid::new_v4().to_string();
    let commands = vec![
        zipzi_runtime::modeling::ModelingCommand::CreateParameter {
            parameter: zipzi_core::Parameter {
                id: parameter_id.clone(),
                ..Default::default()
            },
        },
        zipzi_runtime::modeling::ModelingCommand::EraseObject {
            id: "missing".into(),
        },
    ];
    assert!(matches!(
        rt.apply_modeling(client, edit, commands),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(rt.commit_edit(client, edit), Err(RuntimeError::Aborted));
    assert!(rt
        .document(session)
        .unwrap()
        .get_parameter(&parameter_id)
        .is_none());
    assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
}
#[test]
fn a17_blob_reserves_before_growth_and_shared_pixels_are_charged_once() {
    let (mut rt, client, session) = fixture(RuntimeLimits {
        image_bytes: 16,
        ..Default::default()
    });
    let blob = rt.create_blob(client, 4).unwrap();
    assert_eq!(rt.image_memory_usage().preparing_bytes, 4);
    assert!(rt.create_blob(client, 13).is_err());
    assert_eq!(rt.blob_count(), 1);
    assert!(rt.append_blob(client, blob, 1, &[7, 0, 0, 255]).is_err());
    assert_eq!(rt.append_blob(client, blob, 0, &[7, 0, 0, 255]).unwrap(), 4);
    rt.seal_blob(client, blob).unwrap();
    assert_eq!(rt.image_memory_usage().blob_bytes, 4);
    let image = zipzi_sdk::PreparedImageContent::from_rgba(
        1,
        1,
        rt.blob(client, blob).unwrap(),
        rt.image_memory_budget(),
    )
    .unwrap();
    assert_eq!(rt.image_memory_usage().total_bytes(), 4);
    let edit = rt.begin_edit(client, session, "pixels", None).unwrap();
    let image_id = uuid::Uuid::new_v4().to_string();
    rt.apply_edit(client, edit, |edit| {
        edit.create_prepared_image(image_id.clone(), "pixels".into(), &image)
    })
    .unwrap();
    drop(image);
    rt.release_blob(client, blob).unwrap();
    assert_eq!(rt.blob_count(), 0);
    assert_eq!(rt.image_memory_usage().rgba_bytes, 4);
    rt.release_edit(client, edit).unwrap();
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}
#[test]
fn a17_expiry_releases_blobs_and_plans_while_formal_history_retains_images() {
    let (mut rt, client, session) = fixture(Default::default());
    let image_id = uuid::Uuid::new_v4().to_string();
    let image = zipzi_sdk::PreparedImageContent::from_rgba(
        1,
        1,
        Arc::from([8, 0, 0, 255]),
        rt.image_memory_budget(),
    )
    .unwrap();
    let edit = rt.begin_edit(client, session, "image", None).unwrap();
    rt.apply_edit(client, edit, |e| {
        e.create_prepared_image(image_id.clone(), "image".into(), &image)
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    drop(image);
    let (plan, _, _) = rt
        .plan_modeling(
            client,
            session,
            None,
            ModelingIntent::Resource(ResourceEdit::PatchName {
                id: image_id.clone(),
                expected: "image".into(),
                name: "new".into(),
            }),
        )
        .unwrap();
    let other = rt.register_client().unwrap();
    assert_eq!(
        rt.release_modeling_plan(other, plan),
        Err(RuntimeError::WrongOwner)
    );
    rt.create_blob(client, 4096).unwrap();
    assert_eq!(rt.image_memory_usage().preparing_bytes, 4096);
    rt.release_client(client);
    assert_eq!(rt.blob_count(), 0);
    assert_eq!(rt.modeling_plan_count(), 0);
    assert_eq!(rt.image_memory_usage().total_bytes(), 4);
    assert_eq!(
        rt.session(session)
            .unwrap()
            .read_texture(&image_id)
            .unwrap()
            .rgba[0],
        8
    );
}
#[test]
fn a05_plan_from_another_candidate_or_old_sequence_never_applies() {
    let (mut rt, client, session) = fixture(Default::default());
    let resource = uuid::Uuid::new_v4().to_string();
    let first = rt.begin_edit(client, session, "first", None).unwrap();
    rt.apply_edit(client, first, |e| {
        e.edit_resource(ResourceEdit::Create {
            id: resource.clone(),
            name: "image".into(),
            before: None,
        })
    })
    .unwrap();
    let intent = ModelingIntent::Resource(ResourceEdit::PatchName {
        id: resource.clone(),
        expected: "image".into(),
        name: "new".into(),
    });
    let (plan, _, _) = rt
        .plan_modeling(client, session, Some(first), intent)
        .unwrap();
    rt.apply_edit(client, first, |_| Ok(())).unwrap();
    assert!(
        matches!(rt.apply_modeling_plan(client, first, plan), Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "STALE_PLAN")
    );
    assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
    assert!(rt.document(session).unwrap().get_asset(&resource).is_none());
}
#[test]
fn a17_adopting_oversized_images_does_not_register_a_session() {
    let mut sdk =
        AuthoringSession::new(&uuid::Uuid::new_v4().to_string(), ModelSetup::default()).unwrap();
    let image = zipzi_sdk::PreparedImageContent::from_rgba(
        1,
        1,
        Arc::from([0, 0, 0, 255]),
        &Default::default(),
    )
    .unwrap();
    sdk.import_prepared_image(uuid::Uuid::new_v4().to_string(), "image".into(), &image)
        .unwrap();
    let mut rt = Runtime::new(RuntimeLimits {
        image_bytes: 3,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    assert!(rt.adopt_session(client, sdk).is_err());
    assert!(rt.list_sessions().is_empty());
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}

#[test]
fn a17_plan_input_growth_is_admitted_before_plan_registration() {
    let (mut rt, client, session) = fixture(RuntimeLimits {
        plan_bytes: 65536,
        ..Default::default()
    });
    let resource = uuid::Uuid::new_v4().to_string();
    let intent = ModelingIntent::Resource(ResourceEdit::Create {
        id: resource.clone(),
        name: "large".repeat(10000),
        before: None,
    });
    assert_eq!(
        rt.plan_modeling(client, session, None, intent).unwrap_err(),
        RuntimeError::Budget("plan bytes")
    );
    assert_eq!(rt.modeling_plan_count(), 0);
    assert!(rt.document(session).unwrap().get_asset(&resource).is_none());
    assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
}
