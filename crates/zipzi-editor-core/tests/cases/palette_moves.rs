use crate::support;
use zipzi_editor_core::{
    Document, ObjectKind as K, ObjectRef, PaletteMove, PalettePosition as P, PaletteProjection as V,
};

fn object(kind: K, n: u128) -> ObjectRef {
    ObjectRef::new(kind, uuid::Uuid::from_u128(n).to_string())
}
fn apply(
    d: &mut Document,
    projection: V,
    sources: Vec<ObjectRef>,
    target: Option<ObjectRef>,
    position: P,
) -> anyhow::Result<zipzi_editor_core::OperationResult> {
    let plan = d.plan_palette_move(PaletteMove {
        projection,
        sources,
        target,
        position,
    })?;
    d.execute_palette_move(&plan)
}

#[test]
fn nested_folder_move_and_cycle_rejection_use_one_atomic_history_entry() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parameter_groups()?;
    let group = object(K::ParameterGroup, 103);
    let target = object(K::ParameterGroup, 102);
    apply(
        &mut d,
        V::Parameters,
        vec![group.clone()],
        Some(target.clone()),
        P::Inside,
    )?;
    assert_eq!(
        d.parameter_group_draft(&group.id)?.parent_id,
        Some(target.id.clone())
    );
    assert_eq!(d.history_lengths(), (1, 0));
    let context = d.context().selections.clone();
    assert!(
        d.plan_palette_move(PaletteMove {
            projection: V::Parameters,
            sources: vec![object(K::ParameterGroup, 101)],
            target: Some(group.clone()),
            position: P::Inside,
        })
        .is_err()
    );
    assert_eq!(d.context().selections, context);
    assert_eq!(d.history_lengths(), (1, 0));
    d.undo()?;
    assert_eq!(d.parameter_group_draft(&group.id)?.parent_id, None);
    d.redo()?;
    assert_eq!(
        d.parameter_group_draft(&group.id)?.parent_id,
        Some(target.id)
    );
    Ok(())
}

