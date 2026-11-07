use super::*;
fn connect(rt: &mut Runtime, p: &mut Protocol) -> (ClientId, u64, Value) {
    let hello = p.hello(rt, json!({"protocol":CONTRACT}), Instant::now());
    assert!(hello.get("ok").is_some(), "{hello}");
    (
        serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
        hello["ok"]["connection"].as_u64().unwrap(),
        hello["ok"].clone(),
    )
}
fn request(
    rt: &mut Runtime,
    p: &mut Protocol,
    c: ClientId,
    connection: u64,
    id: u64,
    operation: Value,
) -> Value {
    p.handle(
        rt,
        c,
        connection,
        Request {
            protocol: CONTRACT.into(),
            runtime: rt.instance_id(),
            client: c,
            request_id: id,
            operation,
        },
        Instant::now(),
    )
}

#[test]
fn draft_error_lifecycle_is_owned_and_cached_with_the_original_result() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let (client, connection, _) = connect(&mut rt, &mut p);
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )
        .unwrap();
    let version = rt.session_read(client, session).unwrap().version();
    let draft = rt.begin_draft(client, version).unwrap();
    let command =
        json!({"command":"create_asset","id":uuid::Uuid::new_v4().to_string(),"name":"asset"});
    let op = json!({"op":"batch","expected":draft,"commands":vec![command;4097]});
    let refused = request(&mut rt, &mut p, client, connection, 1, op.clone());
    assert_eq!(refused["error"]["code"], "RESOURCE_LIMIT");
    assert_eq!(refused["error"]["draft_state"], "ready");
    rt.cancel_draft(client, draft.draft).unwrap();
    // Replaying the response must not consult the current state of the candidate.
    assert_eq!(request(&mut rt, &mut p, client, connection, 1, op), refused);
    let ended = p.execute_response(
        &mut rt,
        client,
        Operation::Batch {
            expected: draft,
            commands: vec![],
        },
    );
    assert_eq!(ended["error"]["draft_state"], "ended");

    let owned = rt.begin_draft(client, version).unwrap();
    let other = rt.register_client().unwrap();
    let forbidden = p.execute_response(
        &mut rt,
        other,
        Operation::Batch {
            expected: owned,
            commands: vec![],
        },
    );
    assert_eq!(forbidden["error"]["code"], "NOT_OWNER");
    assert!(forbidden["error"].get("draft_state").is_none());
    assert!(rt.read_draft(client, owned).is_ok());
}
#[test]
fn v4_rejects_old_protocol_before_registration_and_checks_resume_ownership() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    assert_eq!(
        p.hello(
            &mut rt,
            json!({"protocol":"zipzi-runtime-v3"}),
            Instant::now()
        )["error"]["code"],
        "PROTOCOL_MISMATCH"
    );
    let (c, connection, hello) = connect(&mut rt, &mut p);
    let bad = p.hello(
        &mut rt,
        json!({"protocol":CONTRACT,"runtime":hello["runtime"],"client":c,"secret":"wrong"}),
        Instant::now(),
    );
    assert_eq!(bad["error"]["code"], "WRONG_OWNER");
    let resumed = p.hello(
        &mut rt,
        json!({"protocol":CONTRACT,"runtime":hello["runtime"],"client":c,"secret":hello["secret"]}),
        Instant::now(),
    );
    let next = resumed["ok"]["connection"].as_u64().unwrap();
    p.disconnect(c, connection, Instant::now());
    assert!(p.has_connected_clients());
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            c,
            connection,
            1,
            json!({"op":"list_sessions"})
        )["error"]["code"],
        "CLIENT_EXPIRED"
    );
    assert!(
        request(&mut rt, &mut p, c, next, 1, json!({"op":"list_sessions"}))
            .get("ok")
            .is_some()
    );
}
#[test]
fn mutations_replay_the_same_result_and_recovery_probes_never_advance_watermark() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let (c, k, _) = connect(&mut rt, &mut p);
    let op = json!({"op":"create_session","document_id":null});
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            c,
            k,
            999,
            json!({"op":"request_status","request_id":1})
        )["ok"]["state"],
        "not_executed"
    );
    let first = request(&mut rt, &mut p, c, k, 1, op.clone());
    assert_eq!(request(&mut rt, &mut p, c, k, 1, op), first);
    assert_eq!(rt.list_sessions(c).unwrap().len(), 1);
    assert_eq!(
        request(&mut rt, &mut p, c, k, 1, json!({"op":"list_sessions"}))["error"]["code"],
        "REQUEST_ID_REUSED"
    );
    let recovered = request(
        &mut rt,
        &mut p,
        c,
        k,
        999,
        json!({"op":"request_status","request_id":1}),
    );
    assert_eq!(recovered["ok"]["response"], first);
    let version = first["ok"]["version"].clone();
    let started = request(
        &mut rt,
        &mut p,
        c,
        k,
        2,
        json!({"op":"begin_draft","expected":version}),
    );
    let source = started["ok"].clone();
    let failed = request(
        &mut rt,
        &mut p,
        c,
        k,
        3,
        json!({"op":"batch","expected":source,"commands":[{"command":"rename","target":{"kind":"parameter","id":"missing"},"name":"bad"}]}),
    );
    assert!(failed.get("error").is_some());
    let read = request(
        &mut rt,
        &mut p,
        c,
        k,
        4,
        json!({"op":"query","source":{"kind":"draft","source":source},"input":{"query":"summary"}}),
    );
    assert_eq!(read["error"]["code"], "DRAFT_ENDED");
}
#[test]
fn cached_result_expiry_never_reexecutes_and_disconnect_expiry_keeps_sessions() {
    let mut rt = Runtime::default();
    let mut p = Protocol::new(ProtocolLimits {
        results_per_client: 1,
        ..Default::default()
    });
    let (c, k, _) = connect(&mut rt, &mut p);
    let create = json!({"op":"create_session","document_id":null});
    let made = request(&mut rt, &mut p, c, k, 1, create.clone());
    request(&mut rt, &mut p, c, k, 2, json!({"op":"list_sessions"}));
    assert_eq!(
        request(&mut rt, &mut p, c, k, 1, create)["error"]["code"],
        "REQUEST_EXPIRED"
    );
    assert_eq!(rt.list_sessions(c).unwrap().len(), 1);
    let session: SessionKey = serde_json::from_value(made["ok"]["session"].clone()).unwrap();
    let now = Instant::now();
    p.disconnect(c, k, now);
    p.expire(&mut rt, now + Duration::from_secs(31));
    assert!(rt.list_sessions(c).is_err());
    let (c, _, _) = connect(&mut rt, &mut p);
    rt.attach(c, session).unwrap();
    assert_eq!(rt.list_sessions(c).unwrap().len(), 1);
}
#[test]
fn query_pages_bind_kind_and_exact_source_without_address_handles() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let (c, k, _) = connect(&mut rt, &mut p);
    let s = request(
        &mut rt,
        &mut p,
        c,
        k,
        1,
        json!({"op":"create_session","document_id":null}),
    );
    let version = s["ok"]["version"].clone();
    let d = request(
        &mut rt,
        &mut p,
        c,
        k,
        2,
        json!({"op":"begin_draft","expected":version}),
    )["ok"]
        .clone();
    let added = request(
        &mut rt,
        &mut p,
        c,
        k,
        3,
        json!({"op":"batch","expected":d,"commands":[{"command":"create_parameter","spec":{"name":"Angle","minimum":-1,"maximum":1,"default_value":0}}]}),
    );
    let d = added["ok"]["source"].clone();
    let id = added["ok"]["outcomes"][0]["created"].clone();
    let source = json!({"kind":"draft","source":d});
    let q = request(
        &mut rt,
        &mut p,
        c,
        k,
        4,
        json!({"op":"query","source":source,"input":{"query":"collection","kind":"parameter","offset":0,"limit":1}}),
    );
    assert_eq!(q["ok"]["value"]["records"][0]["value"]["name"], "Angle");
    let wrong = request(
        &mut rt,
        &mut p,
        c,
        k,
        5,
        json!({"op":"query","source":source,"input":{"query":"objects","addresses":[{"kind":"mesh","id":id}]}}),
    );
    assert_eq!(wrong["error"]["code"], "MISSING_OBJECT");
    let published = request(
        &mut rt,
        &mut p,
        c,
        k,
        6,
        json!({"op":"publish","expected":d,"description":"parameter"}),
    );
    assert!(published.get("ok").is_some(), "{published}");
    let stale = request(
        &mut rt,
        &mut p,
        c,
        k,
        7,
        json!({"op":"query","source":{"kind":"published","source":version},"input":{"query":"summary"}}),
    );
    assert_eq!(stale["error"]["code"], "STALE_SOURCE");
}

