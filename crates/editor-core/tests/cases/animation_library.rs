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
