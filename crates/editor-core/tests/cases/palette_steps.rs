use crate::support;
use editor_core::{
    Document, ObjectKind as K, ObjectRef, PaletteProjection as V, SceneObjectType as T,
};

fn object(kind: K, n: u128) -> ObjectRef {
    ObjectRef::new(kind, uuid::Uuid::from_u128(n).to_string())
}
fn parameters(d: &Document) -> Vec<String> {
    d.parameters().iter().map(|p| p.id.clone()).collect()
}
fn step(
    d: &mut Document,
    projection: V,
    objects: &[ObjectRef],
    down: bool,
) -> anyhow::Result<editor_core::OperationResult> {
    let plan = d.plan_palette_steps(projection, objects, down)?;
    d.execute_palette_move(&plan)
}
fn create(d: &mut Document, kind: T, owner: &[ObjectRef]) -> anyhow::Result<ObjectRef> {
    let plan = d.plan_scene_object_creation(kind, "Object".into(), owner)?;
    Ok(d.execute_scene_object_creation(&plan)?.suggested_selection[0].clone())
}

#[test]
fn disjoint_blocks_move_one_neighbor_without_compacting_and_share_one_history_entry()
-> anyhow::Result<()> {
    for (down, expected) in [
        (false, [2, 3, 1, 4, 6, 5, 7]),
        (true, [1, 4, 2, 3, 5, 7, 6]),
    ] {
        let (mut d, _assets) = support::sample_document_with_parameters(7)?;
        let sources = [
            object(K::Parameter, 6),
            object(K::Parameter, 3),
            object(K::Parameter, 2),
        ];
        d.select_objects(sources.to_vec())?;
        let before = parameters(&d);
        assert!(step(&mut d, V::Parameters, &sources, down)?.receipt.changed);
        assert_eq!(
            parameters(&d),
            expected.map(|id| object(K::Parameter, id).id)
        );
        assert_eq!(d.context().selections, sources);
        assert_eq!(d.context().inspected.as_ref(), sources.last());
        assert_eq!(d.history_lengths(), (1, 0));
        d.undo()?;
        assert_eq!(parameters(&d), before);
        d.redo()?;
        assert_eq!(
            parameters(&d),
            expected.map(|id| object(K::Parameter, id).id)
        );
        let directory = tempfile::tempdir()?;
        let saved = d.save(&directory.path().join("step.kasane"))?;
        assert_eq!(
            parameters(&Document::open(&saved.manifest)?),
            parameters(&d)
        );
    }
    Ok(())
}

#[test]
fn boundary_blocks_stay_in_place_and_all_selected_noop_preserves_redo() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parameters(7)?;
    let sources: Vec<_> = [1, 2, 5, 7].map(|id| object(K::Parameter, id)).into();
    step(&mut d, V::Parameters, &sources, false)?;
    assert_eq!(
        parameters(&d),
        [1, 2, 3, 5, 4, 7, 6].map(|id| object(K::Parameter, id).id)
    );
    d.undo()?;
    let all: Vec<_> = (1..=7).map(|id| object(K::Parameter, id)).collect();
    for down in [false, true] {
        let plan = d.plan_palette_steps(V::Parameters, &all, down)?;
        assert!(plan.is_noop());
        assert!(!d.execute_palette_move(&plan)?.receipt.changed);
        assert_eq!(d.history_lengths(), (0, 1));
    }
    Ok(())
}

#[test]
fn native_closure_absorbs_selected_descendants_before_stepping_a_part() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parts()?;
    let before_children = d.mesh_tree()[0].children.clone();
    let nested_mesh = before_children[0].children[0].reference().unwrap();
    let sources = [nested_mesh, object(K::Part, 103), object(K::Part, 101)];
    d.select_objects(sources.to_vec())?;
    step(&mut d, V::Organization, &sources, true)?;
    let tree = d.mesh_tree();
    assert_eq!(tree[0].id, object(K::Part, 102).id);
    assert_eq!(tree[1].children, before_children);
    assert_eq!(d.context().selections, sources);
    assert_eq!(d.history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn collection_steps_are_independent_and_locked_boundary_member_rejects_the_whole_batch()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parts()?;
    let body = object(K::Part, 102);
    let a = create(&mut d, T::Mesh, std::slice::from_ref(&body))?;
    let b = create(&mut d, T::Mesh, std::slice::from_ref(&body))?;
    let face_mesh = d.mesh_tree()[0].children[1].reference().unwrap();
    let sources = [face_mesh.clone(), b.clone()];
    step(&mut d, V::Organization, &sources, false)?;
    assert_eq!(d.mesh_tree()[0].children[0].id, face_mesh.id);
    assert_eq!(
        d.mesh_tree()[1]
            .children
            .iter()
            .map(|n| &n.id)
            .collect::<Vec<_>>(),
        [&b.id, &a.id]
    );
    assert_eq!(d.history_lengths(), (3, 0));
    d.undo()?;
    let face = object(K::Part, 101);
    d.set_objects_locked(std::slice::from_ref(&face), true)?;
    let before = d.mesh_tree();
    let history = d.history_lengths();
    let error = d
        .plan_palette_steps(V::Organization, &[face, b], false)
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<editor_core::PaletteMovePlanningError>()
            .unwrap()
            .code(),
        "OBJECT_LOCKED"
    );
    assert_eq!(d.mesh_tree(), before);
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn root_parts_and_unowned_objects_keep_their_distinct_display_buckets() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let a = create(&mut d, T::Part, &[])?;
    let b = create(&mut d, T::Part, &[])?;
    let m = create(&mut d, T::Mesh, &[])?;
    let n = create(&mut d, T::Mesh, &[])?;
    step(&mut d, V::Organization, &[b.clone(), n.clone()], false)?;
    let tree = d.mesh_tree();
    assert_eq!([&tree[0].id, &tree[1].id], [&b.id, &a.id]);
    let unowned = tree.iter().find(|n| n.id == "unowned").unwrap();
    assert_eq!(
        unowned.children.iter().map(|n| &n.id).collect::<Vec<_>>(),
        [&n.id, &m.id]
    );
    assert_eq!(d.history_lengths(), (5, 0));
    Ok(())
}

