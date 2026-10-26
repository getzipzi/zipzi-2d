use zipzi_core::document::{ExpressionEdit, MotionContainer, MotionEdit, PhysicsEdit, PoseEdit};
use zipzi_runtime::{Runtime, SourceRef, modeling::ModelingCommand};
use zipzi_sdk::{ModelingIntent, ModelingQuery};

fn id(n: u32) -> String {
    format!("79610000-0000-4000-8000-{n:012x}")
}

#[test]
fn animation_domains_share_candidate_publication_plans_and_selected_reads() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let before = rt.session_info(session).unwrap();
    let edit = rt.begin_edit(client, session, "animation", None).unwrap();
    rt.apply_modeling(
        client,
        edit,
        vec![
            ModelingCommand::Motion {
                intent: MotionEdit::Create {
                    id: id(2),
                    name: "Motion".into(),
                    registration_id: id(3),
                    destination: MotionContainer::Root,
                },
            },
            ModelingCommand::Expression {
                intent: ExpressionEdit::Create {
                    id: id(4),
                    name: "Expression".into(),
                },
            },
            ModelingCommand::Physics {
                intent: PhysicsEdit::CreateGroup {
                    asset_id: id(5),
                    id: id(6),
                    name: "Physics".into(),
                    before: None,
                },
            },
            ModelingCommand::Pose {
                intent: PoseEdit::CreateGroup {
                    pose_id: id(7),
                    id: id(8),
                    name: "Pose".into(),
                    before: None,
                },
            },
        ],
    )
    .unwrap();
    let query = ModelingQuery {
        motions: vec![id(2), id(99), id(2)],
        motion_registrations: vec![id(3)],
        expressions: vec![id(4)],
        physics: true,
        pose: true,
        ..Default::default()
    };
    let selected = rt
        .modeling_snapshot(client, session, Some(edit), query.clone())
        .unwrap();
    assert!(matches!(
        selected.source,
        SourceRef::Edit { sequence: 1, .. }
    ));
    assert_eq!(selected.records.motions[0].as_ref().unwrap().name, "Motion");
    assert!(selected.records.motions[1].is_none());
    assert_eq!(selected.records.motions[0], selected.records.motions[2]);
    assert_eq!(
        selected.records.motion_registrations[0]
            .as_ref()
            .unwrap()
            .clip_id,
        id(2)
    );
    assert_eq!(selected.records.physics.as_ref().unwrap().id, id(5));
    assert_eq!(selected.records.pose.as_ref().unwrap().id, id(7));
    assert_eq!(rt.session_info(session).unwrap().version, before.version);
    assert!(
        rt.modeling_snapshot(client, session, None, query.clone())
            .unwrap()
            .records
            .motions[0]
            .is_none()
    );
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(rt.session_info(session).unwrap().history_lengths, (1, 0));
    let snapshot = rt.source_snapshot(session).unwrap();
    let plan = snapshot
        .snapshot
        .plan_modeling(
            ModelingIntent::Expression(ExpressionEdit::CreateEntry {
                owner: id(4),
                id: id(9),
                before: None,
            }),
            Default::default(),
            &Default::default(),
        )
        .unwrap();
    assert!(plan.report().affected_ids.contains(&id(4)));
    let candidate = rt
        .begin_edit(client, session, "expression plan", None)
        .unwrap();
    rt.apply_edit(client, candidate, |edit| plan.apply(edit))
        .unwrap();
    rt.commit_edit(client, candidate).unwrap();
    let populated = rt
        .modeling_snapshot(client, session, None, query.clone())
        .unwrap();
    assert_eq!(
        populated.records.expressions[0].as_ref().unwrap().entries[0].id,
        id(9)
    );
    rt.undo(client, session).unwrap();
    assert!(
        rt.modeling_snapshot(client, session, None, query.clone())
            .unwrap()
            .records
            .expressions[0]
            .as_ref()
            .unwrap()
            .entries
            .is_empty()
    );
    rt.redo(client, session).unwrap();
    assert_eq!(
        rt.modeling_snapshot(client, session, None, query)
            .unwrap()
            .records
            .expressions[0]
            .as_ref()
            .unwrap()
            .entries[0]
            .id,
        id(9)
    );
    // Selected snapshots own their old content after publication/Undo/Redo.
    assert!(
        selected.records.expressions[0]
            .as_ref()
            .unwrap()
            .entries
            .is_empty()
    );
}

#[test]
fn animation_selection_budget_refuses_before_expansion_without_poisoning_edit() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let edit = rt
        .begin_edit(client, session, "bounded animation read", None)
        .unwrap();
    let query = ModelingQuery {
        motions: vec![id(2); 4097],
        ..Default::default()
    };
    assert!(
        rt.modeling_snapshot(client, session, Some(edit), query)
            .is_err()
    );
    rt.apply_modeling(
        client,
        edit,
        vec![ModelingCommand::Expression {
            intent: ExpressionEdit::Create {
                id: id(4),
                name: "Still usable".into(),
            },
        }],
    )
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    assert_eq!(rt.session_info(session).unwrap().history_lengths, (1, 0));
}
