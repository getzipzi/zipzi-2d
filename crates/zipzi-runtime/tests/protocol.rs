use serde_json::{json, Value};
use std::time::{Duration, Instant};
use zipzi_core::ModelSetup;
use zipzi_runtime::{protocol::*, *};
fn setup(limits: ProtocolLimits) -> (Runtime, Protocol, ClientId, u64, Instant) {
    let mut rt = Runtime::default();
    let mut p = Protocol::new(limits);
    let now = Instant::now();
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    let client = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    let connection = hello["ok"]["connection"].as_u64().unwrap();
    (rt, p, client, connection, now)
}
fn request(
    rt: &mut Runtime,
    p: &mut Protocol,
    client: ClientId,
    connection: u64,
    id: u64,
    op: Value,
    now: Instant,
) -> Value {
    p.handle(
        rt,
        client,
        connection,
        Request {
            protocol: CONTRACT.into(),
            runtime: client.runtime_instance(),
            client,
            request_id: id,
            operation: op,
        },
        now,
    )
}
#[test]
fn connected_clients_exclude_reconnect_grace_and_ignore_old_socket_disconnects() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let now = Instant::now();
    assert!(!p.has_connected_clients());
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    let client = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    let connection = hello["ok"]["connection"].as_u64().unwrap();
    assert!(p.has_connected_clients());
    let second = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    p.disconnect(client, connection, now);
    assert!(p.has_connected_clients());
    p.disconnect(
        serde_json::from_value(second["ok"]["client"].clone()).unwrap(),
        second["ok"]["connection"].as_u64().unwrap(),
        now,
    );
    assert!(!p.has_connected_clients());
    let instance = rt.instance_id();
    let resumed = p.hello(
        &mut rt,
        json!({"protocol": CONTRACT, "runtime": instance,
            "client": client, "secret": hello["ok"]["secret"]}),
        now,
    );
    assert!(resumed.get("ok").is_some(), "{resumed}");
    p.disconnect(client, connection, now);
    assert!(
        p.has_connected_clients(),
        "old socket displaced a resumed connection"
    );
    p.disconnect(client, resumed["ok"]["connection"].as_u64().unwrap(), now);
    assert!(!p.has_connected_clients());
}

