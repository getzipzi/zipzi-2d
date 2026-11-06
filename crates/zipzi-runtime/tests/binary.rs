use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Instant;
use zipzi_runtime::{local::Auth, protocol::*, *};
fn fixture() -> (Runtime, Protocol, Auth) {
    let mut rt = Runtime::default();
    let mut protocol = Protocol::default();
    let hello = protocol.hello(&mut rt, json!({"protocol": CONTRACT}), Instant::now());
    let auth = Auth {
        client: serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
        connection: hello["ok"]["connection"].as_u64().unwrap(),
    };
    (rt, protocol, auth)
}
fn header(runtime: RuntimeInstanceId, auth: Auth, id: u64, operation: Value) -> Value {
    json!({"protocol": CONTRACT, "runtime":runtime,"client":auth.client,"request_id":id,"operation":operation})
}
#[test]
fn binary_upload_replay_binds_payload_and_frozen_download_survives_blob_release() {
    let (mut rt, mut p, auth) = fixture();
    let instance = rt.instance_id();
    let blob = rt.create_blob(auth.client, 8).unwrap();
    let data = b"pixels\0\xff".to_vec();
    let hash: String = Sha256::digest(&data)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    let input = header(
        instance,
        auth,
        1,
        json!({"op":"append_blob_binary", "blob":blob,"offset":0,"length":8,"payload_sha256":hash}),
    );
    let first = p.receive_binary(&mut rt, auth, input.clone(), data.clone(), Instant::now());
    assert_eq!(first.value["ok"], 8);
    assert_eq!(
        p.receive_binary(&mut rt, auth, input.clone(), data.clone(), Instant::now())
            .value,
        first.value
    );
    assert_eq!(
        p.receive_binary(&mut rt, auth, input, b"different".to_vec(), Instant::now())
            .value["error"]["code"],
        "INVALID_REQUEST"
    );
    rt.seal_blob(auth.client, blob).unwrap();
    let read = header(
        instance,
        auth,
        2,
        json!({"op":"read_blob_binary","blob":blob,"offset":1,"length":5}),
    );
    let output = p.receive_binary(&mut rt, auth, read.clone(), vec![], Instant::now());
    assert_eq!(output.payload.as_ref().unwrap().bytes(), b"ixels");
    rt.release_blob(auth.client, blob).unwrap();
    assert_eq!(
        rt.blob(auth.client, blob).err().unwrap().code,
        "BLOB_NOT_FOUND"
    );
    assert_eq!(
        p.receive_binary(&mut rt, auth, read, vec![], Instant::now())
            .payload
            .unwrap()
            .bytes(),
        b"ixels"
    );
    drop(p);
    assert_eq!(rt.task_bytes(), 5);
    drop(output);
    assert_eq!(rt.task_bytes(), 0);
}
#[test]
fn binary_controls_cannot_be_smuggled_through_json_or_another_owner() {
    let (mut rt, mut p, auth) = fixture();
    let instance = rt.instance_id();
    let blob = rt.create_blob(auth.client, 4).unwrap();
    let data = b"rgba".to_vec();
    let hash: String = Sha256::digest(&data)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    let request = header(
        instance,
        auth,
        1,
        json!({"op":"append_blob_binary", "blob":blob,"offset":0,"length":4,"payload_sha256":hash}),
    );
    assert_eq!(
        p.receive(&mut rt, Some(auth), request.clone(), Instant::now())["error"]["code"],
        "INVALID_REQUEST"
    );
    let mut foreign = request.clone();
    foreign["client"] = json!(rt.register_client().unwrap());
    assert_eq!(
        p.receive_binary(&mut rt, auth, foreign, data.clone(), Instant::now())
            .value["error"]["code"],
        "WRONG_OWNER"
    );
    let mut binary_request = request;
    binary_request["request_id"] = json!(2);
    assert_eq!(
        p.receive_binary(&mut rt, auth, binary_request, data, Instant::now())
            .value["ok"],
        4
    );
}

