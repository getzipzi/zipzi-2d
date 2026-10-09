use editor_core::*;
#[allow(dead_code)]
mod support;

fn object(kind: ObjectKind, n: u128) -> ObjectRef {
    ObjectRef::new(kind, uuid::Uuid::from_u128(n).to_string())
}

fn visible(nodes: &[TreeNode], id: &str) -> Option<bool> {
    nodes.iter().find_map(|node| {
        if node.id == id {
            node.visible
        } else {
            visible(&node.children, id)
        }
    })
}

#[test]
fn all_scene_objects_support_visibility_under_locks_with_history_and_persistence()
-> anyhow::Result<()> {
    let (doc, dir) = support::sample_document_with_scene_objects()?;
    let mut editor = Editor::from_document(doc);
    let mesh = ObjectRef::new(ObjectKind::Mesh, editor.layers()[0].id.clone());
    let selection = editor.document().unwrap().context().selections.clone();
    for target in [
        object(ObjectKind::Part, 101),
        object(ObjectKind::Deformer, 501),
        object(ObjectKind::Deformer, 502),
        object(ObjectKind::Offscreen, 601),
        mesh,
    ] {
        editor.execute(EditorCommand::SetObjectsLocked {
            objects: vec![target.clone()],
            locked: true,
        })?;
        let history = editor.document().unwrap().history_lengths().0;
        let revision = editor.tree_revision();
        editor.execute(EditorCommand::SetObjectVisible {
            object: target.clone(),
            visible: false,
        })?;
        assert_ne!(editor.tree_revision(), revision);
        assert_eq!(visible(&editor.mesh_tree(), &target.id), Some(false));
        if matches!(target.kind, ObjectKind::Mesh | ObjectKind::Deformer) {
            assert_eq!(visible(&editor.deformer_tree(), &target.id), Some(false));
        }
        assert_eq!(editor.document().unwrap().context().selections, selection);
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history + 1, 0)
        );
        editor.execute(EditorCommand::SetObjectVisible {
            object: target.clone(),
            visible: false,
        })?;
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(visible(&editor.mesh_tree(), &target.id), Some(true));
        editor.execute(EditorCommand::Redo)?;
        assert_eq!(visible(&editor.mesh_tree(), &target.id), Some(false));

        let mut task = editor
            .prepare_save(
                editor.active_id().unwrap(),
                &dir.path().join("hidden.kasane"),
                false,
            )?
            .run();
        editor.finish_save(&mut task)?;
        let reopened = Document::open(&dir.path().join("hidden.kasane"))?;
        assert_eq!(visible(&reopened.mesh_tree(), &target.id), Some(false));
        editor.execute(EditorCommand::SetObjectVisible {
            object: target,
            visible: true,
        })?;
    }
    Ok(())
}

#[test]
fn parent_visibility_preserves_child_flags_and_updates_canvas() -> anyhow::Result<()> {
    for parent in [
        object(ObjectKind::Part, 101),
        object(ObjectKind::Offscreen, 601),
    ] {
        let (mut doc, _dir) = support::sample_document_with_scene_objects()?;
        let meshes = doc.layers();
        let child = ObjectRef::new(ObjectKind::Mesh, &meshes[1].id);
        doc.set_object_visible(&child, false)?;
        doc.set_object_visible(&parent, false)?;
        assert_eq!(visible(&doc.mesh_tree(), &meshes[0].id), Some(true));
        assert_eq!(visible(&doc.mesh_tree(), &child.id), Some(false));
        assert_eq!(
            visible(&doc.mesh_tree(), &object(ObjectKind::Part, 103).id),
            Some(true)
        );
        let snapshot = doc.snapshot()?;
        let scene = CanvasScene::new(&doc, snapshot);
        assert!(meshes.iter().all(|m| !scene.mesh_visible(&m.id)));
        doc.set_object_visible(&parent, true)?;
        let snapshot = doc.snapshot()?;
        let scene = CanvasScene::new(&doc, snapshot);
        assert!(scene.mesh_visible(&meshes[0].id));
        assert!(!scene.mesh_visible(&child.id));
    }
    Ok(())
}

