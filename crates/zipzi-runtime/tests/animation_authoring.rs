use zipzi_contract::{
    authoring::ObjectKind,
    kernel_commands::{Command, ObjectAddress},
    kernel_wire::Query,
};
use zipzi_core::document::{ExpressionEdit, MotionContainer, MotionEdit, PhysicsEdit, PoseEdit};
use zipzi_runtime::{QuerySource, Runtime};

fn id(n: u32) -> String {
    format!("79610000-0000-4000-8000-{n:012x}")
}

fn address(kind: ObjectKind, n: u32) -> ObjectAddress {
    ObjectAddress { kind, id: id(n) }
}

#[test]
fn animation_commands_share_candidate_publication_and_source_bound_reads() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let before = rt.subscribe(client, session).unwrap();
    let QuerySource::Published(version) = before.snapshot.source else {
        panic!("expected published source")
    };
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![
                Command::Motion {
                    edit: MotionEdit::Create {
                        id: id(2),
                        name: "Motion".into(),
                        registration_id: id(3),
                        destination: MotionContainer::Root,
                    },
                },
                Command::Expression {
                    edit: ExpressionEdit::Create {
                        id: id(4),
                        name: "Expression".into(),
                    },
                },
                Command::Physics {
                    edit: PhysicsEdit::CreateGroup {
                        asset_id: id(5),
                        id: id(6),
                        name: "Physics".into(),
                        before: None,
                    },
                },
                Command::Pose {
                    edit: PoseEdit::CreateGroup {
                        pose_id: id(7),
                        id: id(8),
                        name: "Pose".into(),
                        before: None,
                    },
                },
            ],
        )
        .unwrap();
    let query = Query::Objects {
        addresses: vec![
            address(ObjectKind::Motion, 2),
            address(ObjectKind::MotionRegistration, 3),
            address(ObjectKind::Expression, 4),
            address(ObjectKind::Physics, 5),
            address(ObjectKind::Pose, 7),
            address(ObjectKind::Motion, 2),
        ],
    };
    let selected = rt
        .query(client, QuerySource::Draft(draft), query.clone())
        .unwrap();
    assert_eq!(
        selected["source"],
        serde_json::to_value(QuerySource::Draft(draft)).unwrap()
    );
    assert_eq!(draft.sequence, 1);
    let records = selected["value"]["records"].as_array().unwrap();
    assert_eq!(records[0]["value"]["name"], "Motion");
    assert_eq!(records[1]["value"]["clip_id"], id(2));
    assert_eq!(records[3]["value"]["id"], id(5));
    assert_eq!(records[4]["value"]["id"], id(7));
    assert_eq!(records[0], records[5]);
    assert_eq!(
        rt.query(
            client,
            QuerySource::Draft(draft),
            Query::Objects {
                addresses: vec![address(ObjectKind::Motion, 99)]
            }
        )
        .unwrap_err()
        .code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        rt.query(client, QuerySource::Published(version), query)
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    let frozen = rt.read_draft(client, draft).unwrap();
    let first = rt.publish(client, draft, "Animation").unwrap();
    assert_eq!(first.history.undo_steps, 1);
    let draft = rt.begin_draft(client, first.version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![Command::Expression {
                edit: ExpressionEdit::CreateEntry {
                    owner: id(4),
                    id: id(9),
                    before: None,
                },
            }],
        )
        .unwrap();
    let second = rt.publish(client, draft, "Expression entry").unwrap();
    let entries = |host: &Runtime, version| {
        host.query(
            client,
            QuerySource::Published(version),
            Query::Objects {
                addresses: vec![address(ObjectKind::Expression, 4)],
            },
        )
        .unwrap()["value"]["records"][0]["value"]["entries"]
            .clone()
    };
    assert_eq!(entries(&rt, second.version)[0]["id"], id(9));
    let undo = rt.undo(client, second.version).unwrap();
    assert_eq!(entries(&rt, undo.version), serde_json::json!([]));
    let redo = rt.redo(client, undo.version).unwrap();
    assert_eq!(entries(&rt, redo.version)[0]["id"], id(9));
    assert!(frozen
        .snapshot
        .document()
        .expression(&id(4))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn animation_query_limit_refuses_before_expansion_and_preserves_draft() {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let QuerySource::Published(version) = rt.read_snapshot(client, session).unwrap().source else {
        panic!("expected published source")
    };
    let draft = rt.begin_draft(client, version).unwrap();
    let query = Query::Objects {
        addresses: (0..257).map(|_| address(ObjectKind::Motion, 2)).collect(),
    };
    assert_eq!(
        rt.query(client, QuerySource::Draft(draft), query)
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![Command::Expression {
                edit: ExpressionEdit::Create {
                    id: id(4),
                    name: "Still usable".into(),
                },
            }],
        )
        .unwrap();
    assert_eq!(
        rt.publish(client, draft, "Expression")
            .unwrap()
            .history
            .undo_steps,
        1
    );
}