#[test]
fn deformation_siblings_step_without_changing_parents_and_mixed_parents_are_rejected()
-> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let a = create(&mut d, T::Warp, &[])?;
    let b = create(&mut d, T::Rotation, &[])?;
    let m = create(&mut d, T::Mesh, &[])?;
    let n = create(&mut d, T::Mesh, &[])?;
    step(&mut d, V::Deformation, &[b.clone(), n.clone()], false)?;
    let tree = d.deformer_tree();
    assert_eq!([&tree[0].id, &tree[1].id], [&b.id, &a.id]);
    assert_eq!(
        tree.last()
            .unwrap()
            .children
            .iter()
            .map(|n| &n.id)
            .collect::<Vec<_>>(),
        [&n.id, &m.id]
    );
    let child = uuid::Uuid::new_v4().to_string();
    let plan = d.plan_scene_authoring(kasane_sdk::SceneEdit::Create {
        id: child.clone(),
        name: "Child".into(),
        object: kasane_sdk::SceneCreate::Rotation,
        part_id: None,
        parent_id: Some(a.id.clone()),
        before: None,
    })?;
    d.execute_scene_authoring(&plan)?;
    let history = d.history_lengths();
    let error = d
        .plan_palette_steps(
            V::Deformation,
            &[a, ObjectRef::new(K::Deformer, child)],
            false,
        )
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<editor_core::PaletteMovePlanningError>()
            .unwrap()
            .code(),
        "PALETTE_DEFORMATION_SIBLINGS"
    );
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn stale_or_mistyped_step_plans_never_mutate_a_restored_document() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parameters(5)?;
    let sources = [object(K::Parameter, 2), object(K::Parameter, 4)];
    let plan = d.plan_palette_steps(V::Parameters, &sources, true)?;
    assert!(
        d.plan_palette_steps(V::Organization, &sources, true)
            .is_err()
    );
    assert!(
        d.plan_palette_steps(
            V::Parameters,
            &[sources[0].clone(), sources[0].clone()],
            true
        )
        .is_err()
    );
    step(&mut d, V::Parameters, &sources, false)?;
    d.undo()?;
    let before = parameters(&d);
    let history = d.history_lengths();
    assert!(d.execute_palette_move(&plan).is_err());
    assert_eq!(parameters(&d), before);
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn parameter_folders_and_parameter_collections_step_together_without_changing_memberships()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parameter_groups()?;
    let sources = [
        object(K::Parameter, 4),
        object(K::ParameterGroup, 103),
        object(K::Parameter, 2),
    ];
    d.select_objects(sources.to_vec())?;
    step(&mut d, V::Parameters, &sources, false)?;
    let folders = d.palette_move_destinations(V::Parameters)?;
    assert_eq!(
        folders[1].object.as_ref(),
        Some(&object(K::ParameterGroup, 103))
    );
    let eyes: Vec<_> = d
        .parameters()
        .into_iter()
        .filter(|p| {
            d.parameter_draft(&p.id).unwrap().group_id == Some(object(K::ParameterGroup, 102).id)
        })
        .map(|p| p.id)
        .collect();
    assert_eq!(
        eyes,
        [object(K::Parameter, 4).id, object(K::Parameter, 3).id]
    );
    assert_eq!(
        d.parameter_draft(&object(K::Parameter, 2).id)?.group_id,
        Some(object(K::ParameterGroup, 103).id)
    );
    assert_eq!(d.context().selections, sources);
    assert_eq!(d.history_lengths(), (1, 0));
    d.undo()?;
    assert_eq!(
        d.palette_move_destinations(V::Parameters)?[1]
            .object
            .as_ref(),
        Some(&object(K::ParameterGroup, 101))
    );
    Ok(())
}
