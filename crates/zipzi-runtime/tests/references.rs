use zipzi_contract::{
    authoring::ObjectKind,
    kernel_commands::{Command, ObjectAddress},
    kernel_wire::Query,
};
use zipzi_core::document::{OrganizationCollection, OrganizationEdit, SceneEdit, SceneFieldValue};
use zipzi_runtime::*;
use zipzi_sdk::{MeshCreateSpec, MeshGeometry};

fn id(n: u32) -> String {
    format!("79900000-0000-4000-8000-{n:012x}")
}
fn setup(rt: &mut Runtime) -> (ClientId, Version) {
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .batch(client, draft, |e| {
            e.create_mesh_spec(MeshCreateSpec {
                name: "A".into(),
                id: Some(id(2)),
                texture_asset_id: String::new(),
                geometry: MeshGeometry {
                    vertex_ids: vec![1, 2, 3],
                    positions: vec![
                        zipzi_core::Vec2::new(0., 0.),
                        zipzi_core::Vec2::new(1., 0.),
                        zipzi_core::Vec2::new(0., 1.),
                    ],
                    uvs: vec![
                        zipzi_core::Vec2::new(0., 0.),
                        zipzi_core::Vec2::new(1., 0.),
                        zipzi_core::Vec2::new(0., 1.),
                    ],
                    triangles: vec![[1, 2, 3]],
                    meshing: None,
                },
            })?;
            e.create_part(zipzi_core::Part {
                id: id(3),
                runtime_id: id(3),
                name: "Folder".into(),
                ..Default::default()
            })
        })
        .unwrap();
    let publication = rt.publish(client, draft, "Create").unwrap();
    (client, publication.version)
}

fn mesh_query() -> Query {
    Query::Objects {
        addresses: vec![ObjectAddress {
            kind: ObjectKind::Mesh,
            id: id(2),
        }],
    }
}
fn rename(expected: &str, value: &str) -> Command {
    Command::Scene {
        edit: SceneEdit::Patch {
            id: id(2),
            expected: SceneFieldValue::Label(expected.into()),
            value: SceneFieldValue::Label(value.into()),
            clear_glue_pairs: false,
        },
    }
}

#[test]
fn stable_identity_survives_moves_and_undo_without_reviving_query_sources() {
    let mut rt = Runtime::default();
    let (client, original) = setup(&mut rt);
    let original_source = QuerySource::Published(original);
    let query = rt.query(client, original_source, mesh_query()).unwrap();
    let draft = rt.begin_draft(client, original).unwrap();
    let (draft, _) = rt
        .batch(client, draft, |c| {
            c.edit_organization(OrganizationEdit::Move {
                ids: vec![id(2)],
                destination: OrganizationCollection::PartMembers {
                    part_id: Some(id(3)),
                },
                before: None,
            })
        })
        .unwrap();
    let moved = rt.publish(client, draft, "Move").unwrap();
    let selected = rt
        .query(client, QuerySource::Published(moved.version), mesh_query())
        .unwrap();
    assert_eq!(
        selected["value"]["records"][0]["address"],
        query["value"]["records"][0]["address"]
    );
    assert_eq!(selected["value"]["records"][0]["value"]["part_id"], id(3));
    let restored = rt.undo(client, moved.version).unwrap();
    assert_eq!(
        rt.query(client, original_source, mesh_query())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let draft = rt.begin_draft(client, restored.version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![Command::Scene {
                edit: SceneEdit::Delete {
                    ids: vec![id(2)],
                    scope: Default::default(),
                },
            }],
        )
        .unwrap();
    let deleted = rt.publish(client, draft, "Delete").unwrap();
    assert_eq!(
        rt.query(
            client,
            QuerySource::Published(deleted.version),
            mesh_query()
        )
        .unwrap_err()
        .code,
        "MISSING_OBJECT"
    );
    let restored = rt.undo(client, deleted.version).unwrap();
    let selected = rt
        .query(
            client,
            QuerySource::Published(restored.version),
            mesh_query(),
        )
        .unwrap();
    assert_eq!(
        selected["value"]["records"][0]["address"],
        query["value"]["records"][0]["address"]
    );
    assert_eq!(
        rt.begin_draft(client, original).unwrap_err().code,
        "STALE_SOURCE"
    );
    let draft = rt.begin_draft(client, restored.version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![rename("A", "B")]).unwrap();
    assert_eq!(
        rt.read_draft(client, draft)
            .unwrap()
            .snapshot
            .document()
            .mesh(&id(2))
            .unwrap()
            .name,
        "B"
    );
}

#[test]
fn reads_require_attachment_and_draft_ownership_without_allocating_reference_handles() {
    let mut rt = Runtime::default();
    let (client, version) = setup(&mut rt);
    let other = rt.register_client().unwrap();
    let source = QuerySource::Published(version);
    assert!(rt.query(other, source, mesh_query()).is_err());
    rt.attach(other, version.session).unwrap();
    let draft = rt.begin_draft(client, version).unwrap();
    assert_eq!(
        rt.query(other, QuerySource::Draft(draft), mesh_query())
            .unwrap_err()
            .code,
        "NOT_OWNER"
    );
    assert_eq!(
        rt.cancel_draft(other, draft.draft).unwrap_err().code,
        "NOT_OWNER"
    );
    let baseline = rt.task_bytes();
    for _ in 0..100 {
        let a = rt.query(client, source, mesh_query()).unwrap();
        let b = rt.query(other, source, mesh_query()).unwrap();
        assert_eq!(a, b);
        assert_eq!(rt.task_bytes(), baseline);
    }
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        1
    );
    rt.release_client(client);
    assert!(rt.query(client, source, mesh_query()).is_err());
    assert_eq!(
        rt.query(other, QuerySource::Draft(draft), mesh_query())
            .unwrap_err()
            .code,
        "DRAFT_ENDED"
    );
    assert_eq!(
        rt.subscribe(other, version.session).unwrap().cursor,
        version
    );
}
