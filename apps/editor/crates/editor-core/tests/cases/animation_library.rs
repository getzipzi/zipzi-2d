use editor_core::{
    Document, ObjectKind as K, ObjectRef as R, PalettePosition as P, PropertyPatch,
    PropertyValue as V, property::animation_entry as fields,
};
fn create(d: &mut Document, kind: K, name: &str, at: Option<&R>) -> anyhow::Result<R> {
    let plan = d.plan_animation_resource_creation(kind, name.into(), at)?;
    Ok(d.execute_animation_library(&plan)?
        .suggested_selection
        .into_iter()
        .find(|r| r.kind == kind)
        .unwrap())
}
fn patch(
    d: &Document,
    object: &R,
    key: editor_core::PropertyKey,
    value: V,
) -> anyhow::Result<PropertyPatch> {
    Ok(PropertyPatch {
        scope: d.property_scope(object),
        expected: d.property_values(object)?,
        changes: [(key, value)].into(),
    })
}
#[test]
fn creation_respects_empty_groups_sibling_positions_and_single_history() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let group = create(&mut d, K::MotionGroup, "Group", None)?;
    assert!(d.motion_groups()[0].entries.is_empty());
    let a = create(&mut d, K::Motion, "Motion", Some(&group))?;
    let b = create(&mut d, K::Motion, "Motion", Some(&group))?;
    let registration = R::new(K::MotionRegistration, &d.motion_groups()[0].entries[0].id);
    let c = create(&mut d, K::Motion, "Motion", Some(&registration))?;
    assert_eq!(
        d.motion_groups()[0]
            .entries
            .iter()
            .map(|e| e.clip_id.as_str())
            .collect::<Vec<_>>(),
        vec![a.id.as_str(), c.id.as_str(), b.id.as_str()]
    );
    assert_eq!(d.motion_definition(&c.id).unwrap().name, "Motion 3");
    assert_eq!(d.history_lengths(), (4, 0));
    let registration_id = d.motion_groups()[0].entries[1].id.clone();
    d.undo()?;
    assert!(d.motion_definition(&c.id).is_none());
    d.redo()?;
    assert_eq!(d.motion_groups()[0].entries[1].id, registration_id);
    let second = create(&mut d, K::MotionGroup, "Group", Some(&registration))?;
    assert_eq!(d.motion_groups()[1].id, second.id);
    Ok(())
}
#[test]
fn registration_overrides_clear_to_inheritance_and_stale_addresses_reject_after_undo()
-> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let clip = create(&mut d, K::Motion, "Motion", None)?;
    let registration = R::new(
        K::MotionRegistration,
        d.motion_resources()[0].selection.entry.as_ref().unwrap(),
    );
    let plan =
        d.plan_motion_authoring(kasane_core::document::MotionEdit::DuplicateRegistrations {
            ids: vec![registration.id.clone()],
            destination: kasane_core::document::MotionContainer::Root,
            before: None,
        })?;
    d.execute_motion_authoring(&plan)?;
    let p = patch(
        &d,
        &registration,
        fields::REG_FADE_IN.key,
        V::OptionalFloat(Some(0.25)),
    )?;
    d.set_object_properties(&registration, &p)?;
    assert_eq!(d.motion_resources()[0].fade_in, 0.25);
    assert_eq!(d.motion_resources()[1].fade_in, 0.);
    assert_eq!(d.motion_definition(&clip.id).unwrap().fade_in, Some(0.));
    let stale = patch(
        &d,
        &registration,
        fields::REG_FADE_IN.key,
        V::OptionalFloat(Some(0.75)),
    )?;
    let group = create(&mut d, K::MotionGroup, "Group", None)?;
    let plan = d.plan_animation_resource_move(&registration, Some(&group), P::Inside)?;
    d.execute_animation_library(&plan)?;
    d.undo()?;
    assert!(d.set_object_properties(&registration, &stale).is_err());
    let p = patch(
        &d,
        &registration,
        fields::REG_FADE_IN.key,
        V::OptionalFloat(None),
    )?;
    d.set_object_properties(&registration, &p)?;
    assert_eq!(d.motion_resources()[0].fade_in, 0.);
    Ok(())
}
#[test]
fn expression_members_use_stable_identity_and_noop_preserves_redo() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let a = create(&mut d, K::Expression, "Expression", None)?;
    assert!(d.expression_definition(&a.id).unwrap().entries.is_empty());
    let x = create(&mut d, K::ExpressionEntry, "", Some(&a))?;
    let y = create(&mut d, K::ExpressionEntry, "", Some(&a))?;
    let stale = patch(&d, &x, fields::VALUE.key, V::Float(0.4))?;
    let plan = d.plan_animation_resource_step(&y, false)?;
    d.execute_animation_library(&plan)?;
    assert_eq!(d.expression_definition(&a.id).unwrap().entries[0].id, y.id);
    assert!(d.set_object_properties(&x, &stale).is_err());
    let p = patch(&d, &x, fields::VALUE.key, V::Float(0.4))?;
    d.set_object_properties(&x, &p)?;
    assert_eq!(d.expression_entry(&x.id).unwrap().1.value, 0.4);
    assert_eq!(d.expression_entry(&y.id).unwrap().1.value, 0.);
    d.undo()?;
    let history = d.history_lengths();
    let plan = d.plan_animation_resource_step(&y, false)?;
    let result = d.execute_animation_library(&plan)?;
    assert!(!result.receipt.changed);
    assert_eq!(d.history_lengths(), history);
    assert!(
        d.plan_animation_resource_creation(K::ExpressionEntry, "".into(), None)
            .is_err()
    );
    Ok(())
}

