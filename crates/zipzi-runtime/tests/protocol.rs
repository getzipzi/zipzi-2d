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
            runtime: rt.instance_id(),
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
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![zipzi_contract::kernel_commands::Command::CreateAsset {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Unsaved".into(),
            }],
        )
        .unwrap();
    let version = rt.publish(client, draft, "Unsaved").unwrap().version;
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    let live = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    rt.attach(live, session).unwrap();
    p.disconnect(client, connection, now);
    p.release_disconnected_clients(&mut rt);
    assert!(p.has_connected_clients());
    assert!(rt.list_sessions(client).is_err());
    assert_eq!(rt.session_keys(live).unwrap(), [session]);
    assert_eq!(rt.subscribe(live, session).unwrap().cursor, version);
    assert_eq!(
        rt.close_session(live, session, false).unwrap_err().code,
        "UNSAVED_CHANGES"
    );
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
                Default::default(),
            )
            .unwrap();
        let version = rt.subscribe(client, session).unwrap().cursor;
        let draft = rt.begin_draft(client, version).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let (draft, _) = rt
            .commands(
                client,
                draft,
                vec![zipzi_contract::kernel_commands::Command::CreateAsset {
                    id: id.clone(),
                    name: "Bad image".into(),
                }],
            )
            .unwrap();
        let operation = if from_blob {
            let blob = rt.create_blob(client, 3).unwrap();
            rt.append_blob(client, blob, 0, &[1, 2, 3]).unwrap();
            rt.seal_blob(client, blob).unwrap();
            json!({"op":"replace_image_blob","expected":draft,"id":id,"blob":blob,"width":1,"height":1})
        } else {
            json!({"op":"batch","expected":draft,"commands":[{"command":"replace_image","id":id,"input":{"kind":"rgba","width":1,"height":1,"bytes":[1,2,3]}}]})
        };
        let response = request(
            &mut rt,
            &mut p,
            client,
            connection,
            1,
            operation.clone(),
            now,
        );
        assert_eq!(response["error"]["code"], "INVALID_IMAGE", "{response}");
        assert_eq!(
            request(&mut rt, &mut p, client, connection, 1, operation, now),
            response
        );
        let publication = request(
            &mut rt,
            &mut p,
            client,
            connection,
            2,
            json!({"op":"publish","expected":draft,"description":"Bad image"}),
            now,
        );
        assert_eq!(publication["error"]["code"], "DRAFT_ENDED");
        let after = rt.subscribe(client, session).unwrap();
        assert_eq!(after.cursor, version);
        assert!(after.snapshot.snapshot.document().asset(&id).is_none());
        assert_eq!(after.history.undo_steps, 0);
    }
}

#[test]
fn handshake_extensions_do_not_block_existing_operations() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let now = Instant::now();
    let hello = p.hello(
        &mut rt,
        json!({"protocol": CONTRACT, "client_version": "99.0", "extensions": ["future"]}),
        now,
    );
    assert_eq!(hello["ok"]["protocol"], CONTRACT);
    let client: ClientId = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    let connection = hello["ok"]["connection"].as_u64().unwrap();
    let envelope = json!({"protocol": CONTRACT, "runtime": rt.instance_id(),
        "client": client, "request_id": 1, "trace_id": "future-metadata",
        "operation": {"op": "list_sessions"}});
    let response = p.handle(
        &mut rt,
        client,
        connection,
        serde_json::from_value(envelope).unwrap(),
        now,
    );
    assert_eq!(response["ok"], json!([]));

    // Unknown operations fail locally to the call; the connection stays usable.
    let unsupported = request(
        &mut rt,
        &mut p,
        client,
        connection,
        2,
        json!({"op": "future_operation"}),
        now,
    );
    assert_eq!(unsupported["error"]["code"], "INVALID_REQUEST");
    // Operation arguments remain strict: ignoring a future mutation option could
    // silently change its meaning, unlike optional envelope metadata.
    let invalid = request(
        &mut rt,
        &mut p,
        client,
        connection,
        3,
        json!({"op": "create_session", "setup": ModelSetup::default(), "future_option": true}),
        now,
    );
    assert!(invalid.get("error").is_some());
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            4,
            json!({"op": "list_sessions"}),
            now,
        )["ok"],
        json!([])
    );
}

