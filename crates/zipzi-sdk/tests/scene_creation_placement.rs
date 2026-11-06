use zipzi_core::document::ObjectEditorState;
use zipzi_core::document::SceneCollection as C;
use zipzi_core::document::SceneCreate as New;
use zipzi_core::document::SceneEdit as E;
use zipzi_core::state::{RecordDomain, RootDomain};
use zipzi_core::{Canvas, Part, Vec2};
use zipzi_sdk::AuthoringSession;

fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "20000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}
fn part(s: &mut AuthoringSession) -> String {
    let id = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id.clone(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Part")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_scene(
                    intent(&id, object, part, parent, None, None),
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Create")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    id
}
fn members(s: &AuthoringSession, c: C) -> Vec<String> {
    s.snapshot()
        .document()
        .scene_collection(&c)
        .unwrap()
        .iter()
        .cloned()
        .collect()
}

#[test]
fn independent_creation_anchors_publish_one_history_and_survive_save_and_undo() {
    let mut s = session();
    let p = part(&mut s);
    let q = part(&mut s);
    let parent = create(&mut s, New::Rotation { origin: None }, Some(&p), None);
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
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            intent(
                &new,
                New::Mesh {
                    texture: Default::default(),
                },
                Some(&q),
                Some(&parent),
                Some(&b),
                Some(&a),
            ),
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert_eq!(s.history_state(), history);
    assert!(s.snapshot().document().mesh(&new).is_none());
    let receipt = s.publish(plan.1).unwrap();
    assert!(receipt
        .writes
        .records()
        .iter()
        .any(|record| record.domain == RecordDomain::Mesh && record.id == new));
    assert!(receipt.writes.root_changed(RootDomain::HierarchyOrder));
    assert_eq!(
        members(&s, org.clone()),
        [a.clone(), new.clone(), b.clone()]
    );
    assert_eq!(
        members(&s, deform.clone()),
        [new.clone(), a.clone(), b.clone()]
    );
    assert_eq!(s.snapshot().document().mesh(&new).unwrap().part_id, q);
    assert_eq!(
        s.snapshot().document().mesh(&new).unwrap().deformer_id,
        parent
    );
    assert_eq!(s.history_state().undo_steps, history.undo_steps + 1);
    s.undo(s.version()).unwrap();
    assert_eq!(members(&s, org.clone()), [a.clone(), b.clone()]);
    assert!(s.snapshot().document().mesh(&new).is_none());
    s.redo(s.version()).unwrap();
    assert_eq!(members(&s, deform.clone()), [new, a, b]);
    let reopened = AuthoringSession::new(crate::support::roundtrip(s.snapshot()));
    assert_eq!(members(&reopened, deform.clone()), members(&s, deform));
    assert_eq!(members(&reopened, org.clone()), members(&s, org));
}

#[test]
fn every_deformable_default_supports_typed_root_placement_via_direct_edits() {
    let mut s = session();
    let anchor = create(&mut s, New::Rotation { origin: None }, None, None);
    for object in [
        New::Mesh {
            texture: Default::default(),
        },
        New::Warp { bounds: None },
        New::Rotation { origin: None },
    ] {
        let new = id();
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene(
                        intent(&new, object, None, None, None, Some(&anchor)),
                        Default::default(),
                        &Default::default(),
                    )
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Create before")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
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
    let owned = create(&mut s, New::Rotation { origin: None }, Some(&p), None);
    let parent = create(&mut s, New::Rotation { origin: None }, None, None);
    let child = create(&mut s, New::Rotation { origin: None }, None, Some(&parent));
    create(&mut s, New::Rotation { origin: None }, None, None);
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    for anchor in [p, child.clone(), id()] {
        let new = id();
        let error = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene(
                intent(
                    &new,
                    New::Rotation { origin: None },
                    None,
                    None,
                    None,
                    Some(&anchor),
                ),
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap_err();
        assert_eq!(error.code.as_str(), "INVALID_DEFORMATION_PLACEMENT");
        assert!(s.snapshot().document().transform(&new).is_none());
    }
    let new = id();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            intent(
                &new,
                New::Rotation { origin: None },
                None,
                None,
                None,
                Some(&new)
            ),
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            intent(
                &id(),
                New::Rotation { origin: None },
                None,
                None,
                Some(&owned),
                Some(&parent)
            ),
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
}

#[test]
fn both_destination_locks_reject_creation_and_invalid_second_edit_rolls_back() {
    let mut s = session();
    let p = part(&mut s);
    let q = part(&mut s);
    let parent = create(&mut s, New::Rotation { origin: None }, Some(&p), None);
    for lock in [&q, &parent] {
        let mut state = s.snapshot().document().editor_state().clone();
        state.objects.insert(
            lock.to_string(),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_editor_state(state))
                .and_then(|value| {
                    let prepared = draft.prepare("Lock")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let history = s.history_state();
        let error = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene(
                intent(
                    &id(),
                    New::Rotation { origin: None },
                    Some(&q),
                    Some(&parent),
                    None,
                    None,
                ),
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap_err();
        assert_eq!(error.code.as_str(), "OBJECT_LOCKED");
        assert_eq!(s.history_state(), history);
        s.undo(s.version()).unwrap();
    }
    let new = id();
    let history = s.history_state();
    let version = s.version();
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_scene(
                    intent(
                        &new,
                        New::Rotation { origin: None },
                        Some(&q),
                        Some(&parent),
                        None,
                        None,
                    ),
                    Default::default(),
                    &Default::default(),
                )?;
                e.edit_scene(
                    intent(
                        &id(),
                        New::Rotation { origin: None },
                        Some(&q),
                        Some(&parent),
                        None,
                        Some(&p),
                    ),
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Atomic")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert!(s.snapshot().document().transform(&new).is_none());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
}

#[test]
fn offscreen_and_glue_cannot_acquire_a_deformation_position() {
    let mut s = session();
    let p = part(&mut s);
    let anchor = create(&mut s, New::Rotation { origin: None }, None, None);
    for object in [
        New::Offscreen,
        New::Glue {
            a: Default::default(),
            b: Default::default(),
        },
    ] {
        let part = matches!(object, New::Offscreen).then_some(p.as_str());
        assert_eq!(
            crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
                intent(&id(), object, part, None, None, Some(&anchor)),
                Default::default(),
                &Default::default()
            ))
            .unwrap_err()
            .code
            .as_str(),
            "NO_DEFORMATION_COLLECTION"
        );
    }
}

#[test]
fn serialized_placement_and_stale_plan_preserve_native_version_guards() {
    let mut s = session();
    let anchor = create(&mut s, New::Rotation { origin: None }, None, None);
    let new = id();
    let json = serde_json::json!({"operation":"create", "id":new, "name":"JSON", "object":{"kind":"rotation"}, "deformation_before":anchor});
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            serde_json::from_value(json).unwrap(),
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    create(&mut s, New::Rotation { origin: None }, None, None);
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    assert!(s.publish(plan.1).is_err());
    assert!(s.snapshot().document().transform(&new).is_none());
    assert_eq!(s.history_state(), history);
}
