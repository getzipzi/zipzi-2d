use super::support::{self, object};
use editor_core::*;

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
    let mut editor = crate::support::editor_from_document(doc);
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
        assert_eq!(
            editor
                .document()
                .unwrap()
                .object_index()
                .get(&target)
                .unwrap()
                .enabled,
            Some(true)
        );
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
fn hiding_a_part_does_not_follow_deformation_links_into_another_part() -> anyhow::Result<()> {
    let (mut doc, dir) = support::sample_document_with_scene_objects()?;
    let saved = doc.save(&dir.path().join("cross-part.kasane"))?;
    let mut sdk = kasane_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        kasane_core::Canvas::new(256., 256., kasane_core::Vec2::default(), 1.),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let mesh = ObjectRef::new(ObjectKind::Mesh, sdk.mesh_ids()[1].clone());
    let parent = object(ObjectKind::Part, 101);
    let child = object(ObjectKind::Deformer, 502);
    let mut transform = sdk.document().get_transform(&child.id).unwrap().clone();
    transform.part_id = Some(object(ObjectKind::Part, 102).id.clone().into());
    sdk.edit("Cross-part fixture", None, |e| {
        e.set_mesh_part(&mesh.id, &object(ObjectKind::Part, 102).id)?;
        e.replace_transform(transform)
    })?;
    sdk.save_project(&saved.manifest, None)?;
    let mut doc = Document::open(&saved.manifest)?;
    let before = doc.snapshot()?;
    doc.set_object_visible(&parent, false)?;
    let after = doc.snapshot()?;
    assert!(std::sync::Arc::ptr_eq(
        &before.runtime_frame,
        &after.runtime_frame
    ));
    assert_eq!(
        before.viewport_frame.drawables[1].positions,
        after.viewport_frame.drawables[1].positions
    );
    let scene = CanvasScene::new(&doc, after);
    assert!(scene.mesh_visible(&mesh.id));
    assert!(doc.deformer_controls_visible(&child.id));
    assert!(!doc.deformer_controls_visible(&object(ObjectKind::Deformer, 501).id));
    let own_mesh = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let state = doc.object_state(&own_mesh);
    assert!(!state.explicit.hidden_in_editor);
    assert_eq!(state.hidden_by, vec![parent]);
    Ok(())
}

#[test]
fn hidden_mesh_remains_a_mask_source_for_picking_and_output() -> anyhow::Result<()> {
    let (mut doc, dir) = support::sample_document_with_two_layers()?;
    let saved = doc.save(&dir.path().join("masked.kasane"))?;
    let mut sdk = kasane_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        kasane_core::Canvas::new(256., 256., kasane_core::Vec2::default(), 1.),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let ids = sdk.mesh_ids().to_vec();
    let mut masked = sdk.mesh(&ids[1]).unwrap();
    masked.masks = vec![ids[0].clone()];
    sdk.edit("Mask fixture", None, |e| e.replace_mesh(masked))?;
    sdk.save_project(&saved.manifest, None)?;
    let mut doc = Document::open(&saved.manifest)?;
    let before = doc.snapshot()?;
    doc.set_object_visible(
        &editor_core::ObjectRef::new(editor_core::ObjectKind::Mesh, &ids[0]),
        false,
    )?;
    let after = doc.snapshot()?;
    assert!(std::sync::Arc::ptr_eq(
        &before.runtime_frame,
        &after.runtime_frame
    ));
    assert!(!after.viewport_frame.drawables[0].visible);
    assert!(after.viewport_frame.drawables[0].enabled);
    assert_eq!(
        after.viewport_frame.drawables[0].positions,
        before.viewport_frame.drawables[0].positions
    );
    assert_eq!(
        after.viewport_frame.drawables[1].masks,
        vec![ids[0].clone()]
    );
    let scene = CanvasScene::new(&doc, after);
    assert_eq!(
        scene.mesh_hits(ModelPoint { x: 128., y: 128. }),
        vec![ids[1].clone()]
    );
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
        std::sync::Arc::ptr_eq(&before.viewport_frame, &after.viewport_frame),
        "Control display must not invalidate runtime evaluation"
    );
    let scene = CanvasScene::new(&doc, after);
    assert!(doc.layers().iter().all(|mesh| scene.mesh_visible(&mesh.id)));
    let transform = ViewportTransform::new(
        ViewportMetrics {
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
        transform.to_viewport(ModelPoint {
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
        &before.viewport_frame,
        &doc.snapshot()?.viewport_frame
    ));
    // Hidden parents still provide the coordinate space for edits to their children.
    doc.adjust_deformer(
        &child,
        DeformerAdjustment {
            translation: editor_core::ModelVector { x: 3., y: 2. },
            angle: 0.,
            scale: 1.,
        },
    )?;
    let edited = doc.snapshot()?;
    assert_ne!(
        before.viewport_frame.deformers[1].points,
        edited.viewport_frame.deformers[1].points
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

#[test]
fn runtime_enable_and_editor_visibility_are_independent_and_recoverable() -> anyhow::Result<()> {
    let (mut doc, _dir) = support::sample_document_with_scene_objects()?;
    for target in [
        object(ObjectKind::Part, 101),
        object(ObjectKind::Deformer, 501),
        object(ObjectKind::Offscreen, 601),
        ObjectRef::new(ObjectKind::Mesh, doc.layers()[0].id.clone()),
    ] {
        let runtime = doc.snapshot()?.runtime_frame;
        doc.set_object_visible(&target, false)?;
        assert!(std::sync::Arc::ptr_eq(
            &runtime,
            &doc.snapshot()?.runtime_frame
        ));
        doc.set_object_property(&target, "enabled", false.into())?;
        doc.set_object_visible(&target, true)?;
        assert_eq!(
            doc.object_index().get(&target).unwrap().enabled,
            Some(false)
        );
        assert!(!doc.object_state(&target).explicit.hidden_in_editor);
        doc.set_object_property(&target, "enabled", true.into())?;
        assert_eq!(doc.object_index().get(&target).unwrap().enabled, Some(true));
        assert!(!doc.object_state(&target).explicit.hidden_in_editor);
        doc.undo()?;
        assert_eq!(
            doc.object_index().get(&target).unwrap().enabled,
            Some(false)
        );
        doc.redo()?;
    }
    Ok(())
}
