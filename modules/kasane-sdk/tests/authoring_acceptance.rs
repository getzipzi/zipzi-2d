//! Cross-domain publication and persistence contracts for the Editor handoff.
use kasane_core::{document::*, Canvas, Mesh, Parameter, Part, Vec2};
use kasane_sdk::{AuthoringSession, BindingAuthoringEdit, ChangeSet, EditSession, ObjectKind};

fn id(n: u32) -> String {
    format!("60000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}

#[test]
fn stale_domain_plans_abort_candidate_writes_without_publication() {
    fn reject(
        sdk: &mut AuthoringSession,
        operation: &str,
        apply: impl FnOnce(&mut EditSession<'_>) -> Result<(), kasane_sdk::SdkError>,
    ) {
        let version = sdk.version();
        let history = sdk.history_lengths();
        let mut edit = sdk.begin_edit("stale plan", None).unwrap();
        edit.create_part(Part {
            id: id(999),
            ..Default::default()
        })
        .unwrap();
        let error = apply(&mut edit).unwrap_err();
        assert_eq!(error.code.as_ref(), "STALE_PLAN");
        assert_eq!(error.operation, operation);
        assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
        assert_eq!(sdk.version(), version);
        assert_eq!(sdk.history_lengths(), history);
        assert!(sdk.document().get_part(&id(999)).is_none());
        assert!(sdk.drain_events().is_empty());
    }

    let mut sdk = session();
    let motion = sdk
        .plan_motion_edit(MotionEdit::CreateGroup {
            id: id(10),
            name: "Planned".into(),
        })
        .unwrap();
    let attachment = sdk
        .plan_attachment_edit(AttachmentEdit::Create {
            id: id(11),
            path: "files/planned.wav".into(),
        })
        .unwrap();
    let physics = sdk
        .plan_physics_edit(PhysicsEdit::CreateGroup {
            asset_id: id(12),
            id: id(13),
            name: "Planned".into(),
            before: None,
        })
        .unwrap();
    let expression = sdk
        .plan_expression_edit(ExpressionEdit::Create {
            id: id(14),
            name: "Planned".into(),
        })
        .unwrap();
    sdk.edit("Advance version", None, |e| {
        e.create_part(Part {
            id: id(20),
            ..Default::default()
        })
    })
    .unwrap();
    sdk.drain_events();

    reject(&mut sdk, "apply_motion_plan", |e| {
        e.apply_motion_plan(&motion)
    });
    reject(&mut sdk, "apply_attachment_plan", |e| {
        e.apply_attachment_plan(&attachment)
    });
    reject(&mut sdk, "apply_physics_plan", |e| {
        e.apply_physics_plan(&physics)
    });
    reject(&mut sdk, "apply_expression_plan", |e| {
        e.apply_expression_plan(&expression)
    });
}

fn create_all(edit: &mut EditSession<'_>) -> Result<(), kasane_sdk::SdkError> {
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
    edit.create_part(Part {
        id: id(22),
        ..Default::default()
    })?;
    edit.create_mesh(Mesh {
        id: id(30),
        texture_asset_id: id(10).into(),
        ..Default::default()
    })?;
    edit.binding_authoring(BindingAuthoringEdit::Grid(BindingGridEdit::Create {
        id: id(31),
        target: BindingGridTarget::Mesh(id(30)),
    }))?;
    edit.edit_scene(SceneEdit::Create {
        id: id(32),
        name: "Pending glue".into(),
        object: SceneCreate::Glue {
            a: Default::default(),
            b: Default::default(),
        },
        part_id: None,
        parent_id: None,
        before: None,
    })?;
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
    edit.edit_references(ReferenceEdit::CreateModelParameterGroup {
        id: id(80),
        name: "EyeBlink".into(),
        before: None,
    })?;
    edit.edit_references(ReferenceEdit::CreateMember {
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
}

fn assert_inverse(forward: &ChangeSet, inverse: &ChangeSet) {
    assert_eq!(forward.created, inverse.deleted);
    assert_eq!(forward.deleted, inverse.created);
    assert_eq!(forward.modified, inverse.modified);
    assert_eq!(forward.collections, inverse.collections);
    assert_eq!(forward.categories, inverse.categories);
    assert_eq!(forward.document_fields, inverse.document_fields);
    // Check every reference family, including families that a fixture leaves empty.
    let a = serde_json::to_value(forward).unwrap();
    let b = serde_json::to_value(inverse).unwrap();
    for field in [
        "references",
        "motion_references",
        "reference_fields",
        "reference_selections",
        "content_references",
        "scene_references",
        "organization_references",
        "pose_references",
        "physics_references",
        "binding_references",
    ] {
        let expected: Vec<_> = a[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| {
                let mut edge = edge.clone();
                let old = edge["before"].take();
                edge["before"] = edge["after"].take();
                edge["after"] = old;
                if edge.get("before_owner").is_some() {
                    let old = edge["before_owner"].take();
                    edge["before_owner"] = edge["after_owner"].take();
                    edge["after_owner"] = old;
                }
                edge
            })
            .collect();
        assert_eq!(serde_json::Value::Array(expected), b[field], "{field}");
    }
}

#[test]
fn all_domains_publish_once_persist_pending_state_and_reverse_one_history_entry() {
    let mut sdk = session();
    let empty = kasane_project::encode_project(sdk.document()).unwrap();
    let (_, receipt) = sdk.edit("Authoring acceptance", None, create_all).unwrap();
    assert_eq!(sdk.history_lengths(), (1, 0));
    assert_eq!(sdk.drain_events(), vec![receipt.clone()]);
    assert!(!receipt.changes.full_invalidation);
    // Enumeration used for receipts and direct lookup must agree for owned rows
    // as well as ordered roots, including generated Physics nodes and samples.
    for entity in &receipt.changes.created {
        let handle = sdk.handle(entity.kind, &entity.id).unwrap();
        sdk.resolve_handle(&handle).unwrap();
    }
    let handles: Vec<_> = [
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
    ]
    .into_iter()
    .map(|(kind, n)| {
        assert!(
            receipt
                .changes
                .created
                .iter()
                .any(|entity| entity.kind == kind && entity.id == id(n)),
            "{kind:?}"
        );
        sdk.handle(kind, &id(n)).unwrap()
    })
    .collect();
    let saved = kasane_project::encode_project(sdk.document()).unwrap();
    let reopened = kasane_project::decode_project(&saved).unwrap();
    assert_eq!(kasane_project::encode_project(&reopened).unwrap(), saved);
    assert!(reopened.validate_structure().is_empty());
    assert!(reopened.get_asset(&id(10)).unwrap().content.is_none());
    assert!(
        reopened.get_expression_entry(&id(41)).unwrap().1.target == ExpressionTarget::Unspecified
    );
    assert!(reopened.get_binding(&id(31)).unwrap().axes.is_empty());
    assert!(reopened
        .get_motion_group(&id(50))
        .unwrap()
        .entries
        .is_empty());
    let undo = sdk.undo().unwrap();
    assert_inverse(&receipt.changes, &undo.changes);
    assert_eq!(
        kasane_project::encode_project(sdk.document()).unwrap(),
        empty
    );
    let redo = sdk.redo().unwrap();
    assert_eq!(redo.changes, receipt.changes);
    assert_eq!(
        kasane_project::encode_project(sdk.document()).unwrap(),
        saved
    );
    assert_eq!(sdk.drain_events(), vec![undo, redo]);
    for old in handles {
        assert_eq!(
            sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
            "STALE_HANDLE"
        );
        sdk.resolve_handle(&sdk.handle(old.kind(), old.id()).unwrap())
            .unwrap();
    }
}

#[test]
fn a_late_domain_failure_rolls_back_every_domain_and_preserves_redo() {
    let mut sdk = session();
    sdk.edit("Populate", None, create_all).unwrap();
    sdk.undo().unwrap();
    sdk.drain_events();
    let version = sdk.version();
    let empty = kasane_project::encode_project(sdk.document()).unwrap();
    let failure = sdk
        .edit("Rejected batch", None, |edit| {
            create_all(edit)?;
            edit.edit_references(ReferenceEdit::CreateMember {
                id: id(82),
                collection: ReferenceCollection::ModelParameterMembers { group_id: id(999) },
                before: None,
            })
        })
        .unwrap_err();
    assert!(!failure.code.is_empty());
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), (0, 1));
    assert!(sdk.drain_events().is_empty());
    assert_eq!(
        kasane_project::encode_project(sdk.document()).unwrap(),
        empty
    );
    sdk.redo().unwrap();
    assert!(sdk.document().get_expression_entry(&id(41)).is_some());
}

#[test]
fn planned_and_direct_domain_writes_publish_identical_results() {
    let mut sdk = session();
    sdk.edit("Populate", None, create_all).unwrap();
    sdk.drain_events();
    macro_rules! parity {
        ($intent:expr, $plan:ident, $execute:ident, $edit:ident) => {{
            let intent = $intent;
            let version = sdk.version();
            let history = sdk.history_lengths();
            let plan = sdk.$plan(intent.clone()).unwrap();
            assert_eq!(sdk.version(), version);
            assert_eq!(sdk.history_lengths(), history);
            assert!(sdk.drain_events().is_empty());
            let planned = sdk.$execute(&plan).unwrap();
            assert!(planned.changed);
            let saved = kasane_project::encode_project(sdk.document()).unwrap();
            let undo = sdk.undo().unwrap();
            assert_inverse(&planned.changes, &undo.changes);
            assert_eq!(sdk.drain_events(), vec![planned.clone(), undo]);
            let (_, direct) = sdk
                .edit("Direct edit", None, |edit| edit.$edit(intent))
                .unwrap();
            assert_eq!(direct.changes, planned.changes);
            assert_eq!(
                kasane_project::encode_project(sdk.document()).unwrap(),
                saved
            );
            let undo = sdk.undo().unwrap();
            assert_eq!(sdk.drain_events(), vec![direct, undo]);
        }};
    }
    parity!(
        ExpressionEdit::PatchEntry {
            id: id(41),
            expected: sdk
                .document()
                .get_expression_entry(&id(41))
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
        plan_expression_edit,
        execute_expression_plan,
        edit_expression
    );
    parity!(
        MotionEdit::SetTrackTarget {
            id: id(53),
            target: MotionTrackTarget::Parameter {
                parameter_id: id(21)
            }
        },
        plan_motion_edit,
        execute_motion_plan,
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
        plan_pose_edit,
        execute_pose_plan,
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
        plan_reference_edit,
        execute_reference_plan,
        edit_references
    );
    parity!(
        OrganizationEdit::Move {
            ids: vec![id(21)],
            destination: OrganizationCollection::Parameters {
                group_id: Some(id(20))
            },
            before: None,
        },
        plan_organization_edit,
        execute_organization_plan,
        edit_organization
    );
    parity!(
        SceneEdit::Patch {
            id: id(30),
            expected: SceneFieldValue::Label(String::new()),
            value: SceneFieldValue::Label("Renamed".into()),
            clear_glue_pairs: false,
        },
        plan_scene_edit,
        execute_scene_plan,
        edit_scene
    );
    parity!(
        ResourceEdit::PatchName {
            id: id(10),
            expected: "Pending image".into(),
            name: "Renamed".into()
        },
        plan_resource_edit,
        execute_resource_plan,
        edit_resource
    );
    parity!(
        DocumentAuthoringEdit::Patch {
            expected: sdk
                .document()
                .document_value(DocumentField::Layout)
                .unwrap(),
            value: DocumentValue::Layout(None),
        },
        plan_document_edit,
        execute_document_plan,
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
        plan_physics_edit,
        execute_physics_plan,
        edit_physics
    );
    parity!(
        BindingAuthoringEdit::Grid(BindingGridEdit::Edit {
            owner: id(31),
            operation: GridOperation::AddAxis {
                axis: kasane_core::BindingAxis::new(id(21), vec![0., 1.]),
            },
        }),
        plan_binding_authoring,
        execute_binding_plan,
        binding_authoring
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
        plan_scene_content,
        execute_scene_content_plan,
        edit_scene_content
    );
    let expected = sdk.document().get_attachment(&id(11)).unwrap().clone();
    let mut attachment = expected.clone();
    attachment.bytes = Some(std::sync::Arc::new(vec![1, 2, 3]));
    parity!(
        AttachmentEdit::Replace {
            expected,
            attachment
        },
        plan_attachment_edit,
        execute_attachment_plan,
        edit_attachment
    );
}
