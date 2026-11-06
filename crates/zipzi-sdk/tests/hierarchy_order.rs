use crate::support::archive;
use zipzi_core::document::{OrganizationCollection, OrganizationDeletePolicy, OrganizationEdit};
use zipzi_core::{document::HierarchyOrder, Canvas, Part, Vec2};
use zipzi_sdk::AuthoringSession;

#[test]
fn palette_metadata_keeps_evaluation_revision_and_survives_checkpoint_and_erasure(
) -> Result<(), Box<dyn std::error::Error>> {
    let first = "00000000-0000-4000-8000-000000000001".to_owned();
    let second = "00000000-0000-4000-8000-000000000002".to_owned();
    let mut session = AuthoringSession::new(zipzi_sdk::Snapshot::new(
        zipzi_core::state::DocumentState::new(
            "00000000-0000-4000-8000-000000000003",
            Canvas::new(100., 100., Vec2::default(), 1.).into(),
        )?,
        Default::default(),
    )?);
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                for id in [&first, &second] {
                    edit.create_part({
                        let mut value = Part {
                            id: id.clone(),
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Parts")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }?;
    let evaluation = session.snapshot().content_id();
    let order = HierarchyOrder {
        organization: vec![second.clone(), first.clone()],
        ..Default::default()
    };
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.edit_organization(OrganizationEdit::Move {
                    ids: vec![second.clone()],
                    destination: OrganizationCollection::PartMembers { part_id: None },
                    before: Some(first.clone()),
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Sort")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }?;
    assert_ne!(session.snapshot().content_id(), evaluation);
    session.undo(session.version())?;
    assert!(session.snapshot().document().hierarchy_order().is_empty());
    assert_eq!(session.snapshot().content_id(), evaluation);
    session.redo(session.version())?;
    assert_eq!(session.snapshot().document().hierarchy_order(), &order);
    let invalid = HierarchyOrder {
        organization: vec![first.clone(), first.clone()],
        ..Default::default()
    };
    assert!({
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.edit_organization(OrganizationEdit::Move {
                    ids: (invalid).organization,
                    destination: OrganizationCollection::PartMembers { part_id: None },
                    before: None,
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Invalid")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(session.snapshot().document().hierarchy_order(), &order);
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.edit_organization(OrganizationEdit::Delete {
                    ids: vec![first.clone()],
                    policy: OrganizationDeletePolicy::Contents,
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Erase")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }?;
    assert_eq!(
        session.snapshot().document().hierarchy_order().organization,
        vec![second]
    );
    session.undo(session.version())?;
    assert_eq!(session.snapshot().document().hierarchy_order(), &order);
    Ok(())
}

#[test]
fn project_codec_preserves_partial_order_and_rejects_invalid_references(
) -> Result<(), Box<dyn std::error::Error>> {
    let id = "00000000-0000-4000-8000-000000000001";
    let mut session = AuthoringSession::new(zipzi_sdk::Snapshot::new(
        zipzi_core::state::DocumentState::new(
            "00000000-0000-4000-8000-000000000002",
            Canvas::new(100., 100., Vec2::default(), 1.).into(),
        )?,
        Default::default(),
    )?);
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_part({
                    let mut value = Part {
                        id: id.into(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                edit.edit_organization(OrganizationEdit::Move {
                    ids: (HierarchyOrder {
                        organization: vec![id.into()],
                        ..Default::default()
                    })
                    .organization,
                    destination: OrganizationCollection::PartMembers { part_id: None },
                    before: None,
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Fixture")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }?;
    let mut model = session.snapshot().document().builder();
    model.set_hierarchy_order(HierarchyOrder {
        organization: vec![id.into()],
        ..Default::default()
    })?;
    let session = AuthoringSession::new(zipzi_sdk::Snapshot::new(
        model.freeze()?.state,
        Default::default(),
    )?);
    let encoded = archive::native_bytes(session.snapshot().document());
    let decoded = archive::load_native(&encoded).unwrap();
    assert_eq!(
        decoded.hierarchy_order(),
        session.snapshot().document().hierarchy_order()
    );
    let value = archive::project_json(&encoded);
    for invalid in [
        serde_json::json!({"organization": [id, id]}),
        serde_json::json!({"organization": ["00000000-0000-4000-8000-000000000099"]}),
        serde_json::json!({"deformation": [id]}),
    ] {
        let mut candidate = value.clone();
        candidate["document"]["hierarchy_order"] = invalid;
        assert!(
            archive::load_native(&archive::replace_project_json(&encoded, &candidate)).is_err()
        );
    }
    let mut legacy = value;
    legacy["document"]
        .as_object_mut()
        .unwrap()
        .remove("hierarchy_order");
    assert!(
        archive::load_native(&archive::replace_project_json(&encoded, &legacy))
            .unwrap()
            .hierarchy_order()
            .is_empty()
    );
    Ok(())
}
