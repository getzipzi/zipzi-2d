//! Cross-domain atomicity and persistence through Draft and PreparedPreview.
use crate::support::{archive, prepared};
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::state::{DocumentState, RecordChange};
use zipzi_core::{document::*, BindingAxisEditing, Canvas, Mesh, Parameter, Part, Vec2};
use zipzi_sdk::{AuthoringSession, Snapshot};
fn id(n: u32) -> String {
    format!("60000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 1.).into()).unwrap(),
            Default::default(),
        )
        .unwrap(),
    )
}
fn json(s: &AuthoringSession) -> serde_json::Value {
    archive::project_json(&archive::native_bytes_with_resources(
        s.snapshot().document(),
        s.snapshot().resources(),
    ))
}
fn create_all(edit: &mut zipzi_sdk::Commands<'_>) -> Result<(), zipzi_core::Status> {
    edit.edit_resource(ResourceEdit::Create {
        id: id(10),
        name: "Pending image".into(),
        before: None,
    })?;
    edit.edit_attachment(AttachmentEdit::Create {
        id: id(11),
        path: "files/sound.wav".into(),
    })?;
    edit.edit_organization(OrganizationEdit::CreateGroup {
        group: CdiParameterGroup {
            id: id(20),
            runtime_id: "Parameters".into(),
            name: "Empty group".into(),
            parent_id: None,
            extensions: Default::default(),
        },
        before: None,
    })?;
    edit.create_parameter(Parameter {
        id: id(21),
        runtime_id: "Param".into(),
        ..Default::default()
    })?;
    edit.create_part({
        let mut value = Part {
            id: id(22),
            ..Default::default()
        };
        value.runtime_id = value.id.clone();
        value
    })?;
    edit.create_mesh({
        let mut value = Mesh {
            id: id(30),
            texture_asset_id: id(10).into(),
            ..Default::default()
        };
        value.runtime_id = value.id.clone();
        value
    })?;
    edit.edit_binding_grid(
        BindingGridEdit::Create {
            id: id(31),
            target: BindingGridTarget::Mesh(id(30)),
        },
        Default::default(),
        &Default::default(),
    )?;
    edit.edit_scene(
        SceneEdit::Create {
            id: id(32),
            name: "Pending glue".into(),
            object: SceneCreate::Glue {
                a: Default::default(),
                b: Default::default(),
            },
            part_id: None,
            parent_id: None,
            before: None,
            deformation_before: None,
        },
        Default::default(),
        &Default::default(),
    )?;
    edit.edit_expression(ExpressionEdit::Create {
        id: id(40),
        name: "Expression".into(),
    })?;
    edit.edit_expression(ExpressionEdit::CreateEntry {
        owner: id(40),
        id: id(41),
        before: None,
    })?;
    edit.edit_motion(MotionEdit::CreateGroup {
        id: id(50),
        name: "Empty motions".into(),
    })?;
    edit.edit_motion(MotionEdit::Create {
        id: id(51),
        name: "Motion".into(),
        registration_id: id(52),
        destination: MotionContainer::Root,
    })?;
    edit.edit_motion(MotionEdit::CreateTrack {
        clip: id(51),
        id: id(53),
        key_id: id(54),
        time: 0.,
    })?;
    edit.edit_physics(PhysicsEdit::CreateGroup {
        asset_id: id(60),
        id: id(61),
        name: "Rig".into(),
        before: None,
    })?;
    edit.edit_physics(PhysicsEdit::CreateInput {
        group_id: id(61),
        id: id(62),
        before: None,
    })?;
    edit.edit_pose(PoseEdit::CreateGroup {
        pose_id: id(70),
        id: id(71),
        name: "Pose".into(),
        before: None,
    })?;
    edit.edit_pose(PoseEdit::CreateEntry {
        group_id: id(71),
        id: id(72),
        before: None,
    })?;
    edit.edit_pose(PoseEdit::CreateLink {
        entry_id: id(72),
        id: id(73),
        before: None,
    })?;
    edit.edit_reference(ReferenceEdit::CreateModelParameterGroup {
        id: id(80),
        name: "EyeBlink".into(),
        before: None,
    })?;
    edit.edit_reference(ReferenceEdit::CreateMember {
        id: id(81),
        collection: ReferenceCollection::ModelParameterMembers { group_id: id(80) },
        before: None,
    })?;
    edit.edit_document(DocumentAuthoringEdit::Patch {
        expected: DocumentValue::Layout(None),
        value: DocumentValue::Layout(Some(std::collections::BTreeMap::from([(
            "CenterX".into(),
            2.,
        )]))),
    })
    .map(|_| ())
}

