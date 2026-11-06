use crate::support::{archive, roundtrip};
use zipzi_core::state::{DocumentState, RootDomain};
use zipzi_core::{
    document::{EditorState, ObjectEditorState},
    Canvas, Part, Transform, Vec2,
};
use zipzi_sdk::AuthoringSession;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(2),
                        enabled: false,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(3),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}

#[test]
fn flag_receipts_exclude_unchanged_peer_flags() {
    let mut s = session();
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id(3),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state.clone()))
            .and_then(|value| {
                let prepared = draft.prepare("Lock peer")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock Part")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.root_changed(RootDomain::EditorState));
    assert!(receipt.writes.records().is_empty());
    s.undo(s.version()).unwrap();
    assert!(!s.snapshot().document().editor_state().get(&id(2)).locked);
    assert!(s.snapshot().document().editor_state().get(&id(3)).locked);
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().editor_state().get(&id(2)).locked);
}

#[test]
fn flag_receipts_exclude_unchanged_bone_preferences() {
    let mut s = session();
    let mut state = s.snapshot().document().editor_state().clone();
    state.rotation_bone_lengths.insert(id(3), 192.);
    state.rotation_bone_reference_scales.insert(id(3), 1.);
    state.rotation_bone_angle_offsets.insert(id(3), 90.);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state.clone()))
            .and_then(|value| {
                let prepared = draft.prepare("Bone preferences")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock Part")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.root_changed(RootDomain::EditorState));
    assert!(receipt.writes.records().is_empty());
    s.undo(s.version()).unwrap();
    assert!(!s.snapshot().document().editor_state().get(&id(2)).locked);
    assert_eq!(
        s.snapshot().document().editor_state().rotation_bone_lengths[&id(3)],
        192.
    );
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().editor_state().get(&id(2)).locked);
}

