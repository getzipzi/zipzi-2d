use zipzi_core::{ModelSetup, Vec2};
use zipzi_runtime::authoring::EditOperation;
use zipzi_runtime::{CollectionDomain, CollectionQuery, Runtime, RuntimeError, RuntimeLimits};
use zipzi_sdk::{AuthoringSession, MeshCreateSpec, MeshGeometry, MeshOperation};

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
        },
    }
}
fn populated() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    sdk.edit("fixture", None, |edit| {
        for (n, name) in [(2, "Other"), (3, "Other"), (4, "Wanted")] {
            edit.create_mesh_spec(spec(n, name))?;
        }
        Ok(())
    })
    .unwrap();
    sdk
}
#[test]
fn metadata_scan_is_bounded_and_empty_pages_make_progress_without_cloning_geometry() {
    let mut limits = RuntimeLimits::default();
    limits.collection_scan = 1;
    limits.collection_records = 2;
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt.adopt_session(client, populated()).unwrap();
    let query = |cursor| CollectionQuery {
        session,
        edit: None,
        domain: CollectionDomain::Meshes,
        name: Some("Wanted".into()),
        limit: 2,
        cursor,
    };
    let first = rt.collection_page(client, query(None)).unwrap();
    assert!(first.items.is_empty());
    let c1 = first.next_cursor.clone().unwrap();
    let second = rt.collection_page(client, query(Some(c1.clone()))).unwrap();
    assert!(second.items.is_empty());
    assert_ne!(second.next_cursor.as_ref(), Some(&c1));
    let third = rt
        .collection_page(client, query(second.next_cursor))
        .unwrap();
    assert_eq!(third.source, first.source);
    assert_eq!(third.items.len(), 1);
    assert_eq!(third.items[0].id, id(4));
    assert_eq!(third.items[0].vertex_count, Some(3));
    assert!(third.next_cursor.is_none());
    let mut too_many = query(None);
    too_many.limit = 3;
    assert!(matches!(
        rt.collection_page(client, too_many),
        Err(RuntimeError::Budget("collection_records"))
    ));
    let mut wrong = query(Some(c1));
    wrong.domain = CollectionDomain::Parameters;
    assert!(matches!(
        rt.collection_page(client, wrong),
        Err(RuntimeError::InvalidReference)
    ));
}
#[test]
fn candidate_cursor_changes_are_read_failures_and_do_not_abort_the_candidate() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt.adopt_session(client, populated()).unwrap();
    let edit = rt.begin_edit(client, session, "candidate", None).unwrap();
    let query = |cursor| CollectionQuery {
        session,
        edit: Some(edit),
        domain: CollectionDomain::Meshes,
        name: None,
        limit: 1,
        cursor,
    };
    let page = rt.collection_page(client, query(None)).unwrap();
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Mesh {
            operation: MeshOperation::Rename {
                id: id(2),
                name: "Changed".into(),
                expected_name: None,
            },
        }],
    )
    .unwrap();
    let error = rt
        .collection_page(client, query(page.next_cursor))
        .unwrap_err();
    assert!(matches!(error,RuntimeError::Sdk(ref e) if e.code.as_ref()=="STALE_SOURCE"));
    rt.apply_authoring(
        client,
        edit,
        vec![EditOperation::Mesh {
            operation: MeshOperation::Rename {
                id: id(3),
                name: "Still editable".into(),
                expected_name: None,
            },
        }],
    )
    .unwrap();
    let current = rt.collection_page(client, query(None)).unwrap();
    assert_eq!(current.items[0].name, "Changed");
    assert_ne!(current.source, page.source);
}
#[test]
fn close_protects_other_members_and_reclaims_default_session_slots() {
    let mut rt = Runtime::default();
    let owner = rt.register_client().unwrap();
    let peer = rt.register_client().unwrap();
    for n in 100..140 {
        let session = rt
            .create_session(owner, &id(n), ModelSetup::default())
            .unwrap();
        assert!(matches!(
            rt.close_session(owner, session, false),
            Err(RuntimeError::Modified)
        ));
        rt.close_session(owner, session, true).unwrap();
    }
    assert!(rt.list_sessions().is_empty());
    let session = rt.adopt_session(owner, populated()).unwrap();
    rt.attach_session(peer, session).unwrap();
    let before = rt.session_info(session).unwrap();
    assert!(matches!(
        rt.close_session(owner, session, true),
        Err(RuntimeError::Busy)
    ));
    assert_eq!(rt.session_info(session).unwrap(), before);
    rt.detach_session(peer, session).unwrap();
    rt.close_session(owner, session, true).unwrap();
    assert!(rt.list_sessions().is_empty());
}
