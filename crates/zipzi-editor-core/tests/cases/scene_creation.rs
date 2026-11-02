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

#[test]
fn empty_rotation_uses_the_current_viewport_center_and_one_history_step() -> anyhow::Result<()> {
    use zipzi_editor_core::{PaletteProjection, ViewportMetrics, ViewportPoint};
    let mut editor = support::editor_from_document(Document::new_blank()?);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 2.,
    });
    let binding = editor.primary_viewport_binding().unwrap();
    editor.set_viewport_zoom(binding, 2., None)?;
    editor.pan_viewport(binding, ViewportPoint { x: 90., y: -60. })?;
    let plan = editor.plan_scene_object_creation_in(
        PaletteProjection::Organization,
        T::Rotation,
        "Rotation".into(),
        &[],
    )?;
    let result = editor.execute_scene_object_creation(binding.document, &plan)?;
    let object = &result.suggested_selection[0];
    let origin = |editor: &zipzi_editor_core::Editor| {
        editor
            .document()
            .unwrap()
            .object_index()
            .get(object)
            .unwrap()
            .properties["rotation"]["pose"]["origin"]
            .clone()
    };
    let expected = serde_json::json!({"x": -45., "y": -30.});
    assert_eq!(origin(&editor), expected);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(zipzi_editor_core::EditorCommand::Undo)?;
    assert!(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(object)
            .is_none()
    );
    editor.execute(zipzi_editor_core::EditorCommand::Redo)?;
    assert_eq!(origin(&editor), expected);
    Ok(())
}

#[test]
fn empty_rotation_retains_the_last_modeling_viewport_after_leave_and_selection_changes()
-> anyhow::Result<()> {
    use zipzi_editor_core::*;
    let mut editor = support::editor_from_document(Document::new_blank()?);
    let metrics = ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    };
    editor.set_canvas_metrics(metrics);
    let primary = editor.primary_viewport_binding().unwrap();
    editor.set_viewport_zoom(primary, 1., None)?;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(other, metrics);
    let binding = editor.viewport_binding(other).unwrap();
    editor.pan_viewport(binding, ViewportPoint { x: -500., y: 300. })?;
    let frame = editor.preview_for(binding)?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    editor.hover_canvas(binding, Some(ViewportPoint { x: 320., y: 240. }))?;
    editor.hover_canvas(binding, None)?;
    editor.cancel_gesture();
    for (hide, expected) in [(false, [500., 300.]), (true, [0., 0.])] {
        if hide {
            editor.set_viewport_visible(other, false);
        }
        let plan = editor.plan_scene_object_creation_in(
            PaletteProjection::Organization,
            T::Rotation,
            "Rotation".into(),
            &[],
        )?;
        let result = editor.execute_scene_object_creation(primary.document, &plan)?;
        let object = &result.suggested_selection[0];
        let index = editor.document().unwrap().object_index();
        assert_eq!(
            index.get(object).unwrap().properties["rotation"]["pose"]["origin"],
            serde_json::json!({"x": expected[0], "y": expected[1]})
        );
        editor.execute(EditorCommand::Undo)?;
    }
    Ok(())
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
    for document in [&d, &reopened] {
        use zipzi_editor_core::{PropertyValue, property::deformer as props};
        let values = document.property_values(&created[2])?;
        for (field, expected) in [
            (props::COLUMNS.key, 5),
            (props::ROWS.key, 5),
            (props::BEZIER_COLUMNS.key, 2),
            (props::BEZIER_ROWS.key, 2),
        ] {
            assert_eq!(values[&field], PropertyValue::Integer(expected));
        }
    }
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
