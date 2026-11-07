use zipzi_contract::kernel_commands::Command;
use zipzi_core::Vec2;
use zipzi_runtime::*;
use zipzi_sdk::{ManualMeshOperation, MeshCreateSpec, MeshGeometry};

fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, Version, DraftSource) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    (rt, client, version, draft)
}

#[test]
fn largest_grid_roundtrips_as_a_typed_command_and_publishes_once() {
    let (mut rt, client, version, draft) = fixture(Default::default());
    let id = uuid::Uuid::new_v4().to_string();
    let points = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(1., 1.),
        Vec2::new(0., 1.),
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
    let commands = vec![Command::CreateMesh {
        spec: MeshCreateSpec {
            id: Some(id.clone()),
            name: "Large grid".into(),
            texture_asset_id: String::new(),
            geometry,
        },
    }];
    let bytes = serde_json::to_vec(&commands).unwrap();
    assert!(bytes.len() < zipzi_runtime::protocol::MAX_FRAME);
    let (draft, _) = rt
        .commands(client, draft, serde_json::from_slice(&bytes).unwrap())
        .unwrap();
    assert_eq!(draft.sequence, 1);
    let candidate = rt.read_draft(client, draft).unwrap();
    assert_eq!(
        candidate
            .snapshot
            .document()
            .mesh(&id)
            .unwrap()
            .vertex_ids
            .len(),
        65536
    );
    assert!(rt
        .read_snapshot(client, version.session)
        .unwrap()
        .snapshot
        .document()
        .mesh(&id)
        .is_none());
    assert_eq!(
        rt.publish(client, draft, "Large grid")
            .unwrap()
            .history
            .undo_steps,
        1
    );
    let after = rt.read_snapshot(client, version.session).unwrap();
    assert_eq!(
        after.snapshot.document().mesh(&id).unwrap().triangles.len(),
        2 * 255 * 255
    );
}

#[test]
fn unsealed_or_released_blob_preserves_draft_but_bad_image_aborts_accepted_batch() {
    let (mut rt, client, version, draft) = fixture(Default::default());
    let id = uuid::Uuid::new_v4().to_string();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![Command::CreateAsset {
                id: id.clone(),
                name: "Image".into(),
            }],
        )
        .unwrap();
    let blob = rt.create_blob(client, 4).unwrap();
    assert_eq!(
        rt.replace_image_blob(client, draft, id.clone(), blob, 1, 1)
            .unwrap_err()
            .code,
        "INCOMPLETE_BLOB"
    );
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    rt.release_blob(client, blob).unwrap();
    assert_eq!(
        rt.replace_image_blob(client, draft, id.clone(), blob, 1, 1)
            .unwrap_err()
            .code,
        "BLOB_NOT_FOUND"
    );
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    let blob = rt.create_blob(client, 4).unwrap();
    rt.append_blob(client, blob, 0, &[255, 0, 0, 255]).unwrap();
    rt.seal_blob(client, blob).unwrap();
    assert_eq!(
        rt.replace_image_blob(client, draft, id.clone(), blob, 2, 1)
            .unwrap_err()
            .code,
        "INVALID_IMAGE"
    );
    assert_eq!(
        rt.read_draft(client, draft).unwrap_err().code,
        "DRAFT_ENDED"
    );
    rt.release_blob(client, blob).unwrap();
    assert_eq!(rt.task_bytes(), 0);
    let formal = rt.subscribe(client, version.session).unwrap();
    assert_eq!(formal.cursor, version);
    assert_eq!(formal.history.undo_steps, 0);
    assert!(formal.snapshot.snapshot.document().asset(&id).is_none());
}

#[test]
fn upload_admission_refusal_preserves_the_candidate_and_sequence() {
    let (mut rt, client, version, draft) = fixture(RuntimeLimits {
        task_bytes: 8,
        ..Default::default()
    });
    assert_eq!(
        rt.create_blob(client, 9).unwrap_err().code,
        "BUDGET_EXCEEDED"
    );
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    assert_eq!(draft.sequence, 0);
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        0
    );
}

#[path = "../../../fixtures/mesh-edit-dependencies.rs"]
mod manual_dependencies;

#[test]
fn manual_mesh_command_preview_rejection_preserves_candidate_and_sequence() {
    let (mut rt, client, _, draft) = fixture(Default::default());
    let (draft, _) = rt
        .batch(client, draft, manual_dependencies::install)
        .unwrap();
    let version = rt.publish(client, draft, "Fixture").unwrap().version;
    let draft = rt.begin_draft(client, version).unwrap();
    let commands = |uv| {
        vec![Command::ManualMesh {
            mesh: manual_dependencies::id(2),
            operation: ManualMeshOperation::InsertVertex { id: 200, uv },
        }]
    };
    assert!(rt
        .preview_commands(client, draft, commands(Vec2::new(-0.05, 0.3)))
        .is_err());
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    let (updated, _) = rt
        .preview_commands(client, draft, commands(Vec2::new(0.35, 0.25)))
        .unwrap();
    assert_eq!(updated.sequence, draft.sequence + 1);
    let candidate = rt.read_draft(client, updated).unwrap();
    let document = candidate.snapshot.document();
    assert_eq!(
        document
            .mesh(&manual_dependencies::id(2))
            .unwrap()
            .vertex_ids
            .len(),
        6
    );
    let binding = document.binding(&manual_dependencies::id(6)).unwrap();
    assert!(binding
        .keyforms
        .iter()
        .all(|shape| shape.positions.len() == 6));
    assert_eq!(
        rt.publish(client, updated, "Insert vertex")
            .unwrap()
            .history
            .undo_steps,
        2
    );
}
