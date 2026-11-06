use zipzi_contract::{
    authoring::ObjectKind,
    kernel_commands::{Command, ObjectAddress},
    kernel_wire::Query,
};
use zipzi_core::Vec2;
use zipzi_runtime::{ClientId, QuerySource, Runtime, SessionKey};
use zipzi_sdk::{MeshCreateSpec, MeshGeometry};

fn id(n: usize) -> String {
    format!("79800000-0000-4000-8000-{n:012x}")
}
fn spec(n: usize, name: &str) -> MeshCreateSpec {
    MeshCreateSpec {
        id: Some(id(n)),
        name: name.into(),
        texture_asset_id: String::new(),
        geometry: MeshGeometry {
            vertex_ids: vec![42, 7, 99],
            positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            uvs: vec![Vec2::new(0., 0.); 3],
            triangles: vec![[42, 7, 99]],
            meshing: None,
        },
    }
}
fn populated(rt: &mut Runtime, client: ClientId) -> SessionKey {
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let QuerySource::Published(version) = rt.read_snapshot(client, session).unwrap().source else {
        panic!("expected published source")
    };
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            [(2, "Other"), (3, "Other"), (4, "Wanted")]
                .into_iter()
                .map(|(n, name)| Command::CreateMesh {
                    spec: spec(n, name),
                })
                .collect(),
        )
        .unwrap();
    rt.publish(client, draft, "Fixture").unwrap();
    session
}

fn page(offset: usize, limit: usize) -> Query {
    Query::Collection {
        kind: ObjectKind::Mesh,
        offset,
        limit,
    }
}

#[test]
fn mesh_pages_keep_order_and_one_frozen_publication_source() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = populated(&mut rt, client);
    let source = rt.read_snapshot(client, session).unwrap().source;
    let first = rt.query(client, source, page(0, 2)).unwrap();
    assert_eq!(first["value"]["records"][0]["address"]["id"], id(2));
    assert_eq!(first["value"]["records"][1]["address"]["id"], id(3));
    let offset = first["value"]["next_offset"].as_u64().unwrap() as usize;
    assert_eq!(offset, 2);
    let second = rt.query(client, source, page(offset, 2)).unwrap();
    assert_eq!(second["source"], first["source"]);
    assert_eq!(second["content"], first["content"]);
    assert_eq!(second["value"]["records"][0]["address"]["id"], id(4));
    assert_eq!(
        second["value"]["records"][0]["value"]["vertex_ids"],
        serde_json::json!([42, 7, 99])
    );
    assert!(second["value"]["next_offset"].is_null());
    for limit in [0, 257] {
        assert_eq!(
            rt.query(client, source, page(0, limit)).unwrap_err().code,
            "RESOURCE_LIMIT"
        );
    }
}

#[test]
fn stale_draft_page_queries_do_not_abort_the_current_candidate() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = populated(&mut rt, client);
    let QuerySource::Published(version) = rt.read_snapshot(client, session).unwrap().source else {
        panic!("expected published source")
    };
    let initial = rt.begin_draft(client, version).unwrap();
    let first = rt
        .query(client, QuerySource::Draft(initial), page(0, 1))
        .unwrap();
    let rename = |n, name: &str| Command::Rename {
        target: ObjectAddress {
            kind: ObjectKind::Mesh,
            id: id(n),
        },
        name: name.into(),
    };
    let (draft, _) = rt
        .commands(client, initial, vec![rename(2, "Changed")])
        .unwrap();
    assert_eq!(
        rt.query(client, QuerySource::Draft(initial), page(1, 1))
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let (draft, _) = rt
        .commands(client, draft, vec![rename(3, "Still editable")])
        .unwrap();
    let current = rt
        .query(client, QuerySource::Draft(draft), page(0, 1))
        .unwrap();
    assert_eq!(current["value"]["records"][0]["value"]["name"], "Changed");
    assert_ne!(current["source"], first["source"]);
    rt.publish(client, draft, "Rename").unwrap();
}

#[test]
fn close_protects_other_members_and_reclaims_default_session_slots() {
    let mut rt = Runtime::default();
    let owner = rt.register_client().unwrap();
    let peer = rt.register_client().unwrap();
    for n in 100..140 {
        let session = rt
            .create_session(owner, &id(n), Default::default())
            .unwrap();
        assert_eq!(
            rt.close_session(owner, session, false).unwrap_err().code,
            "UNSAVED_CHANGES"
        );
        rt.close_session(owner, session, true).unwrap();
    }
    assert!(rt.list_sessions(owner).unwrap().is_empty());
    let session = populated(&mut rt, owner);
    rt.attach(peer, session).unwrap();
    let before = rt.read_snapshot(owner, session).unwrap().source;
    assert_eq!(
        rt.close_session(owner, session, true).unwrap_err().code,
        "SESSION_IN_USE"
    );
    assert_eq!(rt.read_snapshot(owner, session).unwrap().source, before);
    rt.detach(peer, session).unwrap();
    rt.close_session(owner, session, true).unwrap();
    assert!(rt.list_sessions(owner).unwrap().is_empty());
}