#[test]
fn resource_scoped_clip_and_expression_previews_commit_cancel_and_reject_stale_versions()
-> anyhow::Result<()> {
    use editor_core::{EditorCommand, PropertyScope, WorkspaceKind, property};
    let (doc, _assets) = crate::support::animation_document()?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_workspace(WorkspaceKind::Animation)?;
    let doc = editor.document().unwrap();
    let motion = R::new(K::Motion, &doc.motion_resources()[0].selection.clip_id);
    let expression = R::new(K::Expression, &doc.expression_resources()[0].id);
    for (object, key, value) in [
        (&motion, property::motion::DURATION.key, V::Float(3.)),
        (&motion, property::motion::FPS.key, V::Float(45.)),
        (
            &motion,
            property::motion::FADE_IN.key,
            V::OptionalFloat(Some(0.7)),
        ),
        (
            &expression,
            property::expression::FADE_OUT.key,
            V::OptionalFloat(Some(0.7)),
        ),
    ] {
        let doc = editor.document().unwrap();
        let original = doc.property_values(object)?;
        let history = doc.history_lengths();
        let mut changes = patch(doc, object, key, value.clone())?;
        // Resource inspectors capture a document version even for Clip/Expression fields.
        changes.scope = PropertyScope::Resource {
            revision: doc.model_generation(),
        };
        let context = editor.editing_context_token().unwrap();
        let token = editor.begin_property_edit(context, object.clone(), &changes)?;
        assert!(editor.property_edit_owned(&token));
        editor.update_property_edit(&token, &changes)?;
        assert!(editor.property_edit_owned(&token));
        assert_eq!(
            editor.document().unwrap().property_values(object)?,
            original
        );
        assert_eq!(editor.document().unwrap().history_lengths(), history);
        editor.cancel_property_edit(&token);
        assert!(!editor.document().unwrap().has_pending_edit());
        assert_eq!(
            editor.document().unwrap().property_values(object)?,
            original
        );
        assert_eq!(editor.document().unwrap().history_lengths(), history);

        let token = editor.begin_property_edit(context, object.clone(), &changes)?;
        editor.update_property_edit(&token, &changes)?;
        editor.finish_property_edit(&token)?;
        assert!(!editor.property_edit_owned(&token));
        assert_eq!(
            editor.document().unwrap().property_values(object)?[&key],
            value
        );
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(
            editor.document().unwrap().property_values(object)?,
            original
        );
        assert!(
            editor
                .begin_property_edit(context, object.clone(), &changes)
                .is_err()
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (history.0, 1));
        editor.execute(EditorCommand::Redo)?;
        assert_eq!(
            editor.document().unwrap().property_values(object)?[&key],
            value
        );
    }
    Ok(())
}

#[test]
fn expression_library_order_preserves_playback_but_member_order_invalidates_it()
-> anyhow::Result<()> {
    use editor_core::{EditorCommand, WorkspaceKind};
    let (mut doc, _assets) = crate::support::animation_document()?;
    let expression = R::new(K::Expression, &doc.expression_resources()[0].id);
    create(&mut doc, K::Expression, "Extra", None)?;
    let member = create(&mut doc, K::ExpressionEntry, "", Some(&expression))?;
    let mut editor = crate::support::editor_from_document(doc);
    editor.set_workspace(WorkspaceKind::Animation)?;
    let document = editor.active_id().unwrap();
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(document, motion.clone())?;
    editor.select_expression(document, expression.id.clone())?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let generation = editor.animation().unwrap().preview_generation;
    let triggers = editor.animation().unwrap().expressions.clone();
    let history = editor.document().unwrap().history_lengths();
    let assert_playback = |editor: &editor_core::Editor| {
        let animation = editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.time, 0.4);
        assert_eq!(animation.motion.as_ref(), Some(&motion));
        assert_eq!(animation.expression.as_ref(), Some(&expression.id));
        assert_eq!(animation.expressions, triggers);
        assert_eq!(animation.preview_generation, generation);
    };
    let plan = editor
        .document()
        .unwrap()
        .plan_animation_resource_step(&expression, true)?;
    let result = editor.execute_animation_library(document, &plan)?;
    assert!(result.receipt.changed);
    assert_ne!(
        editor.document().unwrap().expression_resources()[0].id,
        expression.id
    );
    assert_playback(&editor);
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (history.0 + 1, 0)
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().expression_resources()[0].id,
        expression.id
    );
    assert_playback(&editor);
    editor.execute(EditorCommand::Redo)?;
    assert_playback(&editor);

    // Member order affects expression evaluation and must still retire the running preview.
    let plan = editor
        .document()
        .unwrap()
        .plan_animation_resource_step(&member, false)?;
    let result = editor.execute_animation_library(document, &plan)?;
    assert!(result.receipt.changed);
    assert!(!editor.animation().unwrap().playing);
    assert!(editor.animation().unwrap().preview_generation > generation);
    Ok(())
}