#[test]
fn unified_flags_are_atomic_sparse_metadata_with_history_and_cleanup() {
    let mut s = session();
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let baseline = s.snapshot().document().editor_state().clone();
    let mut state = baseline.clone();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: true,
        },
    );
    state.objects.insert(id(3), ObjectEditorState::default());
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Hide and lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.snapshot().document().editor_state().objects.len(), 1);
    assert_eq!(
        frame.drawables,
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables
    );
    let bytes =
        archive::native_bytes_with_resources(s.snapshot().document(), s.snapshot().resources());
    let wire = archive::project_json(&bytes);
    assert_eq!(wire["format_version"], 22);
    assert!(wire["document"].get("object_locks").is_none());
    assert!(wire["document"].get("deformer_display").is_none());
    let loaded = archive::load_native(&bytes).unwrap();
    assert_eq!(
        loaded.editor_state(),
        s.snapshot().document().editor_state()
    );
    assert!(!loaded.part(&id(2)).unwrap().enabled);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().editor_state(), &baseline);
    s.redo(s.version()).unwrap();
    assert_eq!(
        frame.drawables,
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables
    );
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let mut invalid = s.snapshot().document().editor_state().clone();
    invalid.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(invalid))
            .and_then(|value| {
                let prepared = draft.prepare("Invalid")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    let state = e.read_document()?;
                    let target = state.deletion_target(
                        if id(2) == id(2) {
                            zipzi_contract::authoring::ObjectKind::Part
                        } else {
                            zipzi_contract::authoring::ObjectKind::Transform
                        },
                        &id(2),
                    )?;
                    e.edit_deletion(
                        &zipzi_core::document::DeletionRequest {
                            roots: vec![target],
                            ..Default::default()
                        },
                        Default::default(),
                        &Default::default(),
                    )
                    .map(|_| ())
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Erase locked")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    let mut unlocked = s.snapshot().document().editor_state().clone();
    unlocked.objects.get_mut(&id(2)).unwrap().locked = false;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(unlocked))
            .and_then(|value| {
                let prepared = draft.prepare("Unlock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let state = e.read_document()?;
                let target = state.deletion_target(
                    if id(2) == id(2) {
                        zipzi_contract::authoring::ObjectKind::Part
                    } else {
                        zipzi_contract::authoring::ObjectKind::Transform
                    },
                    &id(2),
                )?;
                e.edit_deletion(
                    &zipzi_core::document::DeletionRequest {
                        roots: vec![target],
                        ..Default::default()
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Erase")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.snapshot().document().editor_state(), &baseline);
    s.undo(s.version()).unwrap();
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&id(2))
            .hidden_in_editor
    );
    assert!(!s.snapshot().document().editor_state().get(&id(2)).locked);
}

#[test]
fn retired_fields_and_malformed_editor_state_are_rejected() {
    let s = session();
    let bytes = archive::native_bytes(s.snapshot().document());
    let mut wire = archive::project_json(&bytes);
    for (field, value) in [
        ("object_locks", serde_json::json!({"objects":[id(2)]})),
        ("deformer_display", serde_json::json!({"hidden":[id(3)]})),
    ] {
        wire["document"][field] = value;
        assert!(archive::load_native(&archive::replace_project_json(&bytes, &wire)).is_err());
        wire["document"].as_object_mut().unwrap().remove(field);
    }
    for invalid in [
        serde_json::json!(null),
        serde_json::json!({"objects":{id(99):{"locked":true}}}),
        serde_json::json!({"objects":{id(2):{"enabled":false}}}),
        serde_json::json!({"objects":{id(2):{"hidden_in_editor":"false"}}}),
    ] {
        wire["document"]["editor_state"] = invalid;
        assert!(archive::load_native(&archive::replace_project_json(&bytes, &wire)).is_err());
    }
    wire["document"]["editor_state"] = serde_json::json!({"objects":{id(2):{"locked":true}}});
    wire["format_version"] = 9.into();
    assert!(archive::load_native(&archive::replace_project_json(&bytes, &wire)).is_err());
}

#[test]
fn noops_and_invalid_batches_preserve_redo_and_runtime_cache() {
    let mut s = session();
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let before = s.snapshot().content_id();
    let baseline = s.snapshot().document().editor_state().clone();
    let mut state = baseline.clone();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: true,
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state.clone()))
            .and_then(|value| {
                let prepared = draft.prepare("Editor flags")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_ne!(s.snapshot().content_id(), before);
    s.undo(s.version()).unwrap();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let version = s.version();
    let mut invalid = state.clone();
    invalid.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(invalid))
            .and_then(|value| {
                let prepared = draft.prepare("Invalid batch")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    // Explicit defaults canonicalize to the same empty state and preserve redo.
    let defaults = EditorState {
        objects: [(id(2), ObjectEditorState::default())].into(),
        ..baseline
    };
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(defaults))
            .and_then(|value| {
                let prepared = draft.prepare("No-op")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap()
    .1
    .writes
    .is_empty());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    s.redo(s.version()).unwrap();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("No-op")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap()
    .1
    .writes
    .is_empty());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert_eq!(
        frame.drawables,
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables
    );
}

#[test]
fn saving_both_flags_roundtrips_and_establishes_history_clean_baseline() {
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = Temp(
        std::env::temp_dir().join(format!("zipzi-editor-state-{}-{nonce}", std::process::id())),
    );
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("state.zipzi");
    let mut s = session();
    let io = zipzi_project::ProjectIo::new(&dir.0, Default::default());
    s.save_as(&io, &path, None, || true).unwrap();
    assert!(!s.is_dirty());
    let state = EditorState {
        objects: [(
            id(3),
            ObjectEditorState {
                locked: true,
                hidden_in_editor: true,
            },
        )]
        .into(),
        rotation_bone_lengths: [(id(3), 192.)].into(),
        ..s.snapshot().document().editor_state().clone()
    };
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state.clone()))
            .and_then(|value| {
                let prepared = draft.prepare("Editor state")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(s.is_dirty());
    s.save(&io, || true).unwrap();
    assert!(!s.is_dirty());
    s.undo(s.version()).unwrap();
    assert!(s.is_dirty());
    s.redo(s.version()).unwrap();
    assert!(!s.is_dirty());
    let (reopened, _) = AuthoringSession::open(&io, &path, || true).unwrap();
    assert_eq!(reopened.snapshot().document().editor_state(), &state);
}

#[test]
fn rotation_bone_lengths_roundtrip_without_runtime_changes_and_clean_up_with_transforms() {
    let mut s = session();
    let baseline = s.snapshot().document().editor_state().clone();
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let mut state = s.snapshot().document().editor_state().clone();
    state.rotation_bone_lengths.insert(id(3), 192.);
    state.rotation_bone_angle_offsets.insert(id(3), 90.);
    state
        .rotation_bone_reference_scales
        .insert(id(3), 1. / 3749.);
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state.clone()))
            .and_then(|value| {
                let prepared = draft.prepare("Bone length")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.root_changed(RootDomain::EditorState));
    assert_eq!(
        frame.drawables,
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables
    );
    assert_eq!(roundtrip(s.snapshot()).document().editor_state(), &state);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().editor_state(), &baseline);
    s.redo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().editor_state(), &state);
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    for (target, value) in [
        (id(3), 0.),
        (id(3), -1.),
        (id(3), f32::NAN),
        (id(3), f32::INFINITY),
        (id(2), 128.),
        (id(99), 128.),
    ] {
        let mut invalid = state.clone();
        invalid.rotation_bone_lengths.insert(target.clone(), value);
        assert!({
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_editor_state(invalid))
                .and_then(|value| {
                    let prepared = draft.prepare("Invalid bone length")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .is_err());
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            history
        );
        assert_eq!(s.snapshot().document().editor_state(), &state);
        let mut invalid = state.clone();
        invalid.rotation_bone_reference_scales.insert(target, value);
        assert!({
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_editor_state(invalid))
                .and_then(|value| {
                    let prepared = draft.prepare("Invalid bone reference")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .is_err());
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            history
        );
        assert_eq!(s.snapshot().document().editor_state(), &state);
    }
    for (target, value) in [
        (id(3), f32::NAN),
        (id(3), f32::INFINITY),
        (id(2), 90.),
        (id(99), 90.),
    ] {
        let mut invalid = state.clone();
        invalid.rotation_bone_angle_offsets.insert(target, value);
        assert!({
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_editor_state(invalid))
                .and_then(|value| {
                    let prepared = draft.prepare("Invalid bone direction")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .is_err());
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            history
        );
        assert_eq!(s.snapshot().document().editor_state(), &state);
    }
    // Direct erasure and type replacement cannot leave dangling display preferences.
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let state = e.read_document()?;
                let target = state.deletion_target(
                    if id(3) == id(2) {
                        zipzi_contract::authoring::ObjectKind::Part
                    } else {
                        zipzi_contract::authoring::ObjectKind::Transform
                    },
                    &id(3),
                )?;
                e.edit_deletion(
                    &zipzi_core::document::DeletionRequest {
                        roots: vec![target],
                        ..Default::default()
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Erase rotation")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_lengths
        .is_empty());
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_reference_scales
        .is_empty());
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_angle_offsets
        .is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().editor_state(), &state);
    let mut warp = s.snapshot().document().transform(&id(3)).unwrap().clone();
    warp.data = zipzi_core::TransformData::Warp(zipzi_core::WarpTransform {
        rows: 1,
        columns: 1,
        quad: true,
        bezier: None,
        points: vec![
            Vec2::new(0., 0.),
            Vec2::new(100., 0.),
            Vec2::new(0., 100.),
            Vec2::new(100., 100.),
        ],
    });
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_transform(warp))
            .and_then(|value| {
                let prepared = draft.prepare("Convert to Warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_lengths
        .is_empty());
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_reference_scales
        .is_empty());
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_angle_offsets
        .is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().editor_state(), &state);
}

#[test]
fn rotation_bone_reference_is_frozen_across_manual_and_first_key_original_updates() {
    use zipzi_core::{
        document::{SceneContentEdit, ShapeTarget},
        BindingAxis, BindingAxisEditing, DeformerOriginalAction, DeformerOriginalShape,
        DeformerOriginalSource, Parameter, RotationKeyform, SceneBinding, SceneTrack,
    };
    for manual in [false, true] {
        let mut s = session();
        let mut rotation = s.snapshot().document().transform(&id(3)).unwrap().clone();
        rotation.rotation_mut().unwrap().pose.scale = 2.;
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_transform(rotation))
                .and_then(|value| {
                    let prepared = draft.prepare("Scale rotation")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let state = s.snapshot().document().editor_state().clone();
        assert_eq!(state.rotation_bone_reference_scales[&id(3)], 1.);
        if manual {
            let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
            let plan = crate::support::prepared(&s, |candidate_commands| {
                candidate_commands.edit_scene_content(
                    SceneContentEdit::DeformerOriginal {
                        transform_id: id(3),
                        target: ShapeTarget::Base,
                        action: DeformerOriginalAction::Update,
                        context: Default::default(),
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .unwrap();
            s.publish(plan.1).unwrap();
            assert_eq!(
                frame.drawables,
                crate::support::frame(s.snapshot(), &Default::default())
                    .unwrap()
                    .drawables
            );
        } else {
            let pose = s
                .snapshot()
                .document()
                .transform(&id(3))
                .unwrap()
                .rotation()
                .unwrap()
                .pose;
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        e.create_parameter({
                            let mut value = Parameter {
                                id: id(4),
                                ..Default::default()
                            };
                            value.runtime_id = value.id.clone();
                            value
                        })?;
                        e.create_scene_binding(SceneBinding {
                            id: id(5),
                            axes: vec![BindingAxis::new(id(4), vec![-1., 1.])],
                            track: SceneTrack::Rotation {
                                target_id: id(3).into(),
                                keyforms: [-1., 1.]
                                    .into_iter()
                                    .map(|key| RotationKeyform {
                                        keys: vec![key],
                                        rotation: pose,
                                        ..Default::default()
                                    })
                                    .collect(),
                            },
                        })
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("First rotation keys")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
        }
        let snapshot = s
            .snapshot()
            .document()
            .transform(&id(3))
            .unwrap()
            .original_shape
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap();
        assert_eq!(
            snapshot.source,
            if manual {
                DeformerOriginalSource::Manual
            } else {
                DeformerOriginalSource::FirstKey
            }
        );
        let DeformerOriginalShape::Rotation { pose } = &snapshot.shape else {
            panic!("expected rotation original")
        };
        assert_eq!(pose.scale, 2.);
        assert_eq!(
            s.snapshot()
                .document()
                .rotation_bone_reference_scale(&id(3)),
            Some(1.)
        );
        assert_eq!(s.snapshot().document().editor_state(), &state);
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().editor_state(), &state);
        s.redo(s.version()).unwrap();
        let reopened = roundtrip(s.snapshot()).document().clone();
        assert_eq!(reopened.editor_state(), &state);
        assert_eq!(reopened.rotation_bone_reference_scale(&id(3)), Some(1.));
    }
}

#[test]
fn rotation_bone_copy_preserves_preferences_through_scene_and_part_duplication() {
    use zipzi_core::document::{OrganizationCollection, OrganizationEdit};
    for copy_part in [false, true] {
        let mut s = session();
        let mut rotation = s.snapshot().document().transform(&id(3)).unwrap().clone();
        rotation.part_id = Some(id(2).into());
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_transform(rotation))
                .and_then(|value| {
                    let prepared = draft.prepare("Organize rotation")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let mut state = s.snapshot().document().editor_state().clone();
        state.rotation_bone_lengths.insert(id(3), 192.);
        state.rotation_bone_reference_scales.insert(id(3), 0.25);
        state.rotation_bone_angle_offsets.insert(id(3), 90.);
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_editor_state(state.clone()))
                .and_then(|value| {
                    let prepared = draft.prepare("Bone preferences")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let plan = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_organization(OrganizationEdit::Duplicate {
                ids: vec![id(if copy_part { 2 } else { 3 })],
                destination: OrganizationCollection::PartMembers { part_id: None },
                before: None,
            })
        })
        .unwrap();
        let copied = plan.0.copied_ids[&id(3)].clone();
        s.publish(plan.1).unwrap();
        let mut expected = state.clone();
        expected.rotation_bone_lengths.insert(copied.clone(), 192.);
        expected
            .rotation_bone_reference_scales
            .insert(copied.clone(), 0.25);
        expected
            .rotation_bone_angle_offsets
            .insert(copied.clone(), 90.);
        assert_eq!(s.snapshot().document().editor_state(), &expected);
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().editor_state(), &state);
        assert!(s.snapshot().document().transform(&copied).is_none());
        s.redo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().editor_state(), &expected);
        assert_eq!(
            roundtrip(s.snapshot()).document().clone().editor_state(),
            &expected
        );
    }
}

#[test]
fn rotation_bone_empty_creation_freezes_fallback_before_later_descendants_and_reopen() {
    use zipzi_core::{
        document::{SceneCreate, SceneEdit},
        Mesh,
    };
    for scene_creation in [false, true] {
        let mut s = AuthoringSession::new(
            zipzi_sdk::Snapshot::new(
                zipzi_core::state::DocumentState::new(
                    &id(1),
                    (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
                )
                .unwrap(),
                zipzi_sdk::ResourceView::default(),
            )
            .unwrap(),
        );
        if scene_creation {
            let plan = crate::support::prepared(&s, |candidate_commands| {
                candidate_commands.edit_scene(
                    SceneEdit::Create {
                        id: id(3),
                        name: "Rotation".into(),
                        object: SceneCreate::Rotation { origin: None },
                        part_id: None,
                        parent_id: None,
                        before: None,
                        deformation_before: None,
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .unwrap();
            s.publish(plan.1).unwrap();
        } else {
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: id(3),
                                ..Default::default()
                            },
                        )
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Create empty rotation")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
        }
        let state = s.snapshot().document().editor_state().clone();
        assert_eq!(state.rotation_bone_lengths[&id(3)], 128.);
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.create_mesh({
                        let mut value = Mesh {
                            id: id(4),
                            deformer_id: id(3),
                            vertex_ids: vec![0, 1, 2],
                            base_positions: vec![
                                Vec2::new(0., -20.),
                                Vec2::new(100., -20.),
                                Vec2::new(0., 20.),
                            ],
                            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                            triangles: vec![[0, 1, 2]],
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Later descendant")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        assert_eq!(s.snapshot().document().editor_state(), &state);
        let reopened = roundtrip(s.snapshot()).document().clone();
        assert_eq!(reopened.editor_state(), &state);
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().editor_state(), &state);
        s.redo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().editor_state(), &state);
    }
}

#[test]
fn imported_rotation_bone_freezes_fallback_when_bounds_are_degenerate_or_missed() {
    use zipzi_core::Mesh;
    for points in [
        [Vec2::new(-100., 0.), Vec2::new(0., 0.), Vec2::new(100., 0.)],
        [
            Vec2::new(100., 100.),
            Vec2::new(200., 100.),
            Vec2::new(100., 200.),
        ],
    ] {
        let mut model =
            DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 1.).into())
                .unwrap()
                .builder();
        assert!(model
            .create_transform(Transform {
                id: id(3),
                runtime_id: id(3),
                ..Default::default()
            })
            .is_ok());
        assert!(model
            .create_mesh({
                let mut value = Mesh {
                    id: id(4),
                    deformer_id: id(3),
                    vertex_ids: vec![0, 1, 2],
                    base_positions: points.into(),
                    uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    triangles: vec![[0, 1, 2]],
                    ..Default::default()
                };
                value.runtime_id = value.id.clone();
                value
            })
            .is_ok());
        assert!(model.initialize_rotation_bone_reference_scales().is_ok());
        let state = model.editor_state().clone();
        assert_eq!(state.rotation_bone_lengths[&id(3)], 128.);
        assert_eq!(state.rotation_bone_angle_offsets[&id(3)], 90.);
        let mut mesh = model.mesh(&id(4)).unwrap().clone();
        mesh.base_positions = vec![
            Vec2::new(-100., -20.),
            Vec2::new(100., -20.),
            Vec2::new(0., 100.),
        ];
        assert!(model.replace_mesh(mesh).is_ok());
        assert!(model.initialize_rotation_bone_reference_scales().is_ok());
        assert_eq!(model.editor_state(), &state);
        assert_eq!(
            archive::roundtrip(&model.freeze().unwrap().state).editor_state(),
            &state
        );
    }
}
