use zipzi_core::document::ObjectEditorState;
use zipzi_core::{Canvas, Part, Vec2};
use zipzi_sdk::{
    AuthoringSession, CollectionRef, SceneCollection as C, SceneCreate as New, SceneEdit as E,
};

fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "20000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}
fn part(s: &mut AuthoringSession) -> String {
    let id = id();
    s.edit("Part", None, |e| {
        e.create_part(Part {
            id: id.clone(),
            ..Default::default()
        })
    })
    .unwrap();
    id
}
fn intent(
    id: &str,
    object: New,
    part: Option<&str>,
    parent: Option<&str>,
    before: Option<&str>,
    deformation_before: Option<&str>,
) -> E {
    E::Create {
        id: id.into(),
        name: "New".into(),
        object,
        part_id: part.map(str::to_owned),
        parent_id: parent.map(str::to_owned),
        before: before.map(str::to_owned),
        deformation_before: deformation_before.map(str::to_owned),
    }
}
fn create(
    s: &mut AuthoringSession,
    object: New,
    part: Option<&str>,
    parent: Option<&str>,
) -> String {
    let id = id();
    s.edit("Create", None, |e| {
        e.edit_scene(intent(&id, object, part, parent, None, None))
    })
    .unwrap();
    id
}
fn members(s: &AuthoringSession, c: C) -> Vec<String> {
    s.scene_collection(&c)
        .unwrap()
        .iter()
        .map(|h| h.id().to_owned())
        .collect()
}