#[test]
fn hiding_nested_warp_preserves_its_parent_and_sibling_mesh() -> anyhow::Result<()> {
    let (mut doc, _dir) = support::sample_document_with_scene_objects()?;
    let meshes = doc.layers();
    let warp = object(ObjectKind::Deformer, 502);
    doc.set_object_visible(&warp, false)?;
    let snapshot = doc.snapshot()?;
    let scene = CanvasScene::new(&doc, snapshot);
    assert!(scene.mesh_visible(&meshes[0].id));
    assert!(scene.mesh_visible(&meshes[1].id));
    assert_eq!(
        visible(&doc.deformer_tree(), &object(ObjectKind::Deformer, 501).id),
        Some(true)
    );
    assert!(
        scene
            .controls
            .iter()
            .any(|c| c.object == warp && c.enabled && !c.visible)
    );
    doc.undo()?;
    let snapshot = doc.snapshot()?;
    let scene = CanvasScene::new(&doc, snapshot);
    assert!(meshes.iter().all(|m| scene.mesh_visible(&m.id)));
    Ok(())
}

#[test]
fn hiding_parent_deformer_only_hides_its_controls_and_preserves_deformation() -> anyhow::Result<()>
{
    let (mut doc, _dir) = support::sample_document_with_scene_objects()?;
    let parent = object(ObjectKind::Deformer, 501);
    let child = object(ObjectKind::Deformer, 502);
    let before = doc.snapshot()?;
    doc.set_object_visible(&parent, false)?;
    assert_eq!(visible(&doc.deformer_tree(), &parent.id), Some(false));
    assert_eq!(visible(&doc.deformer_tree(), &child.id), Some(true));
    assert_eq!(doc.object_index().get(&parent).unwrap().enabled, Some(true));
    let after = doc.snapshot()?;
    assert!(
        std::sync::Arc::ptr_eq(&before.frame, &after.frame),
        "Control display must not invalidate runtime evaluation"
    );
    let scene = CanvasScene::new(&doc, after);
    assert!(doc.layers().iter().all(|mesh| scene.mesh_visible(&mesh.id)));
    let transform = ViewportTransform::new(
        scene.source.frame.canvas,
        CanvasMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        },
        ViewState::default(),
    )
    .unwrap();
    let mesh = scene.mesh(&doc.layers()[0].id).unwrap();
    let [x0, y0, x1, y1] = mesh.bounds.unwrap();
    let candidates = scene.candidates(
        transform.to_canvas(ModelPoint {
            x: (x0 + x1) / 2.,
            y: (y0 + y1) / 2.,
        }),
        transform,
        &[],
        ToolId::Select,
        DeformerVisibility::default(),
    );
    assert!(candidates.iter().any(|c| c.object.kind == ObjectKind::Mesh));
    assert!(!candidates.iter().any(|c| c.object == parent));
    let control = scene.controls.iter().find(|c| c.object == parent).unwrap();
    assert!(!scene.control_visible(
        control,
        &[parent.clone()],
        ToolId::Select,
        DeformerVisibility {
            all: true,
            ancestors: true
        }
    ));
    assert!(
        scene
            .controls
            .iter()
            .any(|c| c.object == child && c.enabled)
    );
    doc.undo()?;
    assert!(doc.deformer_controls_visible(&parent.id));
    doc.redo()?;
    assert!(!doc.deformer_controls_visible(&parent.id));
    assert!(std::sync::Arc::ptr_eq(
        &before.frame,
        &doc.snapshot()?.frame
    ));
    // Hidden parents still provide the coordinate space for edits to their children.
    doc.adjust_deformer(
        &child,
        DeformerAdjustment {
            translation: ModelPoint { x: 3., y: 2. },
            angle: 0.,
            scale: 1.,
        },
    )?;
    let edited = doc.snapshot()?;
    assert_ne!(
        before.frame.deformers[1].points,
        edited.frame.deformers[1].points
    );
    assert!(doc.layers().iter().all(|mesh| mesh.visible));
    Ok(())
}

#[test]
fn visibility_rejects_missing_and_non_scene_objects_without_history() -> anyhow::Result<()> {
    let (mut doc, _dir) = support::sample_document()?;
    for target in [
        object(ObjectKind::Part, 999),
        object(ObjectKind::Deformer, 999),
        object(ObjectKind::Offscreen, 999),
        object(ObjectKind::Parameter, 999),
    ] {
        assert!(doc.set_object_visible(&target, false).is_err());
    }
    assert_eq!(doc.history_lengths(), (0, 0));
    Ok(())
}
