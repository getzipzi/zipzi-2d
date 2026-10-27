use zipzi_core::document::*;
use zipzi_sdk::AuthoringSession;

fn id(n: u32) -> String {
    format!("79620000-0000-4000-8000-{n:012x}")
}

#[test]
fn field_edits_then_undo_do_not_revive_animation_addresses() {
    let mut session = AuthoringSession::new(&id(1), zipzi_core::ModelSetup::default()).unwrap();
    session
        .edit("fixtures", None, |edit| {
            edit.edit_motion(MotionEdit::Create {
                id: id(2),
                name: "Motion".into(),
                registration_id: id(3),
                destination: MotionContainer::Root,
            })?;
            edit.edit_expression(ExpressionEdit::Create {
                id: id(4),
                name: "Expression".into(),
            })?;
            edit.edit_expression(ExpressionEdit::CreateEntry {
                owner: id(4),
                id: id(5),
                before: None,
            })?;
            edit.edit_physics(PhysicsEdit::CreateGroup {
                asset_id: id(6),
                id: id(7),
                name: "Physics".into(),
                before: None,
            })?;
            edit.edit_physics(PhysicsEdit::CreateInput {
                group_id: id(7),
                id: id(8),
                before: None,
            })?;
            edit.edit_pose(PoseEdit::CreateGroup {
                pose_id: id(9),
                id: id(10),
                name: "Pose".into(),
                before: None,
            })
        })
        .unwrap();

    let motion = session.motion_registration_address(&id(3)).unwrap();
    let mut entry = session
        .document()
        .get_motion_registration(&id(3))
        .unwrap()
        .1
        .clone();
    entry.fade_in = Some(0.5);
    let plan = session
        .plan_motion_registration_patch(&motion, entry.clone())
        .unwrap();
    session.execute_motion_plan(&plan).unwrap();
    session.undo().unwrap();
    assert_eq!(
        session
            .plan_motion_registration_patch(&motion, entry)
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );

    let expression = session.expression_entry_address(&id(5)).unwrap();
    let fields = ExpressionEntryFields {
        target: ExpressionTarget::Unspecified,
        value: 0.5,
        blend: None,
    };
    let plan = session
        .plan_expression_entry_patch(&expression, fields.clone())
        .unwrap();
    session.execute_expression_plan(&plan).unwrap();
    session.undo().unwrap();
    assert_eq!(
        session
            .plan_expression_entry_patch(&expression, fields)
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );

    let physics = session.physics_field_address(&id(8)).unwrap();
    let changes = vec![PhysicsProperty::InputWeight {
        id: id(8),
        value: 50.,
    }];
    let plan = session
        .plan_physics_field_patch(&physics, changes.clone())
        .unwrap();
    session.execute_physics_plan(&plan).unwrap();
    session.undo().unwrap();
    assert_eq!(
        session
            .plan_physics_field_patch(&physics, changes)
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );

    let pose = session.pose_field_address(&id(10)).unwrap();
    let fields = PoseFields::Group {
        name: "Changed".into(),
    };
    let plan = session
        .plan_pose_field_patch(&pose, fields.clone())
        .unwrap();
    session.execute_pose_plan(&plan).unwrap();
    session.undo().unwrap();
    assert_eq!(
        session
            .plan_pose_field_patch(&pose, fields)
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );

    // Root payload invalidation from an owned row must not retire FadeIn fields.
    let root = session.pose_field_address(&id(9)).unwrap();
    session
        .edit("owned entry", None, |edit| {
            edit.edit_pose(PoseEdit::CreateEntry {
                group_id: id(10),
                id: id(11),
                before: None,
            })
        })
        .unwrap();
    session
        .plan_pose_field_patch(
            &root,
            PoseFields::Asset {
                fade_in: Some(0.25),
            },
        )
        .unwrap();
    session.undo().unwrap();
    let plan = session
        .plan_pose_field_patch(
            &root,
            PoseFields::Asset {
                fade_in: Some(0.25),
            },
        )
        .unwrap();
    session.execute_pose_plan(&plan).unwrap();
    session.undo().unwrap();
    assert_eq!(
        session
            .plan_pose_field_patch(
                &root,
                PoseFields::Asset {
                    fade_in: Some(0.25)
                }
            )
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
}