#[test]
fn shutdown_releases_disconnected_clients_but_preserves_unsaved_sessions_and_live_peers() {
    let (mut rt, mut p, client, connection, now) = setup(Default::default());
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    let edit = rt.begin_edit(client, session, "unsaved", None).unwrap();
    rt.apply_edit(client, edit, |e| {
        e.replace_model_setup(zipzi_core::Canvas::new(200., 100., Default::default(), 1.))
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    let version = rt.session_info(session).unwrap().version;
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    let live = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    p.disconnect(client, connection, now);
    p.release_disconnected_clients(&mut rt);
    assert!(p.has_connected_clients());
    assert_eq!(rt.live_counts().0, 1);
    assert!(rt.list_jobs(live).is_ok());
    assert!(rt.list_jobs(client).is_err());
    let info = rt.session_info(session).unwrap();
    assert_eq!(info.version, version);
    assert!(info.modified);
    rt.close_session(live, session, true).unwrap();
}

#[test]
fn image_prepare_failure_aborts_on_host_even_if_client_never_sends_cleanup() {
    for from_blob in [false, true] {
        let (mut rt, mut p, client, connection, now) = setup(Default::default());
        let session = rt
            .create_session(
                client,
                &uuid::Uuid::new_v4().to_string(),
                ModelSetup::default(),
            )
            .unwrap();
        let edit = rt.begin_edit(client, session, "bad image", None).unwrap();
        let version = rt.session_info(session).unwrap().version;
        let parameter = uuid::Uuid::new_v4().to_string();
        rt.apply_modeling(
            client,
            edit,
            vec![zipzi_runtime::modeling::ModelingCommand::CreateParameter {
                parameter: zipzi_core::Parameter {
                    id: parameter.clone(),
                    ..Default::default()
                },
            }],
        )
        .unwrap();
        let mut operation = json!({"op":"apply_rgba", "edit":edit, "id":uuid::Uuid::new_v4().to_string(),
            "name":"bad", "width":1, "height":1, "rgba":[1, 2, 3], "replace":false});
        if from_blob {
            let blob = rt.create_blob(client, 3).unwrap();
            rt.append_blob(client, blob, 0, &[1, 2, 3]).unwrap();
            rt.seal_blob(client, blob).unwrap();
            operation["op"] = json!("apply_rgba_blob");
            operation.as_object_mut().unwrap().remove("rgba");
            operation["blob"] = json!(blob);
        }
        let response = request(
            &mut rt,
            &mut p,
            client,
            connection,
            1,
            operation.clone(),
            now,
        );
        assert_eq!(response["error"]["code"], "INVALID_IMAGE");
        assert_eq!(
            request(&mut rt, &mut p, client, connection, 1, operation, now),
            response
        );
        assert_eq!(
            request(
                &mut rt,
                &mut p,
                client,
                connection,
                2,
                json!({"op":"commit", "edit":edit}),
                now
            )["error"]["code"],
            "EDIT_ABORTED"
        );
        assert_eq!(rt.session_info(session).unwrap().version, version);
        assert!(rt
            .document(session)
            .unwrap()
            .get_parameter(&parameter)
            .is_none());
        assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
    }
}
#[test]
fn a06_strict_handshake_and_old_instance_never_register_or_mutate() {
    let (mut rt, mut p, client, connection, now) = setup(Default::default());
    let response = p.hello(&mut rt, json!({"protocol": "zipzi-runtime-r3-1"}), now);
    assert_eq!(response["error"]["code"], "PROTOCOL_MISMATCH");
    let mut fresh = Runtime::default();
    assert_eq!(
        p.hello(
            &mut fresh,
            json!({"protocol": CONTRACT, "runtime": rt.instance_id()}),
            now
        )["error"]["code"],
        "HOST_RESTARTED"
    );
    assert_eq!(
        request(
            &mut fresh,
            &mut p,
            client,
            connection,
            1,
            json!({"op":"list_sessions"}),
            now
        )["error"]["code"],
        "HOST_RESTARTED"
    );
    assert!(fresh.list_sessions().is_empty());
}
#[test]
fn a07_all_requests_deduplicate_digest_and_expired_watermark() {
    let (mut rt, mut p, client, connection, now) = setup(ProtocolLimits {
        results_per_client: 2,
        result_ttl: Duration::from_secs(1),
        ..Default::default()
    });
    let op = json!({"op":"create_session", "document_id": uuid::Uuid::new_v4().to_string(), "setup": ModelSetup::default()});
    let result = request(&mut rt, &mut p, client, connection, 1, op.clone(), now);
    assert!(result.get("ok").is_some(), "{result}");
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op.clone(), now),
        result
    );
    assert_eq!(rt.list_sessions().len(), 1);
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            1,
            json!({"op":"list_sessions"}),
            now
        )["error"]["code"],
        "REQUEST_ID_REUSED"
    );
    for id in 2..=4 {
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            id,
            json!({"op":"list_sessions"}),
            now,
        );
    }
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op, now)["error"]["code"],
        "REQUEST_EXPIRED"
    );
    assert!(p.retained_results().0 <= 2);
    p.expire(&mut rt, now + Duration::from_secs(2));
    assert_eq!(p.retained_results(), (0, 0));
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            4,
            json!({"op":"list_sessions"}),
            now + Duration::from_secs(2)
        )["error"]["code"],
        "REQUEST_EXPIRED"
    );
}
#[test]
fn a17_result_byte_budget_and_hundred_edits_remain_bounded() {
    let (mut rt, mut p, client, connection, now) = setup(ProtocolLimits {
        result_bytes_per_client: 2048,
        result_bytes_total: 2048,
        ..Default::default()
    });
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    let baseline = rt
        .accounted_bytes()
        .saturating_sub(p.retained_memory_bytes());
    let mut id = 0;
    for _ in 0..100 {
        id += 1;
        let response = request(
            &mut rt,
            &mut p,
            client,
            connection,
            id,
            json!({"op":"begin_edit", "session":session,"label":"cancel","expected":null}),
            now,
        );
        let edit = response["ok"].clone();
        assert!(!edit.is_null(), "{response}");
        id += 1;
        assert_eq!(
            request(
                &mut rt,
                &mut p,
                client,
                connection,
                id,
                json!({"op":"release_edit", "edit":edit}),
                now
            ),
            json!({"ok":null})
        );
        assert!(p.retained_results().1 <= 2048);
        assert!(p.retained_memory_bytes() <= 2048 * 128 + 1024);
    }
    assert_eq!(
        rt.accounted_bytes()
            .saturating_sub(p.retained_memory_bytes()),
        baseline
    );
    assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
}
#[test]
fn a18_disconnect_resume_generation_and_expiry_reclaim_candidate_not_session() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let now = Instant::now();
    let hello = p.hello(&mut rt, json!({"protocol":CONTRACT}), now)["ok"].clone();
    let client: ClientId = serde_json::from_value(hello["client"].clone()).unwrap();
    let generation = hello["connection"].as_u64().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    let edit = rt.begin_edit(client, session, "pending", None).unwrap();
    p.disconnect(client, generation, now);
    let instance = rt.instance_id();
    let resumed = p.hello(
        &mut rt,
        json!({"protocol":CONTRACT,"runtime":instance,"client":client,"secret":hello["secret"]}),
        now,
    )["ok"]
        .clone();
    assert!(resumed["connection"].as_u64().unwrap() > generation);
    p.disconnect(client, generation, now); // An old socket cannot expire a new one.
    p.expire(&mut rt, now + Duration::from_secs(31));
    assert!(rt.edit_state(client, edit).is_ok());
    p.disconnect(client, resumed["connection"].as_u64().unwrap(), now);
    p.expire(&mut rt, now + Duration::from_secs(31));
    assert!(rt.edit_state(client, edit).is_err());
    assert!(rt.session_info(session).is_ok());
}
#[test]
fn a06_framing_limits_utf8_nonfinite_empty_arrays_roundtrip() {
    use zipzi_runtime::local::*;
    let original = json!({"array":[], "float":1.2345678901234567, "text":"你好"});
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &original).unwrap();
    assert_eq!(read_frame(&mut bytes.as_slice()).unwrap(), original);
    for length in [0, (MAX_FRAME + 1) as u32, u32::MAX] {
        assert!(read_frame(&mut length.to_be_bytes().as_slice()).is_err());
    }
    for body in [b"[NaN]".as_slice(), b"[1e999]", b"\xff"] {
        let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
        bytes.extend(body);
        assert!(read_frame(&mut bytes.as_slice()).is_err());
    }
}