#[test]
fn incompatible_protocol_and_explicit_build_pin_never_register_clients() {
    let mut rt = Runtime::new(RuntimeLimits {
        clients: 1,
        ..Default::default()
    });
    let mut p = Protocol::default();
    let now = Instant::now();
    for protocol in [
        "zipzi-runtime-v3",
        "zipzi-runtime-r8-quality-topology-1",
        "",
    ] {
        let response = p.hello(&mut rt, json!({"protocol": protocol}), now);
        assert_eq!(response["error"]["code"], "PROTOCOL_MISMATCH");
        assert!(!p.has_connected_clients());
    }
    let response = p.hello(
        &mut rt,
        json!({"protocol": CONTRACT, "sdk_build": "different-build"}),
        now,
    );
    assert_eq!(response["error"]["code"], "BUILD_MISMATCH");
    assert!(!p.has_connected_clients());
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    assert_eq!(hello["ok"]["sdk_build"], SDK_BUILD);
}

#[test]
fn incompatible_protocol_and_old_instance_never_mutate() {
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
    let response = p.handle(
        &mut fresh,
        client,
        connection,
        Request {
            protocol: CONTRACT.into(),
            runtime: rt.instance_id(),
            client,
            request_id: 1,
            operation: json!({"op":"list_sessions"}),
        },
        now,
    );
    assert_eq!(response["error"]["code"], "HOST_RESTARTED");
    let other = fresh.register_client().unwrap();
    assert!(fresh.list_sessions(other).unwrap().is_empty());
}
#[test]
fn a07_all_requests_deduplicate_digest_and_expired_watermark() {
    let (mut rt, mut p, client, connection, now) = setup(ProtocolLimits {
        results_per_client: 2,
        result_ttl: Duration::from_secs(1),
        ..Default::default()
    });
    let op = json!({"op":"create_session", "document_id": uuid::Uuid::new_v4().to_string()});
    let result = request(&mut rt, &mut p, client, connection, 1, op.clone(), now);
    assert!(result.get("ok").is_some(), "{result}");
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op.clone(), now),
        result
    );
    assert_eq!(rt.list_sessions(client).unwrap().len(), 1);
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
    let version = rt.subscribe(client, session).unwrap().cursor;
    for index in 0..100 {
        let response = request(
            &mut rt,
            &mut p,
            client,
            connection,
            2 * index + 1,
            json!({"op":"begin_draft","expected":version}),
            now,
        );
        let draft: DraftSource =
            serde_json::from_value(response["ok"].clone()).unwrap_or_else(|_| panic!("{response}"));
        assert_eq!(
            request(
                &mut rt,
                &mut p,
                client,
                connection,
                2 * index + 2,
                json!({"op":"cancel_draft","draft":draft.draft}),
                now
            ),
            json!({"ok":null})
        );
        assert!(p.retained_results().1 <= 2048);
        assert_eq!(
            rt.read_draft(client, draft).unwrap_err().code,
            "DRAFT_ENDED"
        );
    }
    assert_eq!(rt.task_bytes(), p.retained_results().1);
    assert_eq!(rt.subscribe(client, session).unwrap().history.undo_steps, 0);
    drop(p);
    assert_eq!(rt.task_bytes(), 0);
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
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
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
    assert!(rt.read_draft(client, draft).is_ok());
    p.disconnect(client, resumed["connection"].as_u64().unwrap(), now);
    p.expire(&mut rt, now + Duration::from_secs(31));
    assert!(rt.read_draft(client, draft).is_err());
    let observer = rt.register_client().unwrap();
    rt.attach(observer, session).unwrap();
    assert_eq!(rt.subscribe(observer, session).unwrap().cursor, version);
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
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (draft, _) = rt
        .batch(client, draft, |c| {
            c.create_parameter(zipzi_core::Parameter {
                id: id.clone(),
                runtime_id: "ParamOriginal".into(),
                name: "original".into(),
                decimal_places: 3,
                kind: zipzi_core::ParameterKind::BlendShape,
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
        })
        .unwrap();
    let op = json!({"op":"batch","expected":draft,"commands":[{"command":"patch_parameter","id":id,"patch":{"name":"replacement","default_value":0.5}}]});
    let result = request(&mut rt, &mut p, client, connection, 1, op.clone(), now);
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op, now),
        result
    );
    let draft = serde_json::from_value(result["ok"]["source"].clone())
        .unwrap_or_else(|_| panic!("{result}"));
    let candidate = rt.read_draft(client, draft).unwrap();
    let parameter = candidate.snapshot.document().parameter(&id).unwrap();
    assert_eq!(parameter.name, "replacement");
    assert_eq!(parameter.default_value, 0.5);
    assert_eq!(parameter.runtime_id, "ParamOriginal");
    assert_eq!(parameter.decimal_places, 3);
    assert_eq!(parameter.kind, zipzi_core::ParameterKind::BlendShape);
    let explicit = json!({"op":"batch","expected":draft,"commands":[{"command":"patch_parameter","id":id,"patch":{"runtime_id":"ParamChanged","decimal_places":6,"kind":"normal"}}]});
    let result = request(&mut rt, &mut p, client, connection, 2, explicit, now);
    let draft = serde_json::from_value(result["ok"]["source"].clone())
        .unwrap_or_else(|_| panic!("{result}"));
    let candidate = rt.read_draft(client, draft).unwrap();
    let parameter = candidate.snapshot.document().parameter(&id).unwrap();
    assert_eq!(parameter.runtime_id, "ParamChanged");
    assert_eq!(parameter.decimal_places, 6);
    assert_eq!(parameter.kind, zipzi_core::ParameterKind::Normal);
    assert_eq!(
        rt.publish(client, draft, "Parameter")
            .unwrap()
            .history
            .undo_steps,
        1
    );
}