#[test]
fn stale_cross_domain_candidates_cannot_publish_even_after_undo() {
    let mut s = session();
    let (_, stale) = prepared(&s, create_all).unwrap();
    let (_, foreign) = prepared(&s, create_all).unwrap();
    let mut other = session();
    assert_eq!(other.publish(foreign).unwrap_err().code, "STALE_SOURCE");
    let (_, current) = prepared(&s, |c| {
        c.create_part(Part {
            id: id(999),
            runtime_id: "intervening".into(),
            ..Default::default()
        })
    })
    .unwrap();
    s.publish(current).unwrap();
    s.undo(s.version()).unwrap();
    s.take_events();
    let before = s.read_snapshot();
    let history = s.history_state();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    assert_eq!(s.read_snapshot().source, before.source);
    assert_eq!(s.history_state(), history);
    assert!(s.take_events().is_empty());
}
#[test]
fn all_domains_publish_once_persist_pending_state_and_reverse_one_history_entry() {
    let mut s = session();
    let empty = s.snapshot().content_id();
    let (_, change) = prepared(&s, create_all).unwrap();
    let receipt = s.publish(change).unwrap();
    assert_eq!(s.history_state().undo_steps, 1);
    let events = s.take_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].version, receipt.version);
    assert_eq!(events[0].writes, receipt.writes);
    let identities = [
        (ObjectKind::Asset, 10),
        (ObjectKind::Attachment, 11),
        (ObjectKind::CdiParameterGroup, 20),
        (ObjectKind::Parameter, 21),
        (ObjectKind::Part, 22),
        (ObjectKind::Mesh, 30),
        (ObjectKind::MeshBinding, 31),
        (ObjectKind::Glue, 32),
        (ObjectKind::Expression, 40),
        (ObjectKind::ExpressionEntry, 41),
        (ObjectKind::MotionGroup, 50),
        (ObjectKind::Motion, 51),
        (ObjectKind::MotionRegistration, 52),
        (ObjectKind::MotionTrack, 53),
        (ObjectKind::MotionKey, 54),
        (ObjectKind::Physics, 60),
        (ObjectKind::PhysicsGroup, 61),
        (ObjectKind::PhysicsInput, 62),
        (ObjectKind::Pose, 70),
        (ObjectKind::PoseGroup, 71),
        (ObjectKind::PoseEntry, 72),
        (ObjectKind::PoseLink, 73),
        (ObjectKind::ModelParameterGroup, 80),
        (ObjectKind::ModelParameterMember, 81),
    ];
    for (kind, n) in identities {
        s.snapshot()
            .document()
            .require_object(kind, &id(n))
            .unwrap();
    }
    let saved = json(&s);
    let restored = crate::support::roundtrip(s.snapshot());
    let reopened = restored.document();
    assert!(matches!(
        reopened.asset(&id(10)).unwrap().content,
        zipzi_core::state::AssetContent::Empty
    ));
    assert_eq!(
        reopened.expression_entry(&id(41)).unwrap().1.target,
        ExpressionTarget::Unspecified
    );
    assert!(reopened.binding(&id(31)).unwrap().axes.is_empty());
    assert!(reopened.motion_group(&id(50)).unwrap().entries.is_empty());
    assert_eq!(json(&AuthoringSession::new(restored)), saved);
    let captured_source = s.read_snapshot().source;
    let undo = s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), empty);
    for (forward, inverse) in receipt.writes.records().iter().zip(undo.writes.records()) {
        assert_eq!((forward.domain, &forward.id), (inverse.domain, &inverse.id));
        assert_eq!(forward.change, RecordChange::Created);
        assert_eq!(inverse.change, RecordChange::Deleted);
    }
    let redo = s.redo(s.version()).unwrap();
    assert_eq!(redo.writes, receipt.writes);
    assert_eq!(json(&s), saved);
    assert_ne!(s.read_snapshot().source, captured_source);
    let events = s.take_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].version, undo.version);
    assert_eq!(events[1].version, redo.version);
    for (kind, n) in identities {
        s.snapshot()
            .document()
            .require_object(kind, &id(n))
            .unwrap();
    }
}
#[test]
fn a_late_domain_failure_rolls_back_every_domain_and_preserves_redo() {
    let mut s = session();
    let (_, change) = prepared(&s, create_all).unwrap();
    s.publish(change).unwrap();
    s.undo(s.version()).unwrap();
    s.take_events();
    let source = s.read_snapshot().source;
    let history = s.history_state();
    let empty = json(&s);
    let mut draft = s.begin_draft();
    assert!(draft
        .batch(draft.source(), |c| {
            create_all(c)?;
            c.edit_reference(ReferenceEdit::CreateMember {
                id: id(82),
                collection: ReferenceCollection::ModelParameterMembers { group_id: id(999) },
                before: None,
            })
        })
        .is_err());
    assert!(draft.prepare("failed batch").is_err());
    assert_eq!(s.read_snapshot().source, source);
    assert_eq!(s.history_state(), history);
    assert!(s.take_events().is_empty());
    assert_eq!(json(&s), empty);
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().expression_entry(&id(41)).is_some());
}
#[test]
fn draft_and_preview_routes_publish_identical_domain_results() {
    let mut sdk = session();
    let (_, change) = prepared(&sdk, create_all).unwrap();
    sdk.publish(change).unwrap();
    sdk.take_events();
    macro_rules! parity {
        ($intent:expr, $edit:ident $(, $extra:expr)*) => {{
            let intent = $intent; let before = sdk.read_snapshot().source; let history = sdk.history_state();
            let (_, change) = prepared(&sdk, |c| c.$edit(intent.clone() $(, $extra)*)).unwrap();
            assert_eq!(sdk.read_snapshot().source, before); assert_eq!(sdk.history_state(), history); assert!(sdk.take_events().is_empty());
            let direct = sdk.publish(change).unwrap(); assert!(!direct.writes.is_empty()); let saved = json(&sdk); sdk.undo(sdk.version()).unwrap(); sdk.take_events();
            let mut draft = sdk.begin_draft(); let (preview, _) = draft.prepare_preview(draft.source(), |c| c.$edit(intent $(, $extra)*)).unwrap();
            draft.install_preview(preview).unwrap();
            let shown = draft.read_snapshot().unwrap().snapshot.content_id();
            let publication = sdk.publish(draft.prepare("preview").unwrap()).unwrap();
            assert_eq!(publication.writes.records(), direct.writes.records()); let mut a=serde_json::to_value(&publication.writes).unwrap(); let mut b=serde_json::to_value(&direct.writes).unwrap(); a.as_object_mut().unwrap().remove("estimated_bytes"); b.as_object_mut().unwrap().remove("estimated_bytes"); assert_eq!(a,b); assert_eq!(sdk.snapshot().content_id(), shown); assert_eq!(json(&sdk), saved); sdk.undo(sdk.version()).unwrap(); sdk.take_events();
        }};
    }
    parity!(
        ExpressionEdit::PatchEntry {
            id: id(41),
            expected: sdk
                .snapshot()
                .document()
                .expression_entry(&id(41))
                .unwrap()
                .1
                .into(),
            fields: ExpressionEntryFields {
                target: ExpressionTarget::Resolved {
                    parameter_id: id(21)
                },
                value: 0.5,
                blend: None
            },
        },
        edit_expression
    );
    parity!(
        MotionEdit::SetTrackTarget {
            id: id(53),
            target: MotionTrackTarget::Parameter {
                parameter_id: id(21)
            }
        },
        edit_motion
    );
    parity!(
        PoseEdit::Patch {
            id: id(72),
            expected: PoseFields::Entry {
                part: PosePartRef::Unspecified
            },
            fields: PoseFields::Entry {
                part: PosePartRef::Resolved { part_id: id(22) }
            },
        },
        edit_pose
    );
    parity!(
        ReferenceEdit::Patch {
            id: id(81),
            expected: ReferenceFields::ModelParameterMember {
                target: ModelTargetRef::Unspecified
            },
            fields: ReferenceFields::ModelParameterMember {
                target: ModelTargetRef::Resolved { object_id: id(21) }
            },
        },
        edit_reference
    );
    parity!(
        OrganizationEdit::Move {
            ids: vec![id(21)],
            destination: OrganizationCollection::Parameters {
                group_id: Some(id(20))
            },
            before: None,
        },
        edit_organization
    );
    parity!(
        SceneEdit::Patch {
            id: id(30),
            expected: SceneFieldValue::Label(String::new()),
            value: SceneFieldValue::Label("Renamed".into()),
            clear_glue_pairs: false,
        },
        edit_scene,
        Default::default(),
        &Default::default()
    );
    parity!(
        ResourceEdit::PatchName {
            id: id(10),
            expected: "Pending image".into(),
            name: "Renamed".into()
        },
        edit_resource
    );
    parity!(
        DocumentAuthoringEdit::Patch {
            expected: sdk
                .snapshot()
                .document()
                .document_value(DocumentField::Layout)
                .unwrap(),
            value: DocumentValue::Layout(None),
        },
        edit_document
    );
    parity!(
        PhysicsEdit::Patch {
            patch: PhysicsPatch {
                asset_id: id(60),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: id(61),
                    changes: vec![PhysicsProperty::Name("Renamed".into())]
                }),
            }
        },
        edit_physics
    );
    parity!(
        BindingGridEdit::Edit {
            owner: id(31),
            operation: GridOperation::AddAxis {
                axis: zipzi_core::BindingAxis::new(id(21), vec![0., 1.]),
            },
        },
        edit_binding_grid,
        Default::default(),
        &Default::default()
    );
    parity!(
        SceneContentEdit::InsertMeshVertices {
            mesh_id: id(30),
            vertices: vec![NewMeshVertex {
                vertex_id: 1,
                position: Vec2::new(2., 3.),
                uv: Vec2::new(0., 0.),
                seed: VertexSeed::Base,
            }],
            before: None,
        },
        edit_scene_content,
        Default::default(),
        &Default::default()
    );
    let expected = sdk
        .snapshot()
        .document()
        .attachment(&id(11))
        .unwrap()
        .clone();
    let mut attachment = expected.clone();
    attachment.bytes = Some(std::sync::Arc::new(vec![1, 2, 3]));
    parity!(
        AttachmentEdit::Replace {
            expected,
            attachment
        },
        edit_attachment
    );
}