#[cfg(unix)]
#[test]
fn private_unix_endpoint_permissions_and_ownership() {
    use zipzi_runtime::local::*;
    let temp = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(Endpoint::new(temp.path(), "BAD").is_err());
    let endpoint = Endpoint::new(temp.path(), "a").unwrap();
    let owner = endpoint.claim().unwrap().unwrap();
    assert!(endpoint.claim().unwrap().is_none());
    drop(owner);
    assert!(endpoint.claim().unwrap().is_some());
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Endpoint::new(temp.path(), "a").is_err());
}

#[test]
fn a07_status_of_unexecuted_id_does_not_expire_its_retry() {
    let (mut rt, mut p, client, connection, now) = setup(Default::default());
    let status = request(
        &mut rt,
        &mut p,
        client,
        connection,
        0,
        json!({"op":"request_status", "request_id":1}),
        now,
    );
    assert_eq!(status["ok"]["state"], "not_executed");
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            1,
            json!({"op":"list_sessions"}),
            now
        ),
        json!({"ok":[]})
    );
}

#[test]
fn a04_parameter_replacement_preserves_candidate_fields_in_one_request() {
    let (mut rt, mut p, client, connection, now) = setup(Default::default());
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    let edit = rt
        .begin_edit(client, session, "replace in candidate", None)
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    rt.apply_edit(client, edit, |candidate| {
        candidate.create_parameter(zipzi_core::Parameter {
            id: id.clone(),
            runtime_id: "ParamOriginal".into(),
            name: "original".into(),
            decimal_places: 3,
            kind: zipzi_core::ParameterKind::BlendShape,
            minimum: 0.0,
            maximum: 1.0,
            ..Default::default()
        })
    })
    .unwrap();
    let op = json!({"op":"apply", "edit":edit, "commands":[{
        "op":"replace_parameter", "parameter":{
            "id":id, "runtime_id":"placeholder", "name":"replacement",
            "minimum":0.0, "maximum":1.0, "default_value":0.5,
            "kind":"normal", "decimal_places":6, "repeat":false
        }, "preserve_runtime_id":true, "preserve_kind":true, "preserve_decimal_places":true
    }]});
    let result = request(&mut rt, &mut p, client, connection, 1, op.clone(), now);
    assert!(result.get("ok").is_some(), "{result}");
    let sequence = rt.edit_sequence(client, edit).unwrap();
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op.clone(), now),
        result
    );
    assert_eq!(rt.edit_sequence(client, edit).unwrap(), sequence);
    let parameter = rt
        .edit_document(client, edit)
        .unwrap()
        .get_parameter(&id)
        .unwrap();
    assert_eq!(parameter.name, "replacement");
    assert_eq!(parameter.runtime_id, "ParamOriginal");
    assert_eq!(parameter.decimal_places, 3);
    assert_eq!(parameter.kind, zipzi_core::ParameterKind::BlendShape);
    let mut explicit = op;
    explicit["commands"][0]["preserve_runtime_id"] = json!(false);
    explicit["commands"][0]["preserve_kind"] = json!(false);
    explicit["commands"][0]["preserve_decimal_places"] = json!(false);
    let result = request(&mut rt, &mut p, client, connection, 2, explicit, now);
    assert!(result.get("ok").is_some(), "{result}");
    let parameter = rt
        .edit_document(client, edit)
        .unwrap()
        .get_parameter(&id)
        .unwrap();
    assert_eq!(parameter.runtime_id, "placeholder");
    assert_eq!(parameter.decimal_places, 6);
    assert_eq!(parameter.kind, zipzi_core::ParameterKind::Normal);
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}

