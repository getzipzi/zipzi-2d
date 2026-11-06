use std::sync::Arc;
use zipzi_core::{
    document::*,
    state::{DocumentState, OwnedDocument},
    Part,
};
use zipzi_sdk::{AuthoringSession, Commands, ResourceView, Snapshot, Status};
fn id(n: u32) -> String {
    format!("00100000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    let doc = DocumentState::from_owned(OwnedDocument {
        id: id(1),
        parts: vec![Part {
            id: id(2),
            runtime_id: "PartA".into(),
            ..Default::default()
        }],
        ..Default::default()
    })
    .unwrap();
    AuthoringSession::new(Snapshot::new(doc, ResourceView::default()).unwrap())
}
fn edit(s: &mut AuthoringSession, run: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>) {
    let mut d = s.begin_draft();
    d.batch(d.source(), run).unwrap();
    s.publish(d.prepare("animation domains").unwrap()).unwrap();
}
fn pose_fixture(s: &mut AuthoringSession) {
    edit(s, |c| {
        for n in [4, 5] {
            c.edit_pose(PoseEdit::CreateGroup {
                pose_id: id(3),
                id: id(n),
                name: format!("Group {n}"),
                before: None,
            })?;
        }
        c.edit_pose(PoseEdit::CreateEntry {
            group_id: id(4),
            id: id(6),
            before: None,
        })?;
        c.edit_pose(PoseEdit::CreateLink {
            entry_id: id(6),
            id: id(7),
            before: None,
        })
    });
}
#[test]
fn pose_moves_copies_dissolves_and_patches_use_one_snapshot_history() {
    let mut s = session();
    let empty = s.snapshot().clone();
    pose_fixture(&mut s);
    let expected = s.snapshot().document().pose_fields(&id(6)).unwrap();
    edit(&mut s, |c| {
        c.edit_pose(PoseEdit::Patch {
            id: id(6),
            expected,
            fields: PoseFields::Entry {
                part: PosePartRef::Resolved { part_id: id(2) },
            },
        })?;
        c.edit_pose(PoseEdit::DissolveGroup {
            id: id(4),
            destination: id(5),
            before: None,
        })
    });
    assert_eq!(
        s.snapshot().document().pose().as_ref().unwrap().groups[0].id,
        id(5)
    );
    assert_eq!(
        s.snapshot().document().pose().as_ref().unwrap().groups[0].entries[0].id,
        id(6)
    );
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), empty.content_id());
    s.redo(s.version()).unwrap();
    edit(&mut s, |c| {
        c.edit_pose(PoseEdit::Duplicate {
            ids: vec![id(6)],
            destination: PoseCollection::Entries { group_id: id(5) },
            before: None,
        })
    });
    let pose = s.snapshot().document().pose().as_ref().unwrap();
    let copy = &pose.groups[1].entries[0];
    assert_ne!(copy.id, id(6));
    assert_ne!(copy.links.as_ref().unwrap()[0].id, id(7));
}
#[test]
fn pose_locks_reject_full_replacement_and_delete_cleans_owned_flags() {
    let mut s = session();
    pose_fixture(&mut s);
    let mut b = s.snapshot().document().builder();
    let mut editor = b.editor_state().clone();
    editor.objects.insert(
        id(7),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: true,
        },
    );
    b.set_editor_state(editor).unwrap();
    let mut s = AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
    );
    let mut d = s.begin_draft();
    assert_eq!(
        d.batch(d.source(), |c| c.replace_pose(None))
            .unwrap_err()
            .code,
        "OBJECT_LOCKED"
    );
    let mut b = s.snapshot().document().builder();
    let mut editor = b.editor_state().clone();
    editor.objects.get_mut(&id(7)).unwrap().locked = false;
    b.set_editor_state(editor).unwrap();
    s = AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
    );
    edit(&mut s, |c| {
        c.edit_pose(PoseEdit::Delete { ids: vec![id(6)] })
    });
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .get(&id(7))
        .is_none());
    s.undo(s.version()).unwrap();
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&id(7))
            .hidden_in_editor
    );
}
fn physics_fixture(s: &mut AuthoringSession) {
    edit(s, |c| {
        for n in [11, 12] {
            c.edit_physics(PhysicsEdit::CreateGroup {
                asset_id: id(10),
                id: id(n),
                name: format!("Rig {n}"),
                before: None,
            })?;
        }
        c.edit_physics(PhysicsEdit::CreateInput {
            group_id: id(11),
            id: id(13),
            before: None,
        })
    });
}
#[test]
fn physics_patches_share_untouched_rigs_and_reject_invalid_nodes_atomically() {
    let mut s = session();
    physics_fixture(&mut s);
    let before = s.snapshot().clone();
    edit(&mut s, |c| {
        c.patch_physics(&PhysicsPatch {
            asset_id: id(10),
            asset_changes: vec![],
            rig: Some(PhysicsRigPatch {
                group_id: id(11),
                changes: vec![PhysicsProperty::InputWeight {
                    id: id(13),
                    value: 25.,
                }],
            }),
        })
    });
    let old = before.document().physics().as_ref().unwrap();
    let next = s.snapshot().document().physics().as_ref().unwrap();
    assert!(Arc::ptr_eq(&old.data.settings[1], &next.data.settings[1]));
    assert!(!Arc::ptr_eq(&old.data.settings[0], &next.data.settings[0]));
    assert_eq!(next.data.settings[0].inputs[0].weight, 25.);
    let mut d = s.begin_draft();
    assert!(d
        .batch(d.source(), |c| c.edit_physics(PhysicsEdit::CreateOutput {
            group_id: id(11),
            id: id(14),
            node_id: id(999),
            before: None
        }))
        .is_err());
    assert!(!d.is_active());
    assert_eq!(
        s.snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .data
            .settings[0]
            .outputs
            .len(),
        0
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}
#[test]
fn physics_output_node_references_and_inherited_locks_are_enforced() {
    let mut s = session();
    physics_fixture(&mut s);
    let node = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0]
        .vertices[1]
        .id
        .clone();
    edit(&mut s, |c| {
        c.edit_physics(PhysicsEdit::CreateOutput {
            group_id: id(11),
            id: id(14),
            node_id: node.clone(),
            before: None,
        })
    });
    let mut d = s.begin_draft();
    assert!(d
        .batch(d.source(), |c| c.edit_physics(PhysicsEdit::Delete {
            ids: vec![node],
            node_replacements: Default::default()
        }))
        .is_err());
    let mut b = s.snapshot().document().builder();
    let mut editor = b.editor_state().clone();
    editor.objects.insert(
        id(11),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    b.set_editor_state(editor).unwrap();
    let s = AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
    );
    let mut d = s.begin_draft();
    assert_eq!(
        d.batch(d.source(), |c| c.patch_physics(&PhysicsPatch {
            asset_id: id(10),
            asset_changes: vec![],
            rig: Some(PhysicsRigPatch {
                group_id: id(11),
                changes: vec![PhysicsProperty::InputWeight {
                    id: id(13),
                    value: 10.
                }]
            })
        }))
        .unwrap_err()
        .code,
        "OBJECT_LOCKED"
    );
}
#[test]
fn motion_library_retains_last_registration_and_tracks_history() {
    let mut s = session();
    let before = s.snapshot().clone();
    edit(&mut s, |c| {
        c.edit_motion(MotionEdit::CreateGroup {
            id: id(20),
            name: "Group".into(),
        })?;
        c.edit_motion(MotionEdit::Create {
            id: id(21),
            name: "Clip".into(),
            registration_id: id(22),
            destination: MotionContainer::Group(id(20)),
        })
    });
    assert_eq!(
        s.snapshot().document().motion_groups()[0].entries[0].id,
        id(22)
    );
    edit(&mut s, |c| {
        c.edit_motion(MotionEdit::DeleteGroup { id: id(20) })
    });
    assert_eq!(s.snapshot().document().motion_roots()[0].id, id(22));
    assert!(s.snapshot().document().motion(&id(21)).is_some());
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}

