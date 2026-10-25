#![cfg(unix)]
use serde_json::{Value, json};
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
fn header(auth: Auth, id: u64, operation: Value) -> Value {
    json!({"protocol": CONTRACT, "runtime":auth.client.runtime_instance(),"client":auth.client,"request_id":id,"operation":operation})
}
#[test]
fn binary_upload_replay_binds_payload_and_frozen_download_survives_blob_release() {
    let (mut rt, mut p, auth) = fixture();
    let blob = rt.create_blob(auth.client, 8).unwrap();
    let data = b"pixels\0\xff".to_vec();
    let hash: String = Sha256::digest(&data)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    let input = header(
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
        auth,
        2,
        json!({"op":"read_blob_binary","blob":blob,"offset":1,"length":5}),
    );
    let output = p.receive_binary(&mut rt, auth, read.clone(), vec![], Instant::now());
    assert_eq!(output.payload.as_ref().unwrap().bytes(), b"ixels");
    rt.release_blob(auth.client, blob).unwrap();
    assert_eq!(rt.blob_count(), 0);
    assert_eq!(
        p.receive_binary(&mut rt, auth, read, vec![], Instant::now())
            .payload
            .unwrap()
            .bytes(),
        b"ixels"
    );
    assert_eq!(rt.image_memory_usage().blob_bytes, 8);
    drop(output);
    drop(p);
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}
#[test]
fn binary_controls_cannot_be_smuggled_through_json_or_another_owner() {
    let (mut rt, mut p, auth) = fixture();
    let blob = rt.create_blob(auth.client, 4).unwrap();
    let data = b"rgba".to_vec();
    let hash: String = Sha256::digest(&data)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    let request = header(
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
    assert_eq!(
        p.receive_binary(&mut rt, auth, request, data, Instant::now())
            .value["ok"],
        4
    );
}

#[test]
fn seal_peak_refusal_keeps_the_upload_and_releases_temporary_reservations() {
    let mut rt = Runtime::new(RuntimeLimits {
        image_bytes: 8,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let blob = rt.create_blob(client, 4).unwrap();
    rt.append_blob(client, blob, 0, b"rgba").unwrap();
    let filler = rt.create_blob(client, 1).unwrap();
    assert!(rt.seal_blob(client, blob).is_err());
    assert_eq!(rt.image_memory_usage().preparing_bytes, 5);
    assert!(matches!(rt.blob(client, blob), Err(RuntimeError::Busy)));
    rt.release_blob(client, filler).unwrap();
    rt.seal_blob(client, blob).unwrap();
    assert_eq!(&*rt.blob(client, blob).unwrap(), b"rgba");
    assert_eq!(rt.image_memory_usage().total_bytes(), 4);
    rt.release_blob(client, blob).unwrap();
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}

#[test]
fn attached_hash_failure_aborts_only_the_authenticated_owner_without_client_cleanup() {
    for attack in ["owner", "foreign", "header"] {
        let (mut rt, mut p, auth) = fixture();
        let session = rt
            .create_session(
                auth.client,
                &uuid::Uuid::new_v4().to_string(),
                zipzi_core::ModelSetup::default(),
            )
            .unwrap();
        let edit = rt.begin_edit(auth.client, session, "upload", None).unwrap();
        let before = rt.session_info(session).unwrap();
        let blob = rt.start_authoring_upload(auth.client, edit, 4).unwrap();
        let request = header(
            auth,
            1,
            json!({"op":"append_blob_binary","blob":blob,"offset":0,"length":4,"payload_sha256":"wrong"}),
        );
        let mut supplied = auth;
        let mut request = request;
        if attack == "foreign" {
            let hello = p.hello(&mut rt, json!({"protocol":CONTRACT}), Instant::now());
            supplied = Auth {
                client: serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
                connection: hello["ok"]["connection"].as_u64().unwrap(),
            };
            request["client"] = json!(supplied.client);
        }
        if attack == "header" {
            request["runtime"] = json!(Runtime::default().instance_id());
        }
        let value = p.receive_binary(&mut rt, supplied, request, b"rgba".to_vec(), Instant::now());
        assert!(value.value.get("error").is_some());
        if attack == "owner" {
            assert_eq!(
                rt.edit_state(auth.client, edit).unwrap(),
                EditState::Aborted
            );
            assert_eq!(rt.blob_count(), 0);
            assert_eq!(rt.image_memory_usage().total_bytes(), 0);
        } else {
            assert_eq!(
                rt.edit_state(auth.client, edit).unwrap(),
                EditState::Editing
            );
            assert_eq!(rt.blob_count(), 1);
        }
        assert_eq!(rt.session_info(session).unwrap().version, before.version);
        assert_eq!(rt.session_info(session).unwrap().history_lengths, (0, 0));
    }
}

#[test]
fn output_arc_remains_charged_after_protocol_eviction_until_the_socket_releases_it() {
    let (mut rt, mut p, auth) = fixture();
    let response = p
        .receive_encoded(
            &mut rt,
            Some(auth),
            header(auth, 1, json!({"op":"list_sessions"})),
            Instant::now(),
        )
        .unwrap();
    assert_eq!(
        rt.image_memory_usage().transfer_bytes,
        response.bytes.len() * 2
    );
    p.expire(
        &mut rt,
        Instant::now() + std::time::Duration::from_secs(600),
    );
    assert_eq!(
        rt.image_memory_usage().transfer_bytes,
        response.bytes.len() * 2
    );
    drop(response);
    assert_eq!(rt.image_memory_usage().transfer_bytes, 0);
}

#[test]
fn failed_output_admission_does_not_report_an_executed_write_as_a_known_refusal() {
    let mut rt = Runtime::new(RuntimeLimits {
        image_bytes: 8,
        ..Default::default()
    });
    let mut p = Protocol::default();
    let hello = p.hello(&mut rt, json!({"protocol":CONTRACT}), Instant::now());
    let auth = Auth {
        client: serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
        connection: hello["ok"]["connection"].as_u64().unwrap(),
    };
    let request = header(
        auth,
        1,
        json!({"op":"create_session","document_id":uuid::Uuid::new_v4().to_string(),"setup":zipzi_core::ModelSetup::default()}),
    );
    assert!(
        p.receive(&mut rt, Some(auth), request.clone(), Instant::now())
            .get("ok")
            .is_some()
    );
    rt.refresh_memory_budget();
    let competing = rt
        .reserve_transfer(RuntimeLimits::default().accounted_bytes - rt.accounted_bytes())
        .unwrap();
    assert!(
        p.receive_encoded(&mut rt, Some(auth), request.clone(), Instant::now())
            .is_none()
    );
    drop(competing);
    assert_eq!(rt.list_sessions().len(), 1);
    assert!(
        p.receive(&mut rt, Some(auth), request, Instant::now())
            .get("ok")
            .is_some()
    );
    assert_eq!(rt.list_sessions().len(), 1);
    assert_eq!(rt.image_memory_usage().total_bytes(), 0);
}