#[test]
fn modeling_query_rejects_expansion_before_json_and_keeps_candidate_usable() {
    let mut rt = Runtime::new(RuntimeLimits {
        accounted_bytes: 1 << 20,
        ..Default::default()
    });
    let mut p = Protocol::new(Default::default());
    let now = Instant::now();
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    let client = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    let connection = hello["ok"]["connection"].as_u64().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    let asset = uuid::Uuid::new_v4().to_string();
    let edit = rt.begin_edit(client, session, "resource", None).unwrap();
    rt.apply_edit(client, edit, |e| {
        e.edit_resource(zipzi_sdk::ResourceEdit::Create {
            id: asset.clone(),
            name: "x".repeat(4096),
            before: None,
        })
    })
    .unwrap();
    let op = json!({"op":"modeling_query", "session":session, "edit":edit,
        "query":{"assets":vec![asset.clone(); 4300]}});
    let result = request(&mut rt, &mut p, client, connection, 1, op.clone(), now);
    assert_eq!(result["error"]["code"], "BUDGET_EXCEEDED");
    assert_eq!(result["error"]["edit_state"], "editing");
    assert!(serde_json::to_vec(&result).unwrap().len() < 1024);
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op, now),
        result
    );
    assert_eq!(rt.session(session).unwrap().history_lengths(), (0, 0));
    rt.commit_edit(client, edit).unwrap();
    let version = rt.session(session).unwrap().version();
    let oversized = request(
        &mut rt,
        &mut p,
        client,
        connection,
        2,
        json!({"op":"modeling_query", "session":session, "query":{"assets":vec![asset.clone(); 4300]}}),
        now,
    );
    assert_eq!(oversized["error"]["code"], "BUDGET_EXCEEDED");
    let small = request(
        &mut rt,
        &mut p,
        client,
        connection,
        3,
        json!({"op":"modeling_query", "session":session, "query":{"assets":[asset, "missing"]}}),
        now,
    );
    assert_eq!(
        small["ok"]["records"]["assets"][0]["name"],
        "x".repeat(4096),
        "{small}"
    );
    assert!(small["ok"]["records"]["assets"][1].is_null());
    assert_eq!(rt.session(session).unwrap().version(), version);
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
}

