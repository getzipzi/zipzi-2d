use crate::support;
use zipzi_editor_core::{Document, ObjectKind, ObjectRef, SceneObjectType as T, TreeNode};

fn create(d: &mut Document, kind: T, selection: &[ObjectRef]) -> anyhow::Result<ObjectRef> {
    let plan = d.plan_scene_object_creation(kind, "Created".into(), selection)?;
    let result = d.execute_scene_object_creation(&plan)?;
    assert!(result.receipt.changed);
    let object = result.suggested_selection[0].clone();
    assert_eq!(object.kind, kind.object_kind());
    Ok(object)
}
fn node<'a>(nodes: &'a [TreeNode], id: &str) -> Option<&'a TreeNode> {
    nodes.iter().find_map(|n| {
        if n.id == id {
            Some(n)
        } else {
            node(&n.children, id)
        }
    })
}
fn children(d: &Document, id: &str) -> Vec<String> {
    node(&d.part_tree(), id)
        .unwrap()
        .children
        .iter()
        .map(|n| n.id.clone())
        .collect()
}

#[test]
fn all_scene_defaults_roundtrip_with_one_creation_history_entry_each() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let part = create(&mut d, T::Part, &[])?;
    let mut created = vec![part.clone()];
    for kind in [T::Mesh, T::Warp, T::Rotation, T::Offscreen] {
        created.push(create(&mut d, kind, std::slice::from_ref(&part))?);
    }
    assert_eq!(d.history_lengths(), (5, 0));
    let index = d.object_index();
    let runtime_ids: std::collections::HashSet<_> = created
        .iter()
        .map(|object| {
            uuid::Uuid::parse_str(&object.id).unwrap();
            index.get(object).unwrap().properties["runtime_id"]
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(runtime_ids.len(), created.len());
    assert!(!runtime_ids.contains(""));
    assert_eq!(
        children(&d, &part.id),
        created[1..]
            .iter()
            .map(|r| r.id.clone())
            .collect::<Vec<_>>()
    );
    let root = tempfile::tempdir()?;
    let saved = d.save(&root.path().join("creation.zipzi"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(reopened.part_tree(), d.part_tree());
    assert_eq!(reopened.deformer_tree(), d.deformer_tree());
    let before = d.part_tree();
    d.undo()?;
    assert!(node(&d.part_tree(), &created[4].id).is_none());
    d.redo()?;
    assert_eq!(d.part_tree(), before);
    Ok(())
}

#[test]
fn organization_creation_keeps_deformation_independent_and_names_unique_in_owner()
-> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let part = create(&mut d, T::Part, &[])?;
    let warp = create(&mut d, T::Warp, std::slice::from_ref(&part))?;
    let mesh = create(&mut d, T::Mesh, std::slice::from_ref(&warp))?;
    let index = d.object_index();
    let properties = &index.get(&mesh).unwrap().properties;
    assert_eq!(properties["part_id"], part.id);
    assert_eq!(properties["deformer_id"], "");
    assert!(
        node(&d.deformer_tree(), &warp.id)
            .unwrap()
            .children
            .is_empty()
    );
    assert_eq!(d.object_index().name(&warp), "Created");
    assert_eq!(d.object_index().name(&mesh), "Created 2");
    let other_part = create(&mut d, T::Part, &[])?;
    let other_mesh = create(&mut d, T::Mesh, std::slice::from_ref(&other_part))?;
    assert_eq!(d.object_index().name(&other_mesh), "Created");
    Ok(())
}

#[test]
fn ordinary_and_same_owner_selection_insert_after_last_in_full_order() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let part = create(&mut d, T::Part, &[])?;
    let a = create(&mut d, T::Mesh, std::slice::from_ref(&part))?;
    let b = create(&mut d, T::Warp, std::slice::from_ref(&part))?;
    let c = create(&mut d, T::Rotation, std::slice::from_ref(&part))?;
    let inserted = create(&mut d, T::Mesh, std::slice::from_ref(&a))?;
    assert_eq!(
        children(&d, &part.id),
        [
            a.id.clone(),
            inserted.id.clone(),
            b.id.clone(),
            c.id.clone()
        ]
    );
    let multi = create(&mut d, T::Part, &[b.clone(), a.clone()])?;
    assert_eq!(
        children(&d, &part.id),
        [a.id, inserted.id, b.id, multi.id, c.id]
    );
    Ok(())
}

#[test]
fn single_part_creates_inside_cross_owner_selection_falls_back_to_root() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let a = create(&mut d, T::Part, &[])?;
    let b = create(&mut d, T::Part, &[])?;
    let nested = create(&mut d, T::Part, std::slice::from_ref(&a))?;
    let mesh_a = create(&mut d, T::Mesh, std::slice::from_ref(&nested))?;
    let mesh_b = create(&mut d, T::Mesh, std::slice::from_ref(&b))?;
    let root = create(&mut d, T::Mesh, &[mesh_a, mesh_b])?;
    assert_eq!(children(&d, &a.id), [nested.id]);
    assert_eq!(children(&d, "unowned"), [root.id]);
    Ok(())
}

#[test]
fn unavailable_or_stale_creation_preserves_context_history_and_redo() -> anyhow::Result<()> {
    let (mut d, _assets) = support::sample_document_with_parts()?;
    let part = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(103).to_string());
    d.navigate(part.clone())?;
    let plan =
        d.plan_scene_object_creation(T::Mesh, "Stale".into(), std::slice::from_ref(&part))?;
    d.set_objects_locked(std::slice::from_ref(&part), true)?;
    let history = d.history_lengths();
    let context = d.context().clone();
    assert!(
        d.plan_scene_object_creation(T::Part, "Locked".into(), std::slice::from_ref(&part))
            .is_err()
    );
    assert!(d.execute_scene_object_creation(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    assert_eq!(d.context().selections, context.selections);
    d.undo()?;
    let history = d.history_lengths();
    assert!(d.execute_scene_object_creation(&plan).is_err());
    assert!(
        d.plan_scene_object_creation(T::Offscreen, "Invalid".into(), &[])
            .is_err()
    );
    assert_eq!(d.history_lengths(), history);
    assert!(
        d.plan_scene_object_creation(
            T::Mesh,
            "Missing".into(),
            &[ObjectRef::new(
                ObjectKind::Mesh,
                uuid::Uuid::new_v4().to_string()
            )]
        )
        .is_err()
    );
    Ok(())
}
