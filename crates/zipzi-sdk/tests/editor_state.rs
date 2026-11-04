use std::sync::Arc;
use zipzi_core::{
    document::{EditorState, ObjectEditorState},
    Canvas, Part, Transform, Vec2,
};
use zipzi_project::{decode_project, encode_project};
use zipzi_sdk::AuthoringSession;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    let mut s =
        AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    s.edit("Fixture", None, |e| {
        e.create_part(Part {
            id: id(2),
            enabled: false,
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(3),
            ..Default::default()
        })
    })
    .unwrap();
    s
}

#[test]
fn flag_receipts_exclude_unchanged_peer_flags() {
    let mut s = session();
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id(3),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("Lock peer", None, |e| e.replace_editor_state(state.clone()))
        .unwrap();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    let (_, receipt) = s
        .edit("Lock Part", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert_eq!(receipt.object_ids, vec![id(2)]);
    // A Part and its DrawOrderGroup share an ID and may both be reported.
    assert!(receipt
        .changes
        .modified
        .iter()
        .all(|entity| entity.id == id(2)));
    assert!(receipt
        .changes
        .modified
        .iter()
        .any(|entity| entity.kind == zipzi_sdk::ObjectKind::Part));
    s.undo().unwrap();
    assert!(!s.document().editor_state().get(&id(2)).locked);
    assert!(s.document().editor_state().get(&id(3)).locked);
    s.redo().unwrap();
    assert!(s.document().editor_state().get(&id(2)).locked);
}

#[test]
fn flag_receipts_exclude_unchanged_bone_preferences() {
    let mut s = session();
    let mut state = s.document().editor_state().clone();
    state.rotation_bone_lengths.insert(id(3), 192.);
    state.rotation_bone_reference_scales.insert(id(3), 1.);
    state.rotation_bone_angle_offsets.insert(id(3), 90.);
    s.edit("Bone preferences", None, |e| {
        e.replace_editor_state(state.clone())
    })
    .unwrap();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    let (_, receipt) = s
        .edit("Lock Part", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert_eq!(receipt.object_ids, vec![id(2)]);
    // A Part and its DrawOrderGroup share an ID and may both be reported.
    assert!(receipt
        .changes
        .modified
        .iter()
        .all(|entity| entity.id == id(2)));
    assert!(receipt
        .changes
        .modified
        .iter()
        .any(|entity| entity.kind == zipzi_sdk::ObjectKind::Part));
    s.undo().unwrap();
    assert!(!s.document().editor_state().get(&id(2)).locked);
    assert_eq!(
        s.document().editor_state().rotation_bone_lengths[&id(3)],
        192.
    );
    s.redo().unwrap();
    assert!(s.document().editor_state().get(&id(2)).locked);
}

#[test]
fn unified_flags_are_atomic_sparse_metadata_with_history_and_cleanup() {
    let mut s = session();
    let frame = s.preview_frame().unwrap();
    let baseline = s.document().editor_state().clone();
    let mut state = baseline.clone();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: true,
        },
    );
    state.objects.insert(id(3), ObjectEditorState::default());
    s.edit("Hide and lock", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert_eq!(s.document().editor_state().objects.len(), 1);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let bytes = encode_project(s.document()).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(wire["format_version"], 21);
    assert!(wire["document"].get("object_locks").is_none());
    assert!(wire["document"].get("deformer_display").is_none());
    let loaded = decode_project(&bytes).unwrap();
    assert_eq!(loaded.editor_state(), s.document().editor_state());
    assert!(!loaded.get_part(&id(2)).unwrap().enabled);
    s.undo().unwrap();
    assert_eq!(s.document().editor_state(), &baseline);
    s.redo().unwrap();
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let history = s.history_lengths();
    let mut invalid = s.document().editor_state().clone();
    invalid.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    assert!(s
        .edit("Invalid", None, |e| e.replace_editor_state(invalid))
        .is_err());
    assert_eq!(s.history_lengths(), history);
    assert_eq!(
        s.edit("Erase locked", None, |e| e.erase_object(&id(2)))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    let mut unlocked = s.document().editor_state().clone();
    unlocked.objects.get_mut(&id(2)).unwrap().locked = false;
    s.edit("Unlock", None, |e| e.replace_editor_state(unlocked))
        .unwrap();
    s.edit("Erase", None, |e| e.erase_object(&id(2))).unwrap();
    assert_eq!(s.document().editor_state(), &baseline);
    s.undo().unwrap();
    assert!(s.document().editor_state().get(&id(2)).hidden_in_editor);
    assert!(!s.document().editor_state().get(&id(2)).locked);
}

#[test]
fn retired_fields_and_malformed_editor_state_are_rejected() {
    let s = session();
    let mut wire: serde_json::Value =
        serde_json::from_str(&encode_project(s.document()).unwrap()).unwrap();
    for (field, value) in [
        ("object_locks", serde_json::json!({"objects":[id(2)]})),
        ("deformer_display", serde_json::json!({"hidden":[id(3)]})),
    ] {
        wire["document"][field] = value;
        assert!(decode_project(&wire.to_string()).is_err());
        wire["document"].as_object_mut().unwrap().remove(field);
    }
    for invalid in [
        serde_json::json!(null),
        serde_json::json!({"objects":{id(99):{"locked":true}}}),
        serde_json::json!({"objects":{id(2):{"enabled":false}}}),
        serde_json::json!({"objects":{id(2):{"hidden_in_editor":"false"}}}),
    ] {
        wire["document"]["editor_state"] = invalid;
        assert!(decode_project(&wire.to_string()).is_err());
    }
    wire["document"]["editor_state"] = serde_json::json!({"objects":{id(2):{"locked":true}}});
    wire["format_version"] = 9.into();
    assert!(decode_project(&wire.to_string()).is_err());
}

#[test]
fn noops_and_invalid_batches_preserve_redo_and_runtime_cache() {
    let mut s = session();
    let frame = s.preview_frame().unwrap();
    let bytes = s.estimated_content_bytes();
    let baseline = s.document().editor_state().clone();
    let mut state = baseline.clone();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: true,
        },
    );
    s.edit("Editor flags", None, |e| {
        e.replace_editor_state(state.clone())
    })
    .unwrap();
    assert!(s.estimated_content_bytes() > bytes);
    s.undo().unwrap();
    let history = s.history_lengths();
    let version = s.version();
    let mut invalid = state.clone();
    invalid.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    assert!(s
        .edit("Invalid batch", None, |e| e.replace_editor_state(invalid))
        .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    // Explicit defaults canonicalize to the same empty state and preserve redo.
    let defaults = EditorState {
        objects: [(id(2), ObjectEditorState::default())].into(),
        ..baseline
    };
    assert!(
        !s.edit("No-op", None, |e| e.replace_editor_state(defaults))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.history_lengths(), history);
    s.redo().unwrap();
    let history = s.history_lengths();
    assert!(
        !s.edit("No-op", None, |e| e.replace_editor_state(state))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.history_lengths(), history);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
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
    s.save_project(&path, None).unwrap();
    assert!(!s.document().modified());
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
        ..s.document().editor_state().clone()
    };
    s.edit("Editor state", None, |e| {
        e.replace_editor_state(state.clone())
    })
    .unwrap();
    assert!(s.document().modified());
    let saved = s.save_project(&path, None).unwrap();
    assert!(!s.document().modified());
    s.undo().unwrap();
    assert!(s.document().modified());
    s.redo().unwrap();
    assert!(!s.document().modified());
    let mut reopened = session();
    reopened.open_project(&saved.path, None).unwrap();
    assert_eq!(reopened.document().editor_state(), &state);
}

