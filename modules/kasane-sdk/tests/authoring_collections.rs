use kasane_core::{document::*, Canvas, Vec2};
use kasane_sdk::{
    AuthoringCollection as A, AuthoringSession, CapabilityState as C, PhysicsCollection,
};
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}
#[test]
fn root_queries_preserve_version_history_and_do_not_allocate_candidates() {
    let s = session();
    let version = s.version();
    let history = s.history_lengths();
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
            s.authoring_collection_capabilities(&c).unwrap().create,
            C::Available,
            "{c:?}"
        );
    }
    assert_eq!(
        s.authoring_collection_capabilities(&A::Physics(PhysicsCollection::Groups))
            .unwrap()
            .reorder,
        C::Available
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
}
#[test]
fn physics_collection_states_distinguish_owner_locks_from_semantic_chain_operations() {
    let mut s = session();
    let plan = s
        .plan_physics_edit(PhysicsEdit::CreateGroup {
            asset_id: id(2),
            id: id(3),
            name: "Rig".into(),
            before: None,
        })
        .unwrap();
    s.execute_physics_plan(&plan).unwrap();
    let inputs = A::Physics(PhysicsCollection::Inputs { group_id: id(3) });
    let nodes = A::Physics(PhysicsCollection::Nodes { group_id: id(3) });
    let groups = A::Physics(PhysicsCollection::Groups);
    assert_eq!(
        s.authoring_collection_capabilities(&inputs)
            .unwrap()
            .move_into,
        C::Available
    );
    assert_eq!(
        s.authoring_collection_capabilities(&inputs)
            .unwrap()
            .reorder,
        C::Available
    );
    assert_eq!(
        s.authoring_collection_capabilities(&groups)
            .unwrap()
            .reorder,
        C::Available
    );
    assert!(matches!(
        s.authoring_collection_capabilities(&nodes).unwrap().reorder,
        C::Unsupported { .. }
    ));
    s.edit("Lock", None, |e| {
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.entry(id(3)).or_default().locked = true;
        e.replace_editor_state(state)
    })
    .unwrap();
    let version = s.version();
    let history = s.history_lengths();
    assert_eq!(
        s.authoring_collection_capabilities(&inputs).unwrap().create,
        C::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert_eq!(
        s.authoring_collection_capabilities(&inputs)
            .unwrap()
            .move_into,
        C::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert!(matches!(
        s.authoring_collection_capabilities(&nodes).unwrap().reorder,
        C::Unsupported { .. }
    ));
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
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
        let history = s.history_lengths();
        let error = s
            .edit("Invalid", None, |e| {
                e.create_parameter(kasane_core::Parameter {
                    id: id(20 + n as u32),
                    runtime_id: format!("Param{n}"),
                    minimum,
                    maximum,
                    default_value,
                    ..Default::default()
                })
            })
            .unwrap_err();
        assert_eq!(error.code.as_ref(), "INVALID_PARAMETER");
        assert_eq!(s.version(), version);
        assert_eq!(s.history_lengths(), history);
    }
    assert_eq!(validate_parameter_range(-1., 1., -1.), Ok(()));
    assert_eq!(validate_parameter_range(-1., 1., 1.), Ok(()));
}

#[test]
fn locked_collection_owners_roundtrip_after_all_authoring_identities_are_restored() {
    let mut s = session();
    let p = s
        .plan_organization_edit(kasane_sdk::OrganizationEdit::CreateGroup {
            group: CdiParameterGroup {
                id: id(2),
                runtime_id: "Folder".into(),
                name: "Folder".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            before: None,
        })
        .unwrap();
    s.execute_organization_plan(&p).unwrap();
    let p = s
        .plan_physics_edit(PhysicsEdit::CreateGroup {
            asset_id: id(3),
            id: id(4),
            name: "Rig".into(),
            before: None,
        })
        .unwrap();
    s.execute_physics_plan(&p).unwrap();
    let p = s
        .plan_pose_edit(PoseEdit::CreateGroup {
            pose_id: id(5),
            id: id(6),
            name: "Pose".into(),
            before: None,
        })
        .unwrap();
    s.execute_pose_plan(&p).unwrap();
    let mut state = s.document().editor_state().clone();
    for n in [2, 3, 4, 5, 6] {
        state.objects.entry(id(n)).or_default().locked = true;
    }
    s.edit("Lock owners", None, |e| e.replace_editor_state(state))
        .unwrap();
    let bytes = kasane_project::encode_project(s.document()).unwrap();
    let restored = kasane_project::decode_project(&bytes).unwrap();
    assert!(restored.same_content(s.document()));
    assert_eq!(restored.editor_state(), s.document().editor_state());
}
