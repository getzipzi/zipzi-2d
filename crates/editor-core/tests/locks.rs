use editor_core::*;
use std::sync::Arc;
#[allow(dead_code)]
mod support;
fn object(kind: ObjectKind, n: u128) -> ObjectRef {
    ObjectRef::new(kind, uuid::Uuid::from_u128(n).to_string())
}
fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (doc, dir) = support::sample_document_with_two_layers()?;
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    Ok((editor, dir))
}
#[test]
fn explicit_and_inherited_locks_survive_save_and_do_not_cross_hierarchies() -> anyhow::Result<()> {
    use kasane_core::{RotationTransform, Transform, TransformData};
    let (mut doc, dir) = support::sample_document_with_parts()?;
    let saved = doc.save(&dir.path().join("hierarchy.kasane"))?;
    let mut sdk = kasane_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        kasane_core::Canvas::new(256., 256., kasane_core::Vec2::default(), 1.),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let mesh = ObjectRef::new(ObjectKind::Mesh, sdk.mesh_ids()[1].clone());
    let part = object(ObjectKind::Part, 101);
    let other = object(ObjectKind::Part, 102);
    let deformer = object(ObjectKind::Deformer, 501);
    sdk.edit("Cross-part fixture", None, |e| {
        e.create_transform(Transform {
            id: deformer.id.clone(),
            part_id: Some(part.id.clone().into()),
            data: TransformData::Rotation(RotationTransform::default()),
            ..Default::default()
        })?;
        e.set_mesh_part(&mesh.id, &other.id)?;
        e.set_deform_parent(&mesh.id, &deformer.id)
    })?;
    sdk.save_project(&saved.manifest, None)?;
    let mut doc = Document::open(&saved.manifest)?;
    doc.set_objects_locked(&[part.clone()], true)?;
    assert!(doc.lock_index().is_locked(&deformer));
    assert!(
        !doc.lock_index().is_locked(&mesh),
        "inherited Part lock must not propagate through a Deformer into another Part"
    );
    doc.set_objects_locked(&[deformer.clone(), mesh.clone()], true)?;
    assert_eq!(doc.history_lengths(), (2, 0));
    let state = doc.lock_index().state(&mesh).clone();
    assert!(state.explicit);
    assert!(state.inherited.is_empty());
    doc.set_objects_locked(&[mesh.clone()], false)?;
    assert!(!doc.lock_index().is_locked(&mesh));
    doc.set_objects_locked(&[part.clone()], false)?;
    assert!(doc.lock_index().is_locked(&deformer));
    let saved = doc.save(&dir.path().join("locked.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(
        reopened.lock_index().state(&mesh),
        doc.lock_index().state(&mesh)
    );
    doc.undo()?;
    assert!(doc.lock_index().state(&part).explicit);
    Ok(())
}
#[test]
fn deformer_lock_only_protects_itself_but_part_locks_still_protect_members() -> anyhow::Result<()> {
    let (mut doc, dir) = support::sample_document_with_scene_objects()?;
    let parent = object(ObjectKind::Deformer, 501);
    let child = object(ObjectKind::Deformer, 502);
    let mesh = ObjectRef::new(ObjectKind::Mesh, doc.layers()[1].id.clone());
    doc.set_objects_locked(&[parent.clone()], true)?;
    assert!(doc.ensure_editable(&[parent.clone()]).is_err());
    doc.ensure_editable(&[child.clone(), mesh.clone()])?;
    let adjustment = DeformerAdjustment {
        translation: ModelPoint { x: 3., y: 2. },
        angle: 0.,
        scale: 1.,
    };
    assert!(doc.adjust_deformer(&parent, adjustment).is_err());
    doc.adjust_deformer(&child, adjustment)?;
    doc.set_object_property(&child, "name", serde_json::json!("Editable child"))?;
    doc.select_object(mesh.clone())?;
    doc.preview_name("Editable mesh")?;
    doc.finish_edit()?;
    doc.set_objects_locked(&[child.clone()], true)?;
    assert!(doc.lock_index().is_locked(&child));
    assert!(!doc.lock_index().is_locked(&mesh));
    doc.undo()?;
    assert!(!doc.lock_index().is_locked(&child));
    doc.redo()?;
    assert!(doc.lock_index().state(&child).explicit);
    let saved = doc.save(&dir.path().join("local-locks.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert!(reopened.lock_index().is_locked(&parent));
    assert!(reopened.lock_index().is_locked(&child));
    assert!(!reopened.lock_index().is_locked(&mesh));
    let part = object(ObjectKind::Part, 101);
    doc.set_objects_locked(&[part.clone()], true)?;
    assert!(doc.ensure_editable(&[mesh.clone()]).is_err());
    assert_eq!(doc.lock_index().state(&mesh).inherited, vec![part]);
    Ok(())
}

#[test]
fn locked_selection_is_inspectable_but_direct_writes_and_structure_moves_fail() -> anyhow::Result<()>
{
    let (mut doc, _dir) = support::sample_document_with_parts()?;
    let mesh = ObjectRef::new(ObjectKind::Mesh, doc.layers()[0].id.clone());
    let parent = object(ObjectKind::Part, 101);
    let destination = object(ObjectKind::Part, 102);
    doc.set_objects_locked(&[mesh.clone()], true)?;
    doc.select_objects(vec![mesh.clone()])?;
    assert!(doc.select_canvas_objects(vec![mesh.clone()]).is_err());
    assert!(doc.preview_name("changed").is_err());
    assert!(doc.preview_opacity(0.4).is_err());
    assert!(
        doc.move_organization(
            &mesh,
            OrganizationDestination::Part(Some(destination.id.clone()))
        )
        .is_err()
    );
    assert!(
        doc.move_organization(
            &parent,
            OrganizationDestination::Part(Some(destination.id.clone()))
        )
        .is_err()
    );
    assert!(
        doc.reorder_tree(false, &parent, &destination, ReorderPosition::Before)
            .is_err()
    );
    assert_eq!(doc.history_lengths(), (1, 0));
    doc.set_layer_visible(&mesh.id, false)?;
    assert!(doc.lock_index().is_locked(&mesh));
    doc.set_objects_locked(&[parent.clone()], true)?;
    assert!(
        doc.set_object_property(&parent, "name", serde_json::json!("changed"))
            .is_err()
    );
    Ok(())
}
#[test]
fn lock_undo_redo_reuse_frames_and_live_picking_skips_locked_top_surface() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let first = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(first.clone()));
    let top = ObjectRef::new(
        ObjectKind::Mesh,
        editor
            .layers()
            .into_iter()
            .find(|l| l.name == "Second")
            .unwrap()
            .id,
    );
    let lower = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    let evaluations = editor.evaluation_count();
    editor.execute(EditorCommand::SelectObject(top.clone()))?;
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![top.clone()],
        locked: true,
    })?;
    assert!(editor.preview_if_needed()?.is_none());
    assert!(editor.overlay().vertices.is_empty());
    assert!(!editor.overlay().accent_lines.is_empty());
    let point = CanvasPoint { x: 256., y: 256. };
    let picks = editor.pick_candidates(point, Some(first.version));
    assert!(!picks.iter().any(|p| p.object == top));
    assert!(picks.iter().any(|p| p.object == lower));
    assert!(
        editor
            .execute(EditorCommand::SelectCanvasObjects(vec![top.clone()]))
            .is_err()
    );
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up { point })?;
    assert_eq!(editor.document().unwrap().context().selection, Some(lower));
    editor.execute(EditorCommand::Undo)?;
    assert!(!editor.document().unwrap().lock_index().is_locked(&top));
    editor.execute(EditorCommand::Redo)?;
    assert!(editor.document().unwrap().lock_index().is_locked(&top));
    let after = editor.preview()?.canvas.unwrap();
    assert!(Arc::ptr_eq(&first.geometry, &after.geometry));
    assert_eq!(editor.evaluation_count(), evaluations);
    Ok(())
}
#[test]
fn locking_during_a_drag_cancels_unpublished_geometry_and_late_pointer_events() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let selected = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    let original = editor.document().unwrap().selected_vertex_positions();
    let point = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    let moved = CanvasPoint {
        x: point.x + 30.,
        y: point.y + 20.,
    };
    editor.canvas_input(CanvasInput::Move {
        point: moved,
        left_down: true,
    })?;
    assert!(editor.document().unwrap().has_pending_edit());
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: vec![selected],
        locked: true,
    })?;
    editor.canvas_input(CanvasInput::Up { point: moved })?;
    assert!(!editor.gesture_active());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}