#[test]
fn rotation_bone_lengths_roundtrip_without_runtime_changes_and_clean_up_with_transforms() {
    let mut s = session();
    let baseline = s.document().editor_state().clone();
    let frame = s.preview_frame().unwrap();
    let mut state = s.document().editor_state().clone();
    state.rotation_bone_lengths.insert(id(3), 192.);
    state.rotation_bone_angle_offsets.insert(id(3), 90.);
    state
        .rotation_bone_reference_scales
        .insert(id(3), 1. / 3749.);
    let (_, receipt) = s
        .edit("Bone length", None, |e| {
            e.replace_editor_state(state.clone())
        })
        .unwrap();
    assert!(receipt
        .changes
        .categories
        .contains(&zipzi_sdk::ChangeCategory::EditorState));
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let encoded = encode_project(s.document()).unwrap();
    assert_eq!(decode_project(&encoded).unwrap().editor_state(), &state);
    s.undo().unwrap();
    assert_eq!(s.document().editor_state(), &baseline);
    s.redo().unwrap();
    assert_eq!(s.document().editor_state(), &state);
    let history = s.history_lengths();
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
        assert!(s
            .edit("Invalid bone length", None, |e| e
                .replace_editor_state(invalid))
            .is_err());
        assert_eq!(s.history_lengths(), history);
        assert_eq!(s.document().editor_state(), &state);
        let mut invalid = state.clone();
        invalid.rotation_bone_reference_scales.insert(target, value);
        assert!(s
            .edit("Invalid bone reference", None, |e| e
                .replace_editor_state(invalid))
            .is_err());
        assert_eq!(s.history_lengths(), history);
        assert_eq!(s.document().editor_state(), &state);
    }
    for (target, value) in [
        (id(3), f32::NAN),
        (id(3), f32::INFINITY),
        (id(2), 90.),
        (id(99), 90.),
    ] {
        let mut invalid = state.clone();
        invalid.rotation_bone_angle_offsets.insert(target, value);
        assert!(s
            .edit("Invalid bone direction", None, |e| e
                .replace_editor_state(invalid))
            .is_err());
        assert_eq!(s.history_lengths(), history);
        assert_eq!(s.document().editor_state(), &state);
    }
    // Direct erasure and type replacement cannot leave dangling display preferences.
    s.edit("Erase rotation", None, |e| e.erase_object(&id(3)))
        .unwrap();
    assert!(s.document().editor_state().rotation_bone_lengths.is_empty());
    assert!(s
        .document()
        .editor_state()
        .rotation_bone_reference_scales
        .is_empty());
    assert!(s
        .document()
        .editor_state()
        .rotation_bone_angle_offsets
        .is_empty());
    s.undo().unwrap();
    assert_eq!(s.document().editor_state(), &state);
    let mut warp = s.document().get_transform(&id(3)).unwrap().clone();
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
    s.edit("Convert to Warp", None, |e| e.replace_transform(warp))
        .unwrap();
    assert!(s.document().editor_state().rotation_bone_lengths.is_empty());
    assert!(s
        .document()
        .editor_state()
        .rotation_bone_reference_scales
        .is_empty());
    assert!(s
        .document()
        .editor_state()
        .rotation_bone_angle_offsets
        .is_empty());
    s.undo().unwrap();
    assert_eq!(s.document().editor_state(), &state);
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
        let mut rotation = s.document().get_transform(&id(3)).unwrap().clone();
        rotation.rotation_mut().unwrap().pose.scale = 2.;
        s.edit("Scale rotation", None, |e| e.replace_transform(rotation))
            .unwrap();
        let state = s.document().editor_state().clone();
        assert_eq!(state.rotation_bone_reference_scales[&id(3)], 1.);
        if manual {
            let frame = s.preview_frame().unwrap();
            let plan = s
                .plan_scene_content(SceneContentEdit::DeformerOriginal {
                    transform_id: id(3),
                    target: ShapeTarget::Base,
                    action: DeformerOriginalAction::Update,
                    context: Default::default(),
                })
                .unwrap();
            s.execute_scene_content_plan(&plan).unwrap();
            assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
        } else {
            let pose = s
                .document()
                .get_transform(&id(3))
                .unwrap()
                .rotation()
                .unwrap()
                .pose;
            s.edit("First rotation keys", None, |e| {
                e.create_parameter(Parameter {
                    id: id(4),
                    ..Default::default()
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
            .unwrap();
        }
        let snapshot = s
            .document()
            .get_transform(&id(3))
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
        assert_eq!(s.document().rotation_bone_reference_scale(&id(3)), Some(1.));
        assert_eq!(s.document().editor_state(), &state);
        s.undo().unwrap();
        assert_eq!(s.document().editor_state(), &state);
        s.redo().unwrap();
        let reopened = decode_project(&encode_project(s.document()).unwrap()).unwrap();
        assert_eq!(reopened.editor_state(), &state);
        assert_eq!(reopened.rotation_bone_reference_scale(&id(3)), Some(1.));
    }
}

#[test]
fn rotation_bone_copy_preserves_preferences_through_scene_and_part_duplication() {
    use zipzi_core::document::{OrganizationCollection, OrganizationEdit};
    for copy_part in [false, true] {
        let mut s = session();
        let mut rotation = s.document().get_transform(&id(3)).unwrap().clone();
        rotation.part_id = Some(id(2).into());
        s.edit("Organize rotation", None, |e| e.replace_transform(rotation))
            .unwrap();
        let mut state = s.document().editor_state().clone();
        state.rotation_bone_lengths.insert(id(3), 192.);
        state.rotation_bone_reference_scales.insert(id(3), 0.25);
        state.rotation_bone_angle_offsets.insert(id(3), 90.);
        s.edit("Bone preferences", None, |e| {
            e.replace_editor_state(state.clone())
        })
        .unwrap();
        let plan = s
            .plan_organization_edit(OrganizationEdit::Duplicate {
                ids: vec![id(if copy_part { 2 } else { 3 })],
                destination: OrganizationCollection::PartMembers { part_id: None },
                before: None,
            })
            .unwrap();
        let copied = plan.copied_ids()[&id(3)].clone();
        s.execute_organization_plan(&plan).unwrap();
        let mut expected = state.clone();
        expected.rotation_bone_lengths.insert(copied.clone(), 192.);
        expected
            .rotation_bone_reference_scales
            .insert(copied.clone(), 0.25);
        expected
            .rotation_bone_angle_offsets
            .insert(copied.clone(), 90.);
        assert_eq!(s.document().editor_state(), &expected);
        s.undo().unwrap();
        assert_eq!(s.document().editor_state(), &state);
        assert!(s.document().get_transform(&copied).is_none());
        s.redo().unwrap();
        assert_eq!(s.document().editor_state(), &expected);
        assert_eq!(
            decode_project(&encode_project(s.document()).unwrap())
                .unwrap()
                .editor_state(),
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
        let mut s =
            AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
        if scene_creation {
            let plan = s
                .plan_scene_edit(SceneEdit::Create {
                    id: id(3),
                    name: "Rotation".into(),
                    object: SceneCreate::Rotation { origin: None },
                    part_id: None,
                    parent_id: None,
                    before: None,
                    deformation_before: None,
                })
                .unwrap();
            s.execute_scene_plan(&plan).unwrap();
        } else {
            s.edit("Create empty rotation", None, |e| {
                e.create_transform(Transform {
                    id: id(3),
                    ..Default::default()
                })
            })
            .unwrap();
        }
        let state = s.document().editor_state().clone();
        assert_eq!(state.rotation_bone_lengths[&id(3)], 128.);
        s.edit("Later descendant", None, |e| {
            e.create_mesh(Mesh {
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
            })
        })
        .unwrap();
        assert_eq!(s.document().editor_state(), &state);
        let reopened = decode_project(&encode_project(s.document()).unwrap()).unwrap();
        assert_eq!(reopened.editor_state(), &state);
        s.undo().unwrap();
        assert_eq!(s.document().editor_state(), &state);
        s.redo().unwrap();
        assert_eq!(s.document().editor_state(), &state);
    }
}

#[test]
fn imported_rotation_bone_freezes_fallback_when_bounds_are_degenerate_or_missed() {
    use zipzi_core::{Document, Mesh};
    for points in [
        [Vec2::new(-100., 0.), Vec2::new(0., 0.), Vec2::new(100., 0.)],
        [
            Vec2::new(100., 100.),
            Vec2::new(200., 100.),
            Vec2::new(100., 200.),
        ],
    ] {
        let mut model = Document::new();
        assert!(model
            .initialize(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))
            .is_ok());
        assert!(model
            .create_imported_transform(Transform {
                id: id(3),
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(model
            .create_mesh(Mesh {
                id: id(4),
                deformer_id: id(3),
                vertex_ids: vec![0, 1, 2],
                base_positions: points.into(),
                uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(model
            .initialize_rotation_bone_reference_scales()
            .status
            .is_ok());
        let state = model.editor_state().clone();
        assert_eq!(state.rotation_bone_lengths[&id(3)], 128.);
        assert_eq!(state.rotation_bone_angle_offsets[&id(3)], 90.);
        let mut mesh = model.get_mesh(&id(4)).unwrap().clone();
        mesh.base_positions = vec![
            Vec2::new(-100., -20.),
            Vec2::new(100., -20.),
            Vec2::new(0., 100.),
        ];
        assert!(model.replace_mesh(mesh).status.is_ok());
        assert!(model
            .initialize_rotation_bone_reference_scales()
            .status
            .is_ok());
        assert_eq!(model.editor_state(), &state);
        assert_eq!(
            decode_project(&encode_project(&model).unwrap())
                .unwrap()
                .editor_state(),
            &state
        );
    }
}