#[test]
fn modeling_query_rejects_expansion_before_json_and_keeps_candidate_usable() {
    let mut rt = Runtime::new(RuntimeLimits {
        result_bytes: 8192,
        ..Default::default()
    });
    let mut p = Protocol::default();
    let now = Instant::now();
    let hello = p.hello(&mut rt, json!({"protocol": CONTRACT}), now);
    let client = serde_json::from_value(hello["ok"]["client"].clone()).unwrap();
    let connection = hello["ok"]["connection"].as_u64().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let asset = uuid::Uuid::new_v4().to_string();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![zipzi_contract::kernel_commands::Command::CreateAsset {
                id: asset.clone(),
                name: "x".repeat(4096),
            }],
        )
        .unwrap();
    let address = json!({"kind":"asset","id":asset});
    let query = |source, count| json!({"op":"query","source":source,"input":{"query":"objects","addresses":vec![address.clone(); count]}});
    let op = query(QuerySource::Draft(draft), 3);
    let result = request(&mut rt, &mut p, client, connection, 1, op.clone(), now);
    assert_eq!(result["error"]["code"], "RESOURCE_LIMIT");
    assert!(serde_json::to_vec(&result).unwrap().len() < 1024);
    assert_eq!(
        request(&mut rt, &mut p, client, connection, 1, op, now),
        result
    );
    assert_eq!(rt.subscribe(client, session).unwrap().history.undo_steps, 0);
    let publication = rt.publish(client, draft, "Asset").unwrap();
    let source = QuerySource::Published(publication.version);
    let oversized = request(
        &mut rt,
        &mut p,
        client,
        connection,
        2,
        query(source, 3),
        now,
    );
    assert_eq!(oversized["error"]["code"], "RESOURCE_LIMIT");
    let small = request(
        &mut rt,
        &mut p,
        client,
        connection,
        3,
        query(source, 1),
        now,
    );
    assert_eq!(
        small["ok"]["value"]["records"][0]["value"]["name"],
        "x".repeat(4096)
    );
    assert_eq!(
        rt.subscribe(client, session).unwrap().cursor,
        publication.version
    );
}

