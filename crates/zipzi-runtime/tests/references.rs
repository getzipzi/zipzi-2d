use zipzi_core::ModelSetup;
use zipzi_runtime::{Runtime, RuntimeError, RuntimeLimits};
use zipzi_sdk::{MeshCreateSpec, MeshGeometry, ObjectKind, PartCreateSpec, SceneOperation};
fn id(n: u32) -> String {
    format!("79900000-0000-4000-8000-{n:012x}")
}
fn setup(rt: &mut Runtime) -> (zipzi_runtime::ClientId, zipzi_runtime::SessionId) {
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "create", None).unwrap();
    rt.apply_edit(client, edit, |e| {
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
        SceneOperation::CreatePart {
            spec: PartCreateSpec {
                name: "Folder".into(),
                id: Some(id(3)),
                parent_id: None,
                before: None,
            },
        }
        .apply(e)?;
        Ok(())
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    (client, session)
}
#[test]
fn sdk_handles_survive_moves_but_not_delete_undo_and_address_epochs_do_not_revive() {
    let mut rt = Runtime::default();
    let (client, session) = setup(&mut rt);
    let reference = rt
        .capture_object_ref(client, session, ObjectKind::Mesh, &id(2))
        .unwrap()
        .reference;
    let address = rt.capture_label_address(client, reference).unwrap().address;
    let before = rt.session_info(session).unwrap().version;
    let plan = rt.plan_label_patch(client, address, "B".into()).unwrap();
    assert_eq!(rt.session_info(session).unwrap().version, before);
    rt.release_modeling_plan(client, plan.plan).unwrap();
    let edit = rt.begin_edit(client, session, "move", None).unwrap();
    rt.apply_edit(client, edit, |e| {
        SceneOperation::MoveToPart {
            ids: vec![id(2)],
            part_id: Some(id(3)),
            before: None,
        }
        .apply(e)
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    rt.resolve_object_ref(client, reference).unwrap();
    assert!(
        matches!(rt.plan_label_patch(client, address, "C".into()), Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "STALE_FIELD_ADDRESS")
    );
    rt.undo(client, session).unwrap();
    assert!(
        matches!(rt.plan_label_patch(client, address, "C".into()), Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "STALE_FIELD_ADDRESS")
    );
    let edit = rt.begin_edit(client, session, "delete", None).unwrap();
    rt.apply_edit(client, edit, |e| {
        SceneOperation::DeleteScene {
            ids: vec![id(2)],
            subtree: false,
        }
        .apply(e)
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    rt.undo(client, session).unwrap();
    assert!(
        matches!(rt.resolve_object_ref(client, reference), Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "STALE_HANDLE")
    );
    rt.capture_object_ref(client, session, ObjectKind::Mesh, &id(2))
        .unwrap();
}
#[test]
fn references_are_owner_scoped_admitted_and_released_without_document_or_history_changes() {
    let mut rt = Runtime::new(RuntimeLimits {
        references_per_client: 2,
        ..Default::default()
    });
    let (client, session) = setup(&mut rt);
    let other = rt.register_client().unwrap();
    let baseline = rt.accounted_bytes();
    let version = rt.session_info(session).unwrap().version;
    let reference = rt
        .capture_object_ref(client, session, ObjectKind::Mesh, &id(2))
        .unwrap()
        .reference;
    assert_eq!(
        rt.resolve_object_ref(other, reference),
        Err(RuntimeError::WrongOwner)
    );
    assert_eq!(
        rt.release_object_ref(other, reference),
        Err(RuntimeError::WrongOwner)
    );
    let address = rt.capture_label_address(client, reference).unwrap().address;
    assert!(matches!(
        rt.capture_object_ref(client, session, ObjectKind::Mesh, &id(2)),
        Err(RuntimeError::Budget(_))
    ));
    assert!(rt.accounted_bytes() > baseline);
    rt.release_object_ref(client, reference).unwrap();
    let plan = rt.plan_label_patch(client, address, "B".into()).unwrap();
    rt.release_modeling_plan(client, plan.plan).unwrap();
    rt.release_field_address(client, address).unwrap();
    rt.release_field_address(client, address).unwrap();
    assert_eq!(rt.reference_counts(), (0, 0));
    assert_eq!(rt.accounted_bytes(), baseline);
    assert_eq!(rt.session_info(session).unwrap().version, version);
    assert_eq!(rt.session(session).unwrap().history_lengths(), (1, 0));
    for _ in 0..100 {
        let reference = rt
            .capture_object_ref(client, session, ObjectKind::Mesh, &id(2))
            .unwrap()
            .reference;
        let address = rt.capture_label_address(client, reference).unwrap().address;
        rt.release_object_ref(client, reference).unwrap();
        rt.release_field_address(client, address).unwrap();
        assert_eq!(rt.reference_counts(), (0, 0));
        assert_eq!(rt.accounted_bytes(), baseline);
    }
    rt.capture_object_ref(client, session, ObjectKind::Mesh, &id(2))
        .unwrap();
    rt.release_client(client);
    assert_eq!(rt.reference_counts(), (0, 0));
    assert!(matches!(
        rt.resolve_object_ref(client, reference),
        Err(RuntimeError::InvalidReference)
    ));
}
