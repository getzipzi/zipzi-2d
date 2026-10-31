use super::common::*;
use zipzi_core::DeformerOriginalAction as Action;
use zipzi_editor_core::*;

fn act(editor: &mut Editor, object: &ObjectRef, action: Action) -> anyhow::Result<()> {
    let target = editor.deformer_original_target(object).unwrap();
    editor.execute(EditorCommand::DeformerOriginal { target, action })
}
#[test]
fn original_update_restore_auto_policy_and_history_follow_the_current_editable_shape()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let created = control_points(&mut editor, &object)?;
    let adjust = |editor: &mut Editor| -> anyhow::Result<()> {
        let target = editor.warp_transform_target(&object).unwrap();
        editor.execute(EditorCommand::AdjustWarp {
            target,
            adjustment: DeformerAdjustment {
                translation: ModelVector { x: 9., y: 3. },
                angle: 0.,
                scale: 1.,
            },
        })
    };
    adjust(&mut editor)?;
    let manual = control_points(&mut editor, &object)?;
    act(&mut editor, &object, Action::Update)?;
    assert_eq!(
        editor.document().unwrap().property_values(&object)?
            [&property::deformer::ORIGINAL_AUTO.key],
        PropertyValue::Bool(false)
    );
    assert_eq!(
        editor
            .document()
            .unwrap()
            .deformer_original_info(&object)
            .unwrap()
            .source,
        Some(zipzi_core::DeformerOriginalSource::Manual)
    );
    adjust(&mut editor)?;
    assert_ne!(control_points(&mut editor, &object)?, manual);
    act(
        &mut editor,
        &object,
        Action::Restore {
            allow_resize: false,
        },
    )?;
    assert_eq!(control_points(&mut editor, &object)?, manual);
    assert_ne!(manual, created);
    assert_eq!(editor.document().unwrap().history_lengths(), (4, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_ne!(control_points(&mut editor, &object)?, manual);
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(control_points(&mut editor, &object)?, manual);
    Ok(())
}
#[test]
fn original_restore_information_precedes_shared_resampling_and_stale_targets_do_not_revive_after_undo()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let stale = editor.deformer_original_target(&object).unwrap();
    editor.execute(EditorCommand::SetObjectProperty {
        object: object.clone(),
        field: "warp.rows".into(),
        value: serde_json::json!(3),
    })?;
    let info = editor
        .document()
        .unwrap()
        .deformer_original_info(&object)
        .unwrap();
    assert!(info.requires_conversion());
    assert_eq!(info.conversion_grid, Some(([1, 3], [1, 1])));
    let before = control_points(&mut editor, &object)?;
    assert!(
        act(
            &mut editor,
            &object,
            Action::Restore {
                allow_resize: false
            }
        )
        .is_err()
    );
    assert_eq!(control_points(&mut editor, &object)?, before);
    act(&mut editor, &object, Action::Restore { allow_resize: true })?;
    assert_eq!(control_points(&mut editor, &object)?.len(), 4);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert!(
        editor
            .execute(EditorCommand::DeformerOriginal {
                target: stale,
                action: Action::Update
            })
            .is_err()
    );
    Ok(())
}