#[test]
fn job_status_cancel_and_explicit_client_release_preserve_formal_resources() {
    let (mut rt, mut protocol, client, connection, now) = setup(Default::default());
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )
        .unwrap();
    let asset = uuid::Uuid::new_v4().to_string();
    let a = rt
        .prepare_image(client, 1, 1, vec![12, 34, 56, 255])
        .unwrap()
        .image;
    let edit = rt.begin_edit(client, session, "formal", None).unwrap();
    rt.apply_authoring(
        client,
        edit,
        vec![authoring::EditOperation::Resource(
            crate::authoring::ResourceOperation::CreateImage {
                spec: zipzi_sdk::ImageCreateSpec {
                    id: Some(asset.clone()),
                    name: "A".into(),
                },
                image: a,
            },
        )],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    rt.release_prepared_image(client, a).unwrap();
    let b = rt
        .prepare_image(client, 1, 1, vec![11, 22, 33, 255])
        .unwrap()
        .image;
    rt.plan_authoring(
        client,
        session,
        None,
        authoring::PlanIntent::ImageRename {
            id: asset.clone(),
            expected_name: Some("A".into()),
            name: "Planned".into(),
        },
    )
    .unwrap();
    let edit = rt.begin_edit(client, session, "cancel", None).unwrap();
    rt.apply_authoring(
        client,
        edit,
        vec![authoring::EditOperation::Resource(
            crate::authoring::ResourceOperation::CreateImage {
                spec: zipzi_sdk::ImageCreateSpec {
                    id: None,
                    name: "B".into(),
                },
                image: b,
            },
        )],
    )
    .unwrap();
    let job = rt.request_commit(client, edit).unwrap();
    assert_eq!(
        request(
            &mut rt,
            &mut protocol,
            client,
            connection,
            1,
            json!({"op":"job_status","job":job}),
            now
        )["ok"],
        "queued"
    );
    assert!(request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        2,
        json!({"op":"cancel_job","job":job}),
        now
    )
    .get("ok")
    .is_some());
    struct Manual(Vec<Work>);
    impl Executor for Manual {
        fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
            self.0.push(work);
            Ok(())
        }
    }
    let mut executor = Manual(vec![]);
    rt.dispatch(&mut executor);
    rt.complete(executor.0.pop().unwrap().run());
    assert_eq!(
        request(
            &mut rt,
            &mut protocol,
            client,
            connection,
            3,
            json!({"op":"job_status","job":job}),
            now
        )["ok"],
        "cancelled"
    );
    assert_eq!(rt.job_info(client, job).unwrap().state, JobState::Cancelled);
    request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        4,
        json!({"op":"job","job":job}),
        now,
    );
    assert!(rt.job_info(client, job).is_err());
    let release = json!({"op":"release_client"});
    let response = request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        5,
        release.clone(),
        now,
    );
    assert_eq!(
        request(&mut rt, &mut protocol, client, connection, 5, release, now),
        response
    );
    assert_eq!(rt.prepared_image_count(), 0);
    assert_eq!(rt.modeling_plan_count(), 0);
    assert_eq!(rt.image_memory_usage().rgba_bytes, 4);
    assert_eq!(rt.session_info(session).unwrap().history_lengths, (1, 0));
    let observer = rt.register_client().unwrap();
    assert_eq!(
        rt.image_snapshot(observer, session, None, &asset)
            .unwrap()
            .pixels
            .rgba
            .as_ref(),
        &[12, 34, 56, 255]
    );
}