#[test]
fn independent_creation_anchors_publish_one_history_and_survive_save_and_undo() {
    let mut s = session();
    let p = part(&mut s);
    let q = part(&mut s);
    let parent = create(&mut s, New::Rotation, Some(&p), None);
    let a = create(
        &mut s,
        New::Mesh {
            texture: Default::default(),
        },
        Some(&q),
        Some(&parent),
    );
    let b = create(
        &mut s,
        New::Mesh {
            texture: Default::default(),
        },
        Some(&q),
        Some(&parent),
    );
    let org = C::Organization {
        part_id: Some(q.clone()),
    };
    let deform = C::Deformation {
        parent_id: Some(parent.clone()),
    };
    let history = s.history_state();
    let new = id();
    let plan = s
        .plan_scene_edit(intent(
            &new,
            New::Mesh {
                texture: Default::default(),
            },
            Some(&q),
            Some(&parent),
            Some(&b),
            Some(&a),
        ))
        .unwrap();
    assert_eq!(s.history_state(), history);
    assert!(s.document().get_mesh(&new).is_none());
    let receipt = s.execute_scene_plan(&plan).unwrap();
    assert!(receipt.changed);
    assert_eq!(receipt.changes.created.len(), 1);
    assert!(receipt.changes.collections.iter().any(|c| matches!(c,
        CollectionRef::Organization { collection: zipzi_sdk::OrganizationCollection::PartMembers { part_id: Some(owner) }, .. }
            if owner == &q)));
    assert!(receipt
        .changes
        .collections
        .iter()
        .any(|c| matches!(c, CollectionRef::Scene { collection, .. } if *collection == deform)));
    assert_eq!(
        members(&s, org.clone()),
        [a.clone(), new.clone(), b.clone()]
    );
    assert_eq!(
        members(&s, deform.clone()),
        [new.clone(), a.clone(), b.clone()]
    );
    assert_eq!(s.document().get_mesh(&new).unwrap().part_id, q);
    assert_eq!(s.document().get_mesh(&new).unwrap().deformer_id, parent);
    assert_eq!(s.history_state().undo_steps, history.undo_steps + 1);
    s.undo().unwrap();
    assert_eq!(members(&s, org.clone()), [a.clone(), b.clone()]);
    assert!(s.document().get_mesh(&new).is_none());
    s.redo().unwrap();
    assert_eq!(members(&s, deform.clone()), [new, a, b]);
    let dir = std::env::temp_dir().join(format!(
        "zipzi-scene-placement-{}-{}",
        std::process::id(),
        id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let saved = s.save_project(&dir.join("positions.zipzi"), None).unwrap();
    let mut reopened = session();
    reopened.open_project(&saved.manifest, None).unwrap();
    assert_eq!(members(&reopened, deform.clone()), members(&s, deform));
    assert_eq!(members(&reopened, org.clone()), members(&s, org));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_deformable_default_supports_typed_root_placement_via_direct_edits() {
    let mut s = session();
    let anchor = create(&mut s, New::Rotation, None, None);
    for object in [
        New::Mesh {
            texture: Default::default(),
        },
        New::Warp { bounds: None },
        New::Rotation,
    ] {
        let new = id();
        s.edit("Create before", None, |e| {
            e.edit_scene(intent(&new, object, None, None, None, Some(&anchor)))
        })
        .unwrap();
        let order = members(&s, C::Deformation { parent_id: None });
        let at = order.iter().position(|id| id == &anchor).unwrap();
        assert_eq!(order[at - 1], new);
    }
}

#[test]
fn wrong_collection_missing_and_self_anchors_reject_without_consuming_redo() {
    let mut s = session();
    let p = part(&mut s);
    let owned = create(&mut s, New::Rotation, Some(&p), None);
    let parent = create(&mut s, New::Rotation, None, None);
    let child = create(&mut s, New::Rotation, None, Some(&parent));
    create(&mut s, New::Rotation, None, None);
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    for anchor in [p, child.clone(), id()] {
        let new = id();
        let error = s
            .plan_scene_edit(intent(&new, New::Rotation, None, None, None, Some(&anchor)))
            .unwrap_err();
        assert_eq!(error.code.as_ref(), "INVALID_DEFORMATION_PLACEMENT");
        assert!(s.document().get_transform(&new).is_none());
    }
    let new = id();
    assert!(s
        .plan_scene_edit(intent(&new, New::Rotation, None, None, None, Some(&new)))
        .is_err());
    assert!(s
        .plan_scene_edit(intent(
            &id(),
            New::Rotation,
            None,
            None,
            Some(&owned),
            Some(&parent)
        ))
        .is_err());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
}

#[test]
fn both_destination_locks_reject_creation_and_invalid_second_edit_rolls_back() {
    let mut s = session();
    let p = part(&mut s);
    let q = part(&mut s);
    let parent = create(&mut s, New::Rotation, Some(&p), None);
    for lock in [&q, &parent] {
        let mut state = s.document().editor_state().clone();
        state.objects.insert(
            lock.to_string(),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        s.edit("Lock", None, |e| e.replace_editor_state(state))
            .unwrap();
        let history = s.history_state();
        let error = s
            .plan_scene_edit(intent(
                &id(),
                New::Rotation,
                Some(&q),
                Some(&parent),
                None,
                None,
            ))
            .unwrap_err();
        assert_eq!(error.code.as_ref(), "OBJECT_LOCKED");
        assert_eq!(s.history_state(), history);
        s.undo().unwrap();
    }
    let new = id();
    let history = s.history_state();
    let version = s.version();
    assert!(s
        .edit("Atomic", None, |e| {
            e.edit_scene(intent(
                &new,
                New::Rotation,
                Some(&q),
                Some(&parent),
                None,
                None,
            ))?;
            e.edit_scene(intent(
                &id(),
                New::Rotation,
                Some(&q),
                Some(&parent),
                None,
                Some(&p),
            ))
        })
        .is_err());
    assert!(s.document().get_transform(&new).is_none());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
}

#[test]
fn offscreen_and_glue_cannot_acquire_a_deformation_position() {
    let mut s = session();
    let p = part(&mut s);
    let anchor = create(&mut s, New::Rotation, None, None);
    for object in [
        New::Offscreen,
        New::Glue {
            a: Default::default(),
            b: Default::default(),
        },
    ] {
        let part = matches!(object, New::Offscreen).then_some(p.as_str());
        assert_eq!(
            s.plan_scene_edit(intent(&id(), object, part, None, None, Some(&anchor)))
                .unwrap_err()
                .code
                .as_ref(),
            "NO_DEFORMATION_COLLECTION"
        );
    }
}

#[test]
fn serialized_placement_and_stale_plan_preserve_native_version_guards() {
    let mut s = session();
    let anchor = create(&mut s, New::Rotation, None, None);
    let new = id();
    let json = serde_json::json!({"operation":"create", "id":new, "name":"JSON", "object":{"kind":"rotation"}, "deformation_before":anchor});
    let plan = s
        .plan_scene_edit(serde_json::from_value(json).unwrap())
        .unwrap();
    create(&mut s, New::Rotation, None, None);
    s.undo().unwrap();
    let history = s.history_state();
    assert!(s.execute_scene_plan(&plan).is_err());
    assert!(s.document().get_transform(&new).is_none());
    assert_eq!(s.history_state(), history);
}