#[test]
fn released_client_can_recover_close_but_cannot_execute_new_operations() {
    let mut rt = Runtime::default();
    let mut protocol = Protocol::default();
    let (client, connection, hello) = connect(&mut rt, &mut protocol);
    let operation = json!({"op":"release_client"});
    let response = request(
        &mut rt,
        &mut protocol,
        client,
        connection,
        1,
        operation.clone(),
    );
    assert_eq!(response, success(Value::Null));
    let resumed = protocol.hello(&mut rt, json!({"protocol":CONTRACT,"runtime":hello["runtime"],"client":client,"secret":hello["secret"]}), Instant::now());
    let next = resumed["ok"]["connection"].as_u64().unwrap();
    assert_eq!(
        request(&mut rt, &mut protocol, client, next, 1, operation),
        response
    );
    assert_eq!(
        request(
            &mut rt,
            &mut protocol,
            client,
            next,
            2,
            json!({"op":"create_session","document_id":null})
        )["error"]["code"],
        "CLIENT_RELEASED"
    );
    assert_eq!(
        protocol
            .execute(&mut rt, client, Operation::RequestStatus { request_id: 1 })
            .unwrap_err()
            .code,
        "INVALID_REQUEST"
    );
}

#[test]
fn binary_recovery_retains_exact_bytes_and_rejects_payload_substitution() {
    let mut rt = Runtime::default();
    let mut p = Protocol::default();
    let (client, connection, _) = connect(&mut rt, &mut p);
    let auth = crate::local::Auth { client, connection };
    let blob: BlobId = serde_json::from_value(
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            1,
            json!({"op":"create_blob","length":4}),
        )["ok"]
            .clone(),
    )
    .unwrap();
    let bytes = vec![10, 20, 30, 255];
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let envelope = |id, operation| json!({"protocol":CONTRACT,"runtime":rt.instance_id(),"client":client,"request_id":id,"operation":operation});
    let upload = envelope(
        2,
        json!({"op":"append_blob_binary","blob":blob,"offset":0,"length":4,"payload_sha256":digest}),
    );
    let download = envelope(
        4,
        json!({"op":"read_blob_binary","blob":blob,"offset":1,"length":2}),
    );
    assert_eq!(
        p.receive_binary(&mut rt, auth, upload.clone(), bytes.clone(), Instant::now())
            .value["ok"],
        4
    );
    assert_eq!(
        p.receive_binary(&mut rt, auth, upload.clone(), bytes, Instant::now())
            .value["ok"],
        4
    );
    assert_eq!(
        p.receive_binary(&mut rt, auth, upload, vec![1, 2, 3, 4], Instant::now())
            .value["error"]["code"],
        "INVALID_REQUEST"
    );
    assert_eq!(
        request(
            &mut rt,
            &mut p,
            client,
            connection,
            3,
            json!({"op":"seal_blob","blob":blob})
        )["ok"],
        4
    );
    let response = p.receive_binary(&mut rt, auth, download.clone(), vec![], Instant::now());
    assert_eq!(response.payload.unwrap().bytes(), &[20, 30]);
    assert!(request(
        &mut rt,
        &mut p,
        client,
        connection,
        5,
        json!({"op":"release_blob","blob":blob})
    )
    .get("ok")
    .is_some());
    assert_eq!(
        p.receive_binary(&mut rt, auth, download, vec![], Instant::now())
            .payload
            .unwrap()
            .bytes(),
        &[20, 30]
    );
}
