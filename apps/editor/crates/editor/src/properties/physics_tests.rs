use super::*;
use editor_core::{EditorCommand, WorkspaceKind};
use form::{PropertyForm, PropertyForms};

fn fixture() -> (Editor, tempfile::TempDir) {
    let (document, assets) = crate::test_fixture::combined_animation_document().unwrap();
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation).unwrap();
    let asset = editor.document().unwrap().physics_definition().unwrap();
    let object = ObjectRef::new(ObjectKind::PhysicsGroup, asset.data.settings[0].id.clone());
    editor
        .inspect_animation_resource(editor.active_id().unwrap(), object)
        .unwrap();
    (editor, assets)
}
#[test]
fn physics_zero_fps_commits_through_the_form_and_supports_undo_redo() {
    let (mut editor, _assets) = fixture();
    let target = target(&editor).unwrap();
    let spec = physics::NumberField::Fps
        .spec(
            &editor
                .document()
                .unwrap()
                .physics_definition()
                .unwrap()
                .data
                .settings[0],
        )
        .unwrap();
    let original = target.values(&editor).unwrap()[&spec.key].clone();
    let mut form = PropertyForm::default();
    form.sync_definition(target.definition(&editor), target.values(&editor).unwrap());
    form.edit(&spec, "0".into());
    let patch = form.patch(ObjectKind::PhysicsGroup, spec.key).unwrap();
    let token = begin_edit(&mut editor, &target, &patch).unwrap();
    update_edit(&mut editor, &token, &patch).unwrap();
    assert_eq!(
        editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .meta
            .fps,
        Some(0.)
    );
    token.finish(&mut editor).unwrap();
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo).unwrap();
    assert_eq!(target.values(&editor).unwrap()[&spec.key], original);
    editor.execute(EditorCommand::Redo).unwrap();
    assert_eq!(
        target.values(&editor).unwrap()[&spec.key],
        PropertyValue::Float(0.)
    );
    form.sync_definition(target.definition(&editor), target.values(&editor).unwrap());
    form.edit(&spec, "-1".into());
    assert!(form.patch(ObjectKind::PhysicsGroup, spec.key).is_err());
}

#[test]
fn physics_uses_shared_form_baselines_and_one_owned_history_step() {
    let (mut editor, _assets) = fixture();
    let target = target(&editor).unwrap();
    let spec = physics::NumberField::InputWeight(0)
        .spec(
            &editor
                .document()
                .unwrap()
                .physics_definition()
                .unwrap()
                .data
                .settings[0],
        )
        .unwrap();
    let original = target.values(&editor).unwrap()[&spec.key].clone();
    let mut form = PropertyForm::default();
    form.sync_definition(target.definition(&editor), target.values(&editor).unwrap());
    form.edit(&spec, "40".into());
    let patch = form.patch(ObjectKind::PhysicsGroup, spec.key).unwrap();
    let token = begin_edit(&mut editor, &target, &patch).unwrap();
    for value in ["40", "60", "25"] {
        form.edit(&spec, value.into());
        let patch = form.patch(ObjectKind::PhysicsGroup, spec.key).unwrap();
        update_edit(&mut editor, &token, &patch).unwrap();
        assert_eq!(target.values(&editor).unwrap()[&spec.key], original);
        form.sync_definition(target.definition(&editor), target.values(&editor).unwrap());
        assert!(!form.fields[&spec.key].conflict);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert_eq!(
        editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .settings[0]
            .inputs[0]
            .weight,
        25.
    );
    token.finish(&mut editor).unwrap();
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo).unwrap();
    assert_eq!(target.values(&editor).unwrap()[&spec.key], original);
    assert!(begin_edit(&mut editor, &target, &patch).is_err());
    let target = super::target(&editor).unwrap();
    let replacement = begin_edit(&mut editor, &target, &patch).unwrap();
    token.cancel(&mut editor);
    assert!(replacement.owned(&editor));
    replacement.cancel(&mut editor);
}
#[test]
fn shared_form_commits_mapping_name_and_rejects_stale_or_invalid_physics_fields() {
    let (mut editor, _assets) = fixture();
    let target = target(&editor).unwrap();
    let mut forms = PropertyForms::default();
    forms.sync(
        &target,
        target.definition(&editor),
        target.values(&editor).unwrap(),
    );
    let name = physics::NAME;
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(&name, "  Hair  ".into());
    forms.commit(&mut editor, &target, name.key).unwrap();
    assert_eq!(
        target.values(&editor).unwrap()[&name.key],
        PropertyValue::Text("Hair".into())
    );
    let target = super::target(&editor).unwrap();
    let axis = target
        .definition(&editor)
        .fields
        .into_iter()
        .find(|s| {
            s.binding.key
                == physics::field_key(
                    "input_axis",
                    0,
                    &editor
                        .document()
                        .unwrap()
                        .physics_definition()
                        .unwrap()
                        .data
                        .settings[0],
                )
        })
        .unwrap()
        .binding;
    forms.sync(
        &target,
        target.definition(&editor),
        target.values(&editor).unwrap(),
    );
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(&axis, "Angle".into());
    forms.commit(&mut editor, &target, axis.key).unwrap();
    assert_eq!(
        target.values(&editor).unwrap()[&axis.key],
        PropertyValue::Text("Angle".into())
    );
    let target = super::target(&editor).unwrap();
    forms.sync(
        &target,
        target.definition(&editor),
        target.values(&editor).unwrap(),
    );
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(&axis, "unknown".into());
    assert!(forms.commit(&mut editor, &target, axis.key).is_err());
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    let node = physics::field_key(
        "output_node",
        0,
        &editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .settings[0],
    );
    let mut form = PropertyForm::default();
    form.sync_definition(target.definition(&editor), target.values(&editor).unwrap());
    let spec = target.definition(&editor).field(node).unwrap().binding;
    form.edit(&spec, "999".into());
    assert!(
        target
            .commit(
                &mut editor,
                form.patch(ObjectKind::PhysicsGroup, node).unwrap()
            )
            .is_err()
    );
    editor
        .inspect_timeline_selection(editor.active_id().unwrap())
        .unwrap();
    assert!(
        target
            .commit(
                &mut editor,
                form.patch(ObjectKind::PhysicsGroup, node).unwrap()
            )
            .is_err()
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
}
