use zipzi_core::binary_wire::{encode, WireWrite};
use zipzi_runtime::{
    authoring::{EditOperation, WriteResult},
    *,
};
use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation};
fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, SessionId, EditId) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            zipzi_core::Canvas::default(),
        )
        .unwrap();
    let edit = rt.begin_edit(client, session, "binary", None).unwrap();
    (rt, client, session, edit)
}
fn payload(operations: &Vec<EditOperation>) -> Vec<u8> {
    let mut value = AUTHORING_MAGIC.to_vec();
    Vec::<EditOperation>::SIGNATURE.encode(&mut value).unwrap();
    value.extend(encode(operations).unwrap());
    value
}
fn upload(rt: &mut Runtime, c: ClientId, e: EditId, bytes: &[u8]) -> BlobId {
    let blob = rt.start_authoring_upload(c, e, bytes.len()).unwrap();
    for (index, chunk) in bytes.chunks(1 << 20).enumerate() {
        rt.append_blob(c, blob, index * (1 << 20), chunk).unwrap();
    }
    rt.seal_blob(c, blob).unwrap();
    blob
}
#[test]
fn sdk_limit_grid_uses_typed_binary_and_pending_upload_blocks_other_writes() {
    let (mut rt, c, s, e) = fixture(Default::default());
    let id = uuid::Uuid::new_v4().to_string();
    let points = vec![
        zipzi_core::Vec2::new(0., 0.),
        zipzi_core::Vec2::new(1., 0.),
        zipzi_core::Vec2::new(1., 1.),
        zipzi_core::Vec2::new(0., 1.),
    ];
    let base = MeshGeometry {
        vertex_ids: vec![0, 1, 2, 3],
        positions: points.clone(),
        uvs: points,
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        meshing: None,
    };
    let geometry = zipzi_sdk::rectangle_grid_geometry(&base, 255, 255).unwrap();
    assert_eq!(geometry.vertex_ids.len(), 65536);
    assert!(zipzi_sdk::rectangle_grid_geometry(&base, 256, 255).is_err());
    let operations = vec![EditOperation::Mesh {
        operation: MeshOperation::Create {
            spec: MeshCreateSpec {
                texture_asset_id: String::new(),
                id: Some(id.clone()),
                name: "large".into(),
                geometry,
            },
        },
    }];
    let bytes = payload(&operations);
    assert!(bytes.len() > 1 << 20);
    let blob = rt.start_authoring_upload(c, e, bytes.len()).unwrap();
    assert!(matches!(
        rt.apply_authoring(c, e, vec![]),
        Err(RuntimeError::Busy)
    ));
    assert!(matches!(rt.request_commit(c, e), Err(RuntimeError::Busy)));
    assert_eq!(rt.edit_sequence(c, e).unwrap(), 0);
    for (index, chunk) in bytes.chunks(1 << 20).enumerate() {
        rt.append_blob(c, blob, index * (1 << 20), chunk).unwrap();
    }
    rt.seal_blob(c, blob).unwrap();
    let WriteResult { source, .. } = rt.apply_authoring_blob(c, e, blob).unwrap();
    assert!(matches!(source, SourceRef::Edit { sequence: 1, .. }));
    assert_eq!(
        rt.edit_document(c, e)
            .unwrap()
            .get_mesh(&id)
            .unwrap()
            .vertex_ids
            .len(),
        65536
    );
    rt.release_blob(c, blob).unwrap();
    assert_eq!(rt.image_memory_usage().blob_bytes, 0);
    rt.commit_edit(c, e).unwrap();
    assert_eq!(
        rt.document(s)
            .unwrap()
            .get_mesh(&id)
            .unwrap()
            .triangles
            .len(),
        2 * 255 * 255
    );
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (1, 0));
}
#[test]
fn malformed_or_abandoned_attached_work_fails_on_the_host_without_client_abort() {
    for failure in [
        "append", "seal", "contract", "trailing", "variant", "release",
    ] {
        let (mut rt, c, s, e) = fixture(Default::default());
        let before = rt.session_info(s).unwrap();
        let mut bytes = payload(&vec![]);
        if failure == "contract" {
            bytes[4] ^= 1;
        }
        if failure == "trailing" {
            bytes.push(9);
        }
        if failure == "variant" {
            bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&999u32.to_le_bytes());
        }
        let blob = if matches!(failure, "append" | "seal" | "release") {
            rt.start_authoring_upload(c, e, bytes.len()).unwrap()
        } else {
            upload(&mut rt, c, e, &bytes)
        };
        match failure {
            "append" => assert!(rt.append_blob(c, blob, 1, b"x").is_err()),
            "seal" => assert!(rt.seal_blob(c, blob).is_err()),
            "release" => rt.release_blob(c, blob).unwrap(),
            _ => assert!(rt.apply_authoring_blob(c, e, blob).is_err()),
        }
        assert_eq!(
            rt.edit_state(c, e).unwrap(),
            EditState::Aborted,
            "{failure}"
        );
        assert_eq!(rt.blob_count(), 0, "{failure}");
        assert_eq!(rt.image_memory_usage().total_bytes(), 0, "{failure}");
        assert_eq!(rt.session_info(s).unwrap().version, before.version);
        assert_eq!(rt.session_info(s).unwrap().history_lengths, (0, 0));
        assert!(matches!(
            rt.request_commit(c, e),
            Err(RuntimeError::Aborted)
        ));
    }
}
#[test]
fn upload_admission_refusal_preserves_the_candidate_and_sequence() {
    let (mut rt, c, s, e) = fixture(RuntimeLimits {
        blob_bytes: 8,
        ..Default::default()
    });
    assert!(rt.start_authoring_upload(c, e, 9).is_err());
    assert_eq!(rt.edit_state(c, e).unwrap(), EditState::Editing);
    assert_eq!(rt.edit_sequence(c, e).unwrap(), 0);
    assert_eq!(rt.blob_count(), 0);
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (0, 0));
}