#[test]
fn rectangle_selection_never_adds_locked_objects_or_components() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let objects: Vec<_> = editor
        .layers()
        .iter()
        .map(|l| ObjectRef::new(ObjectKind::Mesh, &l.id))
        .collect();
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: objects.clone(),
        locked: true,
    })?;
    editor.execute(EditorCommand::SelectObjects(objects))?;
    editor.canvas_input(CanvasInput::Down {
        point: CanvasPoint { x: 0., y: 0. },
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint { x: 512., y: 512. },
    })?;
    assert!(editor.document().unwrap().context().selections.is_empty());
    assert!(editor.overlay().vertices.is_empty());
    Ok(())
}

#[test]
fn parameter_preview_and_visibility_remain_available_on_locked_binding_targets()
-> anyhow::Result<()> {
    let (mut doc, _dir) = support::sample_document_with_binding(true)?;
    let target = doc.context().selection.clone().unwrap();
    let binding = doc.object_index().bindings[0].reference.clone();
    let parameter = doc.parameters()[0].id.clone();
    doc.set_objects_locked(&[target.clone()], true)?;
    assert!(doc.ensure_editable(&[binding.clone()]).is_err());
    doc.open_binding(binding.clone())?;
    doc.set_preview_parameter(&parameter, 0.5)?;
    assert_eq!(doc.parameters()[0].value, 0.5);
    doc.preview_keyform(&binding, 1)?;
    assert_eq!(doc.parameters()[0].value, 1.);
    doc.set_layer_visible(&target.id, false)?;
    assert!(!doc.layers()[0].visible);
    assert!(doc.lock_index().is_locked(&target));
    Ok(())
}

#[test]
fn rename_history_still_refreshes_canvas_labels_after_lock_history_optimization()
-> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let object = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    let original = editor.document().unwrap().selected_layer().unwrap().name;
    editor.execute(EditorCommand::PreviewName("Renamed".into()))?;
    editor.execute(EditorCommand::FinishEdit)?;
    assert_eq!(
        editor
            .preview()?
            .canvas
            .unwrap()
            .geometry
            .index
            .name(&object),
        "Renamed"
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor
            .preview()?
            .canvas
            .unwrap()
            .geometry
            .index
            .name(&object),
        original
    );
    Ok(())
}