#[test]
fn motion_track_transfers_preserve_owned_flags_and_copies_remap_keys() {
    let mut s = session();
    edit(&mut s, |c| {
        for n in [30, 31] {
            c.edit_motion(MotionEdit::Create {
                id: id(n),
                name: format!("Clip {n}"),
                registration_id: id(n + 10),
                destination: MotionContainer::Root,
            })?;
        }
        c.edit_motion(MotionEdit::CreateTrack {
            clip: id(30),
            id: id(32),
            key_id: id(33),
            time: 0.,
        })?;
        c.edit_motion(MotionEdit::SetTrackTarget {
            id: id(32),
            target: MotionTrackTarget::PartOpacity { part_id: id(2) },
        })
    });
    let mut editor = s.snapshot().document().editor_state().clone();
    editor.objects.insert(
        id(33),
        ObjectEditorState {
            locked: false,
            hidden_in_editor: true,
        },
    );
    edit(&mut s, |c| c.replace_editor_state(editor));
    edit(&mut s, |c| {
        c.edit_motion(MotionEdit::MoveTracks {
            ids: vec![id(32)],
            destination: id(31),
            before: None,
            copy: false,
        })
    });
    let doc = s.snapshot().document();
    assert!(doc.motion(&id(30)).unwrap().tracks.is_empty());
    assert_eq!(doc.motion(&id(31)).unwrap().tracks[0].initial.id, id(33));
    assert!(doc.editor_state().get(&id(33)).hidden_in_editor);
    edit(&mut s, |c| {
        c.edit_motion(MotionEdit::MoveTracks {
            ids: vec![id(32)],
            destination: id(30),
            before: None,
            copy: true,
        })
    });
    let doc = s.snapshot().document();
    let copied = &doc.motion(&id(30)).unwrap().tracks[0];
    assert_ne!(copied.id, id(32));
    assert_ne!(copied.initial.id, id(33));
    assert_eq!(
        copied.initial.point(),
        doc.motion(&id(31)).unwrap().tracks[0].initial.point()
    );
    edit(&mut s, |c| {
        c.edit_motion(MotionEdit::DeleteTracks { ids: vec![id(32)] })
    });
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&id(33)));
    s.undo(s.version()).unwrap();
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&id(33))
            .hidden_in_editor
    );
}