#[test]
fn sealing_requires_complete_upload_and_reuses_the_reserved_buffer() {
    let mut rt = Runtime::new(RuntimeLimits {
        task_bytes: 4,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let blob = rt.create_blob(client, 4).unwrap();
    rt.append_blob(client, blob, 0, b"rg").unwrap();
    assert_eq!(
        rt.seal_blob(client, blob).unwrap_err().code,
        "INCOMPLETE_BLOB"
    );
    assert_eq!(rt.task_bytes(), 4);
    assert_eq!(
        rt.create_blob(client, 1).unwrap_err().code,
        "BUDGET_EXCEEDED"
    );
    rt.append_blob(client, blob, 2, b"ba").unwrap();
    rt.seal_blob(client, blob).unwrap();
    assert_eq!(rt.blob(client, blob).unwrap().bytes(), b"rgba");
    assert_eq!(rt.task_bytes(), 4);
    rt.release_blob(client, blob).unwrap();
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn invalid_binary_identity_cannot_mutate_the_upload_or_a_draft() {
    for attack in ["hash", "foreign", "runtime"] {
        let (mut rt, mut p, auth) = fixture();
        let instance = rt.instance_id();
        let session = rt
            .create_session(
                auth.client,
                &uuid::Uuid::new_v4().to_string(),
                Default::default(),
            )
            .unwrap();
        let before = rt.subscribe(auth.client, session).unwrap();
        let draft = rt.begin_draft(auth.client, before.cursor).unwrap();
        let blob = rt.create_blob(auth.client, 4).unwrap();
        let data = b"rgba";
        let hash: String = Sha256::digest(data)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect();
        let mut request = header(
            instance,
            auth,
            1,
            json!({"op":"append_blob_binary","blob":blob,"offset":0,"length":4,"payload_sha256":hash}),
        );
        let mut supplied = auth;
        match attack {
            "foreign" => {
                let hello = p.hello(&mut rt, json!({"protocol":CONTRACT}), Instant::now());
                supplied = Auth {
                    client: serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
                    connection: hello["ok"]["connection"].as_u64().unwrap(),
                };
                request["client"] = json!(supplied.client);
            }
            "runtime" => request["runtime"] = json!(Runtime::default().instance_id()),
            _ => request["operation"]["payload_sha256"] = json!("wrong"),
        }
        let reply = p.receive_binary(&mut rt, supplied, request, data.to_vec(), Instant::now());
        assert!(reply.value.get("error").is_some());
        assert_eq!(
            rt.seal_blob(auth.client, blob).unwrap_err().code,
            "INCOMPLETE_BLOB"
        );
        assert_eq!(
            rt.read_draft(auth.client, draft).unwrap().source,
            QuerySource::Draft(draft)
        );
        let after = rt.subscribe(auth.client, session).unwrap();
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.history, before.history);
    }
}

#[test]
fn output_arc_remains_charged_after_protocol_eviction_until_the_socket_releases_it() {
    let (mut rt, mut p, auth) = fixture();
    let instance = rt.instance_id();
    let response = p
        .receive_encoded(
            &mut rt,
            Some(auth),
            header(instance, auth, 1, json!({"op":"list_sessions"})),
            Instant::now(),
        )
        .unwrap();
    let retained = rt.task_bytes();
    assert!(retained >= response.bytes.len());
    p.expire(
        &mut rt,
        Instant::now() + std::time::Duration::from_secs(600),
    );
    assert!(rt.task_bytes() >= response.bytes.len());
    drop(response);
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn failed_output_admission_does_not_report_an_executed_write_as_a_known_refusal() {
    let mut rt = Runtime::new(RuntimeLimits {
        task_bytes: 4096,
        ..Default::default()
    });
    let mut p = Protocol::default();
    let hello = p.hello(&mut rt, json!({"protocol":CONTRACT}), Instant::now());
    let auth = Auth {
        client: serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
        connection: hello["ok"]["connection"].as_u64().unwrap(),
    };
    let request = header(
        rt.instance_id(),
        auth,
        1,
        json!({"op":"create_session","document_id":uuid::Uuid::new_v4().to_string()}),
    );
    assert!(p
        .receive(&mut rt, Some(auth), request.clone(), Instant::now())
        .get("ok")
        .is_some());
    let competing = rt
        .reserve_transfer(rt.limits().task_bytes - rt.task_bytes())
        .unwrap();
    assert!(p
        .receive_encoded(&mut rt, Some(auth), request.clone(), Instant::now())
        .is_none());
    drop(competing);
    assert_eq!(rt.list_sessions(auth.client).unwrap().len(), 1);
    assert!(p
        .receive(&mut rt, Some(auth), request, Instant::now())
        .get("ok")
        .is_some());
    assert_eq!(rt.list_sessions(auth.client).unwrap().len(), 1);
    drop(p);
    assert_eq!(rt.task_bytes(), 0);
}