#[test]
fn job_status_cancel_and_explicit_client_release_preserve_formal_resources() {
    use zipzi_contract::kernel_commands::{Command, ImageInput};
    let (mut rt, mut protocol, client, connection, now) = setup(Default::default());
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let asset = uuid::Uuid::new_v4().to_string();
    let replace = |bytes| Command::ReplaceImage {
        id: asset.clone(),
        input: ImageInput::Rgba {
            width: 1,
            height: 1,
            bytes,
        },
    };
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![
                Command::CreateAsset {
                    id: asset.clone(),
                    name: "A".into(),
                },
                replace(vec![12, 34, 56, 255]),
            ],
        )
        .unwrap();
    let publication = rt.publish(client, draft, "Formal").unwrap();
    let draft = rt.begin_draft(client, publication.version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![replace(vec![11, 22, 33, 255])])
        .unwrap();
    let job = rt
        .request_publish(client, draft, "Cancel", Default::default())
        .unwrap();
    let status = json!({"op":"job","job":job});
    let queued = request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        1,
        status.clone(),
        now,
    );
    assert_eq!(queued["ok"]["state"], "queued");
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
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    let cancelled = request(&mut rt, &mut protocol, client, connection, 3, status, now);
    assert_eq!(cancelled["ok"]["state"], "cancelled");
    assert!(request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        4,
        json!({"op":"release_job","job":job}),
        now
    )
    .get("ok")
    .is_some());
    assert!(rt.job(client, job).is_err());
    let release = json!({"op":"release_client"});
    let result = request(
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
        result
    );
    let observer = rt.register_client().unwrap();
    rt.attach(observer, session).unwrap();
    let after = rt.subscribe(observer, session).unwrap();
    assert_eq!(after.cursor, publication.version);
    assert_eq!(after.history.undo_steps, 1);
    let asset = after.snapshot.snapshot.document().asset(&asset).unwrap();
    assert_eq!(
        after
            .snapshot
            .snapshot
            .resources()
            .read_texture(asset)
            .unwrap()
            .rgba
            .as_ref(),
        &[12, 34, 56, 255]
    );
}

#[test]
fn desktop_open_projects_admits_bounded_batches_and_retries_idempotently() {
    let (mut rt, mut protocol, client, connection, now) = setup(ProtocolLimits::default());
    let queue = std::sync::Arc::new(std::sync::Mutex::new(ProjectOpenQueue::default()));
    protocol = protocol
        .with_window_support()
        .with_project_open_support(queue.clone());
    let path = std::env::temp_dir().join("中文 project.zipzi");
    let op = json!({"op": "open_projects", "paths": [path]});
    assert!(request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        1,
        op.clone(),
        now
    )
    .get("ok")
    .is_some());
    assert!(
        request(&mut rt, &mut protocol, client, connection, 1, op, now)
            .get("ok")
            .is_some()
    );
    assert_eq!(queue.lock().unwrap().pop(), Some(path.clone()));
    assert!(queue.lock().unwrap().is_empty());
    for paths in [
        json!([]),
        json!(["relative.zipzi"]),
        json!(vec![path.clone(); 129]),
    ] {
        let result = request(
            &mut rt,
            &mut protocol,
            client,
            connection,
            2 + paths.as_array().unwrap().len() as u64,
            json!({"op":"open_projects", "paths":paths}),
            now,
        );
        assert_eq!(result["error"]["code"], "OPEN_QUEUE_LIMIT");
        assert!(queue.lock().unwrap().is_empty());
    }
    queue
        .lock()
        .unwrap()
        .enqueue(vec![path.clone(); 128])
        .unwrap();
    assert!(queue.lock().unwrap().enqueue(vec![path.clone()]).is_err());
    assert_eq!(queue.lock().unwrap().pop(), Some(path.clone()));
    queue.lock().unwrap().enqueue(vec![path]).unwrap();
}
