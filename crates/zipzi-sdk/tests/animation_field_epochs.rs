use zipzi_core::document::*;
use zipzi_sdk::AuthoringSession;

fn id(n: u32) -> String {
    format!("79620000-0000-4000-8000-{n:012x}")
}

#[test]
fn animation_field_edits_then_undo_do_not_revive_draft_sources() {
    let mut session = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (zipzi_core::ModelSetup::default()).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
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
            .and_then(|value| {
                let prepared = draft.prepare("fixtures")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();

    for domain in 0..4 {
        let mut stale = session.begin_draft();
        let apply = |c: &mut zipzi_sdk::Commands<'_>| -> Result<(), zipzi_sdk::Status> {
            match domain {
                0 => c.patch_property(zipzi_core::state::PropertyPatch::Motion {
                    id: id(2),
                    duration: 1.,
                    fps: 30.,
                    fade_in: Some(0.5),
                    fade_out: None,
                }),
                1 => c.patch_property(zipzi_core::state::PropertyPatch::Expression {
                    id: id(4),
                    fade_in: Some(0.5),
                    fade_out: None,
                }),
                2 => {
                    let mut asset = c
                        .read_document()?
                        .physics()
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .clone();
                    asset.data.meta.fps = Some(60.);
                    c.replace_physics(Some(asset))
                }
                _ => c.edit_pose(PoseEdit::Patch {
                    id: id(10),
                    expected: PoseFields::Group {
                        name: "Pose".into(),
                    },
                    fields: PoseFields::Group {
                        name: "Changed".into(),
                    },
                }),
            }
        };
        stale.batch(stale.source(), apply).unwrap();
        let mut current = session.begin_draft();
        current.batch(current.source(), apply).unwrap();
        session
            .publish(current.prepare("animation field").unwrap())
            .unwrap();
        session.undo(session.version()).unwrap();
        assert_eq!(
            session
                .publish(stale.prepare("stale animation field").unwrap())
                .unwrap_err()
                .code,
            "STALE_SOURCE"
        );
    }
}