#[test]
fn batch_moves_keep_source_order_and_preserve_the_inspection_target() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parameters(5)?;
    let parameters = d.parameters();
    let references: Vec<_> = parameters
        .iter()
        .map(|parameter| ObjectRef::new(K::Parameter, &parameter.id))
        .collect();
    d.navigate(references[0].clone())?;
    apply(
        &mut d,
        V::Parameters,
        vec![references[4].clone(), references[2].clone()],
        Some(references[1].clone()),
        P::Before,
    )?;
    assert_eq!(
        d.parameters().iter().map(|p| &p.id).collect::<Vec<_>>(),
        vec![
            &references[0].id,
            &references[2].id,
            &references[4].id,
            &references[1].id,
            &references[3].id
        ]
    );
    assert_eq!(d.context().inspected.as_ref(), Some(&references[0]));
    assert_eq!(d.history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn selected_ancestor_absorbs_descendants_and_locked_batch_is_rejected() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parts()?;
    let parent = object(K::Part, 101);
    let child = object(K::Part, 103);
    let target = object(K::Part, 102);
    apply(
        &mut d,
        V::Organization,
        vec![child.clone(), parent.clone()],
        Some(target.clone()),
        P::Inside,
    )?;
    assert_eq!(d.mesh_tree()[0].children[0].id, parent.id);
    assert_eq!(
        d.mesh_tree()[0].children[0]
            .children
            .iter()
            .filter(|node| node.id == child.id)
            .count(),
        1
    );
    d.undo()?;
    d.set_objects_locked(std::slice::from_ref(&child), true)?;
    let tree = d.mesh_tree();
    let history = d.history_lengths();
    assert!(
        d.plan_palette_move(PaletteMove {
            projection: V::Organization,
            sources: vec![parent, target.clone()],
            target: None,
            position: P::Inside,
        })
        .is_err()
    );
    assert_eq!(d.mesh_tree(), tree);
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn no_op_and_stale_plans_never_consume_redo() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parameters(3)?;
    let first = ObjectRef::new(K::Parameter, &d.parameters()[0].id);
    let last = ObjectRef::new(K::Parameter, &d.parameters()[2].id);
    let plan = d.plan_palette_step(V::Parameters, last.clone(), false)?;
    d.execute_palette_move(&plan)?;
    d.undo()?;
    let history = d.history_lengths();
    let noop = d.plan_palette_step(V::Parameters, first, false)?;
    assert!(noop.is_noop());
    assert!(!d.execute_palette_move(&noop)?.receipt.changed);
    assert_eq!(d.history_lengths(), history);
    // Even Undo back to the same content cannot revive an old SDK plan.
    assert!(d.execute_palette_move(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn steps_and_explicit_destinations_produce_the_same_native_order() -> anyhow::Result<()> {
    let (mut a, _assets_a) = support::sample_document_with_parameters(3)?;
    let (mut b, _assets_b) = support::sample_document_with_parameters(3)?;
    let middle = ObjectRef::new(K::Parameter, &a.parameters()[1].id);
    let first = ObjectRef::new(K::Parameter, &a.parameters()[0].id);
    let plan = a.plan_palette_step(V::Parameters, middle.clone(), false)?;
    a.execute_palette_move(&plan)?;
    apply(&mut b, V::Parameters, vec![middle], Some(first), P::Before)?;
    assert_eq!(a.parameters(), b.parameters());
    Ok(())
}

#[test]
fn destination_metadata_uses_typed_containers_in_native_preorder_without_history_changes()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parts()?;
    let before = d.history_lengths();
    let choices = d.palette_move_destinations(V::Organization)?;
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.path.as_str())
            .collect::<Vec<_>>(),
        ["", "Face", "Face / Eyes", "Body"]
    );
    assert!(choices[0].object.is_none());
    assert!(
        choices[1..]
            .iter()
            .all(|choice| choice.object.as_ref().unwrap().kind == K::Part)
    );
    assert_eq!(d.history_lengths(), before);
    assert!(
        d.palette_move_destinations(V::Deformation)?[0]
            .object
            .is_none()
    );
    let plan = d.plan_palette_move(PaletteMove {
        projection: V::Organization,
        sources: vec![object(K::Part, 103)],
        target: choices[3].object.clone(),
        position: P::Inside,
    })?;
    d.execute_palette_move(&plan)?;
    assert_eq!(
        d.palette_move_destinations(V::Organization)?
            .iter()
            .map(|choice| choice.path.as_str())
            .collect::<Vec<_>>(),
        ["", "Face", "Body", "Body / Eyes"]
    );
    d.undo()?;
    let history = d.history_lengths();
    assert_eq!(d.palette_move_destinations(V::Organization)?, choices);
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn parameter_destinations_include_empty_folders_and_cycle_rejection_stays_in_planner()
-> anyhow::Result<()> {
    let (d, _assets) = support::sample_document_with_parameter_groups()?;
    let choices = d.palette_move_destinations(V::Parameters)?;
    assert_eq!(choices.len(), d.parameter_groups().len() + 1);
    assert!(
        choices[1..]
            .iter()
            .all(|choice| choice.object.as_ref().unwrap().kind == K::ParameterGroup)
    );
    let folder = object(K::ParameterGroup, 101);
    assert!(
        d.plan_palette_move(PaletteMove {
            projection: V::Parameters,
            sources: vec![folder.clone()],
            target: Some(object(K::ParameterGroup, 102)),
            position: P::Inside,
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn equally_named_container_paths_retain_distinct_typed_identities() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let mut objects = Vec::new();
    for _ in 0..2 {
        let id = uuid::Uuid::new_v4().to_string();
        let plan = d.plan_organization_authoring(zipzi_sdk::OrganizationEdit::CreatePart {
            part: zipzi_core::Part {
                id: id.clone(),
                name: "Same".into(),
                ..Default::default()
            },
            before: None,
        })?;
        d.execute_organization_authoring(&plan)?;
        objects.push(ObjectRef::new(K::Part, id));
    }
    let choices = d.palette_move_destinations(V::Organization)?;
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[1].path, "Same");
    assert_eq!(choices[2].path, "Same");
    assert_eq!(choices[1].object.as_ref(), Some(&objects[0]));
    assert_eq!(choices[2].object.as_ref(), Some(&objects[1]));
    Ok(())
}
