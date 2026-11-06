use zipzi_contract::authoring::CapabilityState as C;
use zipzi_contract::{authoring::PhysicsCollection, kernel_wire::CollectionTarget as A};
use zipzi_core::{document::*, Canvas, Vec2};
use zipzi_sdk::AuthoringSession;
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}
#[test]
fn root_queries_preserve_version_history_and_do_not_allocate_candidates() {
    let s = session();
    let version = s.version();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    for c in [
        A::Motion(MotionContainer::Root),
        A::MotionGroups,
        A::Clips,
        A::Expressions,
        A::Resources,
        A::Attachments,
        A::Pose(PoseCollection::Groups),
        A::Physics(PhysicsCollection::Groups),
    ] {
        assert_eq!(
            s.snapshot()
                .document()
                .collection_capabilities(&c)
                .unwrap()
                .create,
            C::Available,
            "{c:?}"
        );
    }
    assert_eq!(
        s.snapshot()
            .document()
            .collection_capabilities(&A::Physics(PhysicsCollection::Groups))
            .unwrap()
            .reorder,
        C::Available
    );
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
}
#[test]
fn physics_collection_states_distinguish_owner_locks_from_semantic_chain_operations() {
    let mut s = session();
    let plan = crate::support::prepared(&s, |c| {
        c.edit_physics(PhysicsEdit::CreateGroup {
            asset_id: id(2),
            id: id(3),
            name: "Rig".into(),
            before: None,
        })
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    let inputs = A::Physics(PhysicsCollection::Inputs { group_id: id(3) });
    let nodes = A::Physics(PhysicsCollection::Nodes { group_id: id(3) });
    let groups = A::Physics(PhysicsCollection::Groups);
    assert_eq!(
        s.snapshot()
            .document()
            .collection_capabilities(&inputs)
            .unwrap()
            .move_into,
        C::Available
    );
    assert_eq!(
        s.snapshot()
            .document()
            .collection_capabilities(&inputs)
            .unwrap()
            .reorder,
        C::Available
    );
    assert_eq!(
        s.snapshot()
            .document()
            .collection_capabilities(&groups)
            .unwrap()
            .reorder,
        C::Available
    );
    assert!(matches!(
        s.snapshot()
            .document()
            .collection_capabilities(&nodes)
            .unwrap()
            .reorder,
        C::Unsupported { .. }
    ));
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut state = e.read_document().unwrap().editor_state().clone();
                state.objects.entry(id(3)).or_default().locked = true;
                e.replace_editor_state(state)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = s.version();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    assert_eq!(
        s.snapshot()
            .document()
            .collection_capabilities(&inputs)
            .unwrap()
            .create,
        C::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert_eq!(
        s.snapshot()
            .document()
            .collection_capabilities(&inputs)
            .unwrap()
            .move_into,
        C::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert!(matches!(
        s.snapshot()
            .document()
            .collection_capabilities(&nodes)
            .unwrap()
            .reorder,
        C::Unsupported { .. }
    ));
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
}
#[test]
fn parameter_range_helper_matches_transaction_validation_and_rolls_back_rejection() {
    let mut s = session();
    for (n, (minimum, maximum, default_value, reason)) in [
        (f32::NAN, 1., 0., ParameterRangeError::MinimumInvalid),
        (0., f32::INFINITY, 0., ParameterRangeError::MaximumInvalid),
        (0., 1., f32::NAN, ParameterRangeError::DefaultInvalid),
        (1., 1., 1., ParameterRangeError::RangeInvalid),
        (-f32::MAX, f32::MAX, 0., ParameterRangeError::RangeOverflow),
        (0., 1., 2., ParameterRangeError::DefaultOutOfRange),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            validate_parameter_range(minimum, maximum, default_value),
            Err(reason)
        );
        let version = s.version();
        let history = {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        };
        let error = {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.create_parameter(zipzi_core::Parameter {
                        id: id(20 + n as u32),
                        runtime_id: format!("Param{n}"),
                        minimum,
                        maximum,
                        default_value,
                        ..Default::default()
                    })
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Invalid")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err();
        assert_eq!(error.code.as_str(), "INVALID_PARAMETER");
        assert_eq!(s.version(), version);
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            history
        );
    }
    assert_eq!(validate_parameter_range(-1., 1., -1.), Ok(()));
    assert_eq!(validate_parameter_range(-1., 1., 1.), Ok(()));
}

#[test]
fn locked_collection_owners_roundtrip_after_all_authoring_identities_are_restored() {
    let mut s = session();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(zipzi_sdk::OrganizationEdit::CreateGroup {
            group: CdiParameterGroup {
                id: id(2),
                runtime_id: "Folder".into(),
                name: "Folder".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            before: None,
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    let p = crate::support::prepared(&s, |c| {
        c.edit_physics(PhysicsEdit::CreateGroup {
            asset_id: id(3),
            id: id(4),
            name: "Rig".into(),
            before: None,
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    let p = crate::support::prepared(&s, |c| {
        c.edit_pose(PoseEdit::CreateGroup {
            pose_id: id(5),
            id: id(6),
            name: "Pose".into(),
            before: None,
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    let mut state = s.snapshot().document().editor_state().clone();
    for n in [2, 3, 4, 5, 6] {
        state.objects.entry(id(n)).or_default().locked = true;
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock owners")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let restored = crate::support::roundtrip(s.snapshot());
    assert_eq!(
        restored.document().editor_state(),
        s.snapshot().document().editor_state()
    );
    for n in [2, 3, 4, 5, 6] {
        assert!(restored.document().editor_state().objects[&id(n)].locked);
    }
}
