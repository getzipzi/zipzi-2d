use super::common::*;
use zipzi_core::document::{MotionContainer as C, MotionEdit as E};
use zipzi_editor_core::*;
#[test]
fn library_organization_keeps_registration_selection_clock_and_playback_run() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let document = editor.active_id().unwrap();
    let selection = editor.animation().unwrap().motion.clone().unwrap();
    let mut evaluator = AnimationEvaluator::default();
    evaluate(&mut editor, &mut evaluator)?;
    let generation = editor.animation().unwrap().preview_generation;
    let group = uuid::Uuid::new_v4().to_string();
    for intent in [
        E::CreateGroup {
            id: group.clone(),
            name: "Empty group".into(),
        },
        E::MoveRegistrations {
            ids: vec![selection.entry.clone().unwrap()],
            destination: C::Group(group.clone()),
            before: None,
        },
        E::RenameGroup {
            id: group.clone(),
            name: "Renamed".into(),
        },
        E::MoveRegistrations {
            ids: vec![selection.entry.clone().unwrap()],
            destination: C::Root,
            before: None,
        },
    ] {
        let plan = editor.plan_motion_authoring(document, intent)?;
        let r = editor.execute_motion_authoring(document, &plan)?;
        assert!(r.receipt.changed);
        assert_eq!(
            editor.animation().unwrap().motion.as_ref(),
            Some(&selection)
        );
        assert_eq!(editor.animation().unwrap().time, 0.4);
        assert!(editor.animation().unwrap().playing);
        assert_eq!(editor.animation().unwrap().preview_generation, generation);
    }
    editor.advance_animation_clock(0.6);
    let frame = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(frame.snapshot.time, 0.6);
    Ok(())
}
#[test]
fn deleting_active_registration_does_not_activate_another_shared_entry() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 0)?;
    let doc = editor.active_id().unwrap();
    let selection = editor.animation().unwrap().motion.clone().unwrap();
    let plan = editor.plan_motion_authoring(
        doc,
        E::DeleteRegistrations {
            ids: vec![selection.entry.unwrap()],
        },
    )?;
    editor.execute_motion_authoring(doc, &plan)?;
    assert!(editor.animation().unwrap().motion.is_none());
    assert!(!editor.animation().unwrap().playing);
    Ok(())
}

#[test]
fn shared_registration_changes_keep_playback_and_new_entries_can_be_selected() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let document = editor.active_id().unwrap();
    let selection = editor.animation().unwrap().motion.clone().unwrap();
    let mut evaluator = AnimationEvaluator::default();
    evaluate(&mut editor, &mut evaluator)?;
    let generation = editor.animation().unwrap().preview_generation;
    let plan = editor.plan_motion_authoring(
        document,
        E::DuplicateRegistrations {
            ids: vec![selection.entry.clone().unwrap()],
            destination: C::Root,
            before: None,
        },
    )?;
    let result = editor.execute_motion_authoring(document, &plan)?;
    let new_entry = result
        .receipt
        .changes
        .created
        .iter()
        .find(|e| e.kind == zipzi_sdk::ObjectKind::MotionRegistration)
        .unwrap()
        .id
        .clone();
    assert!(editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().preview_generation, generation);
    editor.select_motion(
        document,
        MotionSelection {
            clip_id: selection.clip_id,
            entry: Some(new_entry.clone()),
        },
    )?;
    evaluate(&mut editor, &mut evaluator)?;
    let plan = editor.plan_motion_authoring(
        document,
        E::DeleteRegistrations {
            ids: vec![new_entry],
        },
    )?;
    editor.execute_motion_authoring(document, &plan)?;
    assert!(editor.animation().unwrap().motion.is_none());
    assert!(!editor.animation().unwrap().playing);
    Ok(())
}

#[test]
fn root_sound_registration_keeps_unsupported_sound_feedback() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let document = editor.active_id().unwrap();
    let selection = editor
        .document()
        .unwrap()
        .motion_resources()
        .into_iter()
        .find(|r| r.sound.is_some())
        .unwrap()
        .selection;
    let count = editor
        .document()
        .unwrap()
        .animation_catalog()
        .diagnostics
        .iter()
        .filter(|d| matches!(d, AnimationDiagnostic::SoundUnsupported { .. }))
        .count();
    let plan = editor.plan_motion_authoring(
        document,
        E::MoveRegistrations {
            ids: vec![selection.entry.unwrap()],
            destination: C::Root,
            before: None,
        },
    )?;
    editor.execute_motion_authoring(document, &plan)?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .animation_catalog()
            .diagnostics
            .iter()
            .filter(|d| matches!(d, AnimationDiagnostic::SoundUnsupported { .. }))
            .count(),
        count
    );
    Ok(())
}

#[test]
fn group_rename_and_history_share_unrelated_object_index_rows() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let document = editor.active_id().unwrap();
    let before = editor.document().unwrap().object_index();
    let group = before
        .objects
        .iter()
        .find(|o| o.reference.kind == ObjectKind::MotionGroup)
        .unwrap()
        .reference
        .id
        .clone();
    let unrelated = ObjectRef::new(
        ObjectKind::Motion,
        editor.document().unwrap().motion_resources()[0]
            .selection
            .clip_id
            .clone(),
    );
    let plan = editor.plan_motion_authoring(
        document,
        E::RenameGroup {
            id: group.clone(),
            name: "Renamed".into(),
        },
    )?;
    editor.execute_motion_authoring(document, &plan)?;
    for name in ["Renamed", "Idle", "Renamed"] {
        let after = editor.document().unwrap().object_index();
        assert!(std::ptr::eq(
            before.get(&unrelated).unwrap(),
            after.get(&unrelated).unwrap()
        ));
        assert_eq!(
            after
                .get(&ObjectRef::new(ObjectKind::MotionGroup, &group))
                .unwrap()
                .name,
            name
        );
        if name == "Idle" {
            editor.execute(EditorCommand::Redo)?;
        } else {
            editor.execute(EditorCommand::Undo)?;
        }
    }
    Ok(())
}
