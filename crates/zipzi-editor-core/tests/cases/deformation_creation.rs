use zipzi_editor_core::{
    Document, ObjectKind as K, ObjectRef, PaletteProjection as P, SceneObjectType as T, TreeNode,
};

fn create(d: &mut Document, projection: P, kind: T, selected: &[ObjectRef]) -> ObjectRef {
    let plan = d
        .plan_scene_object_creation_in(projection, kind, "Created".into(), selected)
        .unwrap();
    assert_eq!(plan.projection(), projection);
    d.execute_scene_object_creation(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone()
}
fn mesh(d: &mut Document, part: &ObjectRef, parent: &ObjectRef) -> ObjectRef {
    let plan = d
        .plan_scene_authoring(zipzi_sdk::SceneEdit::Create {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Member".into(),
            object: zipzi_sdk::SceneCreate::Mesh {
                texture: Default::default(),
            },
            part_id: Some(part.id.clone()),
            parent_id: Some(parent.id.clone()),
            before: None,
            deformation_before: None,
        })
        .unwrap();
    d.execute_scene_authoring(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone()
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
fn children(d: &Document, deform: bool, id: &str) -> Vec<String> {
    let tree = if deform {
        d.deformer_tree()
    } else {
        d.mesh_tree()
    };
    node(&tree, id)
        .unwrap()
        .children
        .iter()
        .map(|n| n.id.clone())
        .collect()
}

#[test]
fn creation_inside_deformer_and_after_mesh_keep_organization_and_deformation_independent() {
    let mut d = Document::new_blank().unwrap();
    let p = create(&mut d, P::Organization, T::Part, &[]);
    let q = create(&mut d, P::Organization, T::Part, &[]);
    let parent = create(
        &mut d,
        P::Organization,
        T::Rotation,
        std::slice::from_ref(&p),
    );
    let a = mesh(&mut d, &q, &parent);
    let b = mesh(&mut d, &q, &parent);
    let history = d.history_lengths();
    let inserted = create(&mut d, P::Deformation, T::Warp, std::slice::from_ref(&a));
    assert_eq!(
        children(&d, true, &parent.id),
        [a.id.clone(), inserted.id.clone(), b.id.clone()]
    );
    assert_eq!(
        children(&d, false, &q.id),
        [a.id.clone(), b.id.clone(), inserted.id.clone()]
    );
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    let inside = create(
        &mut d,
        P::Deformation,
        T::Mesh,
        std::slice::from_ref(&inserted),
    );
    assert_eq!(children(&d, true, &inserted.id), [inside.id.clone()]);
    assert!(children(&d, false, &q.id).contains(&inside.id));
    d.undo().unwrap();
    assert!(d.object_index().get(&inside).is_none());
    d.redo().unwrap();
    assert_eq!(children(&d, true, &inserted.id), [inside.id]);
}

#[test]
fn multi_selection_uses_last_full_sibling_and_different_parents_fall_back_to_roots() {
    let mut d = Document::new_blank().unwrap();
    let p = create(&mut d, P::Organization, T::Part, &[]);
    let parent = create(
        &mut d,
        P::Organization,
        T::Rotation,
        std::slice::from_ref(&p),
    );
    let a = mesh(&mut d, &p, &parent);
    let b = mesh(&mut d, &p, &parent);
    let c = mesh(&mut d, &p, &parent);
    let inserted = create(&mut d, P::Deformation, T::Rotation, &[b.clone(), a.clone()]);
    assert_eq!(
        children(&d, true, &parent.id),
        [a.id.clone(), b.id.clone(), inserted.id, c.id]
    );
    let root = create(&mut d, P::Deformation, T::Mesh, &[parent, a]);
    let tree = d.deformer_tree();
    assert!(
        tree.iter()
            .any(|n| n.children.iter().any(|n| n.id == root.id) && n.reference().is_none())
    );
    assert_eq!(
        d.object_index().get(&root).unwrap().properties["part_id"],
        ""
    );
    assert_eq!(
        d.object_index().get(&root).unwrap().properties["deformer_id"],
        ""
    );
}

#[test]
fn mixed_organization_owners_do_not_change_the_shared_deformation_parent() {
    let mut d = Document::new_blank().unwrap();
    let p = create(&mut d, P::Organization, T::Part, &[]);
    let q = create(&mut d, P::Organization, T::Part, &[]);
    let parent = create(&mut d, P::Organization, T::Warp, std::slice::from_ref(&p));
    let a = mesh(&mut d, &p, &parent);
    let b = mesh(&mut d, &q, &parent);
    let child = create(&mut d, P::Deformation, T::Mesh, &[b.clone(), a.clone()]);
    assert_eq!(
        children(&d, true, &parent.id),
        [a.id, b.id, child.id.clone()]
    );
    assert_eq!(
        d.object_index().get(&child).unwrap().properties["part_id"],
        ""
    );
    assert_eq!(
        d.object_index().get(&child).unwrap().properties["deformer_id"],
        parent.id
    );
    let dir = tempfile::tempdir().unwrap();
    let saved = d.save(&dir.path().join("deformation.zipzi")).unwrap();
    let reopened = Document::open(&saved.manifest).unwrap();
    assert_eq!(reopened.deformer_tree(), d.deformer_tree());
    assert_eq!(reopened.mesh_tree(), d.mesh_tree());
}

#[test]
fn root_deformers_and_meshes_use_their_own_complete_display_buckets() {
    let mut d = Document::new_blank().unwrap();
    let a = create(&mut d, P::Organization, T::Rotation, &[]);
    let b = create(&mut d, P::Organization, T::Warp, &[]);
    let mesh = create(&mut d, P::Organization, T::Mesh, &[]);
    let next = create(&mut d, P::Deformation, T::Mesh, &[a.clone(), b.clone()]);
    let deform = create(&mut d, P::Deformation, T::Rotation, &[a.clone(), b.clone()]);
    let roots = d.deformer_tree();
    let ids: Vec<_> = roots
        .iter()
        .filter(|n| n.reference().is_some())
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(ids, [a.id, b.id, deform.id]);
    let group = roots.iter().find(|n| n.reference().is_none()).unwrap();
    assert_eq!(
        group
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
        [mesh.id, next.id]
    );
}

#[test]
fn unavailable_or_stale_creation_preserves_redo_and_never_creates_unsupported_types() {
    let mut d = Document::new_blank().unwrap();
    let part = create(&mut d, P::Organization, T::Part, &[]);
    let parent = create(
        &mut d,
        P::Organization,
        T::Warp,
        std::slice::from_ref(&part),
    );
    for kind in [T::Part, T::Offscreen] {
        assert!(
            d.plan_scene_object_creation_in(P::Deformation, kind, "No".into(), &[])
                .is_err()
        );
    }
    assert!(
        d.plan_scene_object_creation_in(
            P::Deformation,
            T::Mesh,
            "No".into(),
            std::slice::from_ref(&part)
        )
        .is_err()
    );
    d.set_objects_locked(std::slice::from_ref(&part), true)
        .unwrap();
    assert!(
        d.validate_scene_object_creation_in(P::Deformation, T::Mesh, std::slice::from_ref(&parent))
            .is_err()
    );
    assert!(
        d.plan_scene_object_creation_in(
            P::Deformation,
            T::Mesh,
            "No".into(),
            std::slice::from_ref(&parent)
        )
        .is_err()
    );
    d.undo().unwrap();
    let plan = d
        .plan_scene_object_creation_in(
            P::Deformation,
            T::Mesh,
            "Stale".into(),
            std::slice::from_ref(&parent),
        )
        .unwrap();
    let x = create(
        &mut d,
        P::Deformation,
        T::Mesh,
        std::slice::from_ref(&parent),
    );
    d.undo().unwrap();
    let history = d.history_lengths();
    assert!(d.execute_scene_object_creation(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    assert!(
        d.object_index()
            .get(&ObjectRef::new(K::Mesh, x.id))
            .is_none()
    );
}

#[test]
fn deformation_commands_expose_their_projection_and_native_destination_availability() {
    use zipzi_editor_core::{CommandEffect, CommandId as C, EditorCommand, WorkspaceKind};
    let mut d = Document::new_blank().unwrap();
    let part = create(&mut d, P::Organization, T::Part, &[]);
    let parent = create(
        &mut d,
        P::Organization,
        T::Warp,
        std::slice::from_ref(&part),
    );
    let mut editor = crate::support::editor_from_document(d);
    let commands = [
        C::CreateDeformationMesh,
        C::CreateDeformationWarp,
        C::CreateDeformationRotation,
    ];
    editor.execute(EditorCommand::SelectObject(part)).unwrap();
    for id in commands {
        assert!(!editor.command_available(editor.command(id)));
        assert_eq!(id.scene_creation_projection(), Some(P::Deformation));
    }
    editor
        .execute(EditorCommand::SelectObject(parent.clone()))
        .unwrap();
    for id in commands {
        assert!(editor.command_available(editor.command(id)));
        assert!(matches!(
            editor.dispatch(editor.command(id)).unwrap(),
            CommandEffect::Host(_)
        ));
    }
    editor
        .execute(EditorCommand::SetObjectsLocked {
            objects: vec![parent],
            locked: true,
        })
        .unwrap();
    for id in commands {
        assert!(!editor.command_available(editor.command(id)));
    }
    editor.execute(EditorCommand::Undo).unwrap();
    editor.set_workspace(WorkspaceKind::Animation).unwrap();
    for id in commands {
        assert!(!editor.command_available(editor.command(id)));
    }
}
