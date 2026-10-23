//! Shared-form acceptance for transient animation selections.
use super::*;
use editor_core::{EditorCommand, TimelineCommand, WorkspaceKind};
use form::{PropertyForm, PropertyForms};

fn fixture() -> (Editor, tempfile::TempDir) {
    let (document, assets) = crate::test_fixture::animation_document().unwrap();
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation).unwrap();
    let document = editor.active_id().unwrap();
    let motion = editor.document().unwrap().animation_catalog().motions[0]
        .selection
        .clone();
    editor.select_motion(document, motion).unwrap();
    (editor, assets)
}
fn select_key(editor: &mut Editor) -> editor_core::TimelineKeyRef {
    let projection = editor.timeline_projection().unwrap();
    let key = projection.tracks.values().next().unwrap().keys[0]
        .reference
        .clone();
    editor
        .reveal_timeline_keys(editor.active_id().unwrap(), std::slice::from_ref(&key))
        .unwrap();
    key
}

#[test]
fn explicit_resource_inspection_preserves_timeline_selection_and_time_navigation() {
    let (mut editor, _assets) = fixture();
    let document = editor.active_id().unwrap();
    let key = select_key(&mut editor);
    assert!(matches!(
        target(&editor).unwrap().timeline,
        Some(timeline::TimelineTarget::Keys(_))
    ));
    let motion = ObjectRef::new(ObjectKind::Motion, &key.clip_id);
    editor
        .inspect_animation_resource(document, motion.clone())
        .unwrap();
    assert!(target(&editor).unwrap().timeline.is_none());
    assert!(editor.timeline_view().unwrap().keys.contains(&key));
    editor.seek_timeline(document, 0.5).unwrap();
    assert!(target(&editor).unwrap().timeline.is_none());
    assert_eq!(target(&editor).unwrap().object, motion);
    editor.inspect_timeline_selection(document).unwrap();
    assert!(matches!(
        target(&editor).unwrap().timeline,
        Some(timeline::TimelineTarget::Keys(_))
    ));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
}

#[test]
fn shared_form_previews_selection_values_and_rejects_stale_targets_and_releases() {
    let (mut editor, _assets) = fixture();
    let key = select_key(&mut editor);
    let target = target(&editor).unwrap();
    let original = target.values(&editor).unwrap()[&timeline::VALUE.key].clone();
    let mut form = PropertyForm::default();
    form.sync_definition(target.definition(&editor), target.values(&editor).unwrap());
    form.set_value(&timeline::VALUE, PropertyValue::Float(0.25));
    let patch = form.patch(ObjectKind::Motion, timeline::VALUE.key).unwrap();
    let token = begin_edit(&mut editor, &target, &patch).unwrap();
    update_edit(&mut editor, &token, &patch).unwrap();
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key)
            .unwrap()
            .value,
        0.25
    );
    assert_eq!(
        target.values(&editor).unwrap()[&timeline::VALUE.key],
        original
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    token.finish(&mut editor).unwrap();
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo).unwrap();
    let replacement = begin_edit(&mut editor, &target, &patch).unwrap();
    token.cancel(&mut editor);
    assert!(replacement.owned(&editor));
    replacement.cancel(&mut editor);
    editor
        .inspect_animation_resource(editor.active_id().unwrap(), target.object.clone())
        .unwrap();
    assert!(target.commit(&mut editor, patch).is_err());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
}

#[test]
fn duration_is_an_ordinary_property_and_does_not_retime_keys() {
    let (mut editor, _assets) = fixture();
    let original = editor.timeline_projection().unwrap();
    let key = original
        .tracks
        .values()
        .next()
        .unwrap()
        .keys
        .last()
        .unwrap()
        .reference
        .clone();
    let time = original.key(&key).unwrap().time;
    let target = target(&editor).unwrap();
    let mut forms = PropertyForms::default();
    forms.sync(
        &target,
        target.definition(&editor),
        target.values(&editor).unwrap(),
    );
    forms.entries.get_mut(&target).unwrap().set_value(
        &editor_core::property::motion::DURATION,
        PropertyValue::Float(3.),
    );
    forms
        .commit(
            &mut editor,
            &target,
            editor_core::property::motion::DURATION.key,
        )
        .unwrap();
    assert_eq!(editor.timeline_projection().unwrap().duration, 3.);
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key)
            .unwrap()
            .time,
        time
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let document = editor.active_id().unwrap();
    editor
        .execute_timeline(document, TimelineCommand::ScaleTime(6.))
        .unwrap();
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key)
            .unwrap()
            .time,
        time * 2.
    );
}
