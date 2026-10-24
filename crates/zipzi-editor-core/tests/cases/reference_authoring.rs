use zipzi_editor_core::{Document, ObjectKind, ObjectRef};
use zipzi_sdk::{
    ReferenceCollection as C, ReferenceEdit as E, ReferenceFields as F,
    ReferenceSelectionField as S,
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn run(d: &mut Document, e: E) -> anyhow::Result<zipzi_editor_core::OperationResult> {
    let p = d.plan_reference_authoring(e)?;
    d.execute_reference_authoring(&p)
}
#[test]
fn pending_business_objects_are_independently_inspectable_and_persistent() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let g = id();
    let m = id();
    let h = id();
    let s = id();
    let c = id();
    run(
        &mut d,
        E::CreateModelParameterGroup {
            id: g.clone(),
            name: String::new(),
            before: None,
        },
    )?;
    run(
        &mut d,
        E::CreateMember {
            id: m.clone(),
            collection: C::ModelParameterMembers {
                group_id: g.clone(),
            },
            before: None,
        },
    )?;
    run(
        &mut d,
        E::CreateModelHitArea {
            id: h.clone(),
            name: String::new(),
            before: None,
        },
    )?;
    run(
        &mut d,
        E::CreateCdiCombinedSet {
            id: s.clone(),
            before: None,
        },
    )?;
    run(
        &mut d,
        E::CreateMember {
            id: c.clone(),
            collection: C::CdiCombinedMembers { set_id: s.clone() },
            before: None,
        },
    )?;
    let objects = [
        (ObjectKind::ModelParameterGroup, &g),
        (ObjectKind::ModelParameterMember, &m),
        (ObjectKind::ModelHitArea, &h),
        (ObjectKind::CdiCombinedSet, &s),
        (ObjectKind::CdiCombinedMember, &c),
    ];
    for (kind, id) in &objects {
        assert!(
            d.object_index()
                .get(&ObjectRef::new(*kind, *id))
                .unwrap()
                .diagnostic
                .is_some()
        );
    }
    assert_eq!(
        d.reference_collection(&C::ModelParameterMembers {
            group_id: g.clone()
        })?,
        [ObjectRef::new(ObjectKind::ModelParameterMember, &m)]
    );
    let root = tempfile::tempdir()?;
    let saved = d.save(&root.path().join("references.zipzi"))?;
    let reopened = Document::open(&saved.manifest)?;
    for (kind, id) in objects {
        assert!(
            reopened
                .object_index()
                .get(&ObjectRef::new(kind, id))
                .is_some()
        );
    }
    Ok(())
}
#[test]
fn label_publication_reuses_relations_and_refreshes_document_metadata() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let g = id();
    run(
        &mut d,
        E::CreateModelParameterGroup {
            id: g.clone(),
            name: "EyeBlink".into(),
            before: None,
        },
    )?;
    let reference = ObjectRef::new(ObjectKind::ModelParameterGroup, &g);
    let before = d.object_index();
    let frame = d.snapshot()?;
    let address = d.reference_field_address(&g)?;
    let p = d.plan_reference_field_patch(
        &address,
        F::ModelParameterGroup {
            name: "LipSync".into(),
        },
    )?;
    d.execute_reference_authoring(&p)?;
    let after = d.object_index();
    assert_eq!(after.name(&reference), "LipSync");
    assert_eq!(before.name(&reference), "EyeBlink");
    assert!(std::ptr::eq(&before.relations[0], &after.relations[0]));
    let doc = after
        .objects
        .iter()
        .find(|o| o.reference.kind == ObjectKind::Document)
        .unwrap();
    assert_eq!(
        doc.properties["model_settings"]["groups"][0]["name"],
        "LipSync"
    );
    assert_eq!(
        frame.runtime_frame.source_revision,
        d.snapshot()?.runtime_frame.source_revision
    );
    d.undo()?;
    assert!(
        d.plan_reference_field_patch(&address, F::ModelParameterGroup { name: "New".into() })
            .is_err()
    );
    Ok(())
}
#[test]
fn reference_copy_delete_and_selection_patch_refresh_the_shared_projection() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let mesh = d.mesh_tree()[0].children[0].id.clone();
    let other = d.mesh_tree()[0].children[1].id.clone();
    let address = d.reference_selection_address(&mesh, S::MeshMasks)?;
    let p = d.plan_reference_selection_patch(&address, vec![other])?;
    d.execute_reference_authoring(&p)?;
    assert_eq!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Mesh, &mesh))
            .unwrap()
            .properties["masks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    d.undo()?;
    assert!(d.plan_reference_selection_patch(&address, vec![]).is_err());
    let g = id();
    let member = id();
    run(
        &mut d,
        E::CreateModelParameterGroup {
            id: g.clone(),
            name: "EyeBlink".into(),
            before: None,
        },
    )?;
    run(
        &mut d,
        E::CreateMember {
            id: member.clone(),
            collection: C::ModelParameterMembers {
                group_id: g.clone(),
            },
            before: None,
        },
    )?;
    let p = d.plan_reference_authoring(E::Duplicate {
        ids: vec![g.clone()],
        destination: C::ModelParameterGroups,
        before: None,
    })?;
    let child = p.copied_ids()[&member].clone();
    let copied = p.copied_ids()[&g].clone();
    let result = d.execute_reference_authoring(&p)?;
    assert!(
        result
            .suggested_selection
            .contains(&ObjectRef::new(ObjectKind::ModelParameterMember, &child))
    );
    run(&mut d, E::Delete { ids: vec![copied] })?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::ModelParameterMember, &child))
            .is_none()
    );
    d.undo()?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::ModelParameterMember, &child))
            .is_some()
    );
    Ok(())
}
