use zipzi_core::document::{
    AttachmentEdit as A, AttachmentRef, MotionContainer as C, MotionEdit as E,
    MotionTrackTarget as T,
};
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_sdk::{AuthoringSession, ChangeCategory, ObjectKind};
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 10.)).unwrap()
}
fn apply(s: &mut AuthoringSession, e: E) -> zipzi_sdk::EditReceipt {
    let v = s.version();
    let h = s.history_lengths();
    let p = s.plan_motion_edit(e).unwrap();
    assert_eq!(s.version(), v);
    assert_eq!(s.history_lengths(), h);
    s.execute_motion_plan(&p).unwrap()
}
fn create(s: &mut AuthoringSession, n: u32) {
    apply(
        s,
        E::Create {
            id: id(n),
            name: format!("Motion {n}"),
            registration_id: id(n + 1),
            destination: C::Root,
        },
    );
}
fn group(s: &mut AuthoringSession, n: u32) {
    apply(
        s,
        E::CreateGroup {
            id: id(n),
            name: format!("Group {n}"),
        },
    );
}
fn attachment(s: &mut AuthoringSession, e: A) -> zipzi_sdk::EditReceipt {
    let p = s.plan_attachment_edit(e).unwrap();
    s.execute_attachment_plan(&p).unwrap()
}

fn lock(s: &mut AuthoringSession, object: u32) {
    let mut state = s.document().editor_state().clone();
    state.objects.entry(id(object)).or_default().locked = true;
    s.edit("Lock object", None, |e| e.replace_editor_state(state))
        .unwrap();
}

#[test]
fn available_motion_export_is_read_only_even_for_locked_pending_tracks() {
    let mut s = session();
    create(&mut s, 10);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    lock(&mut s, 30);
    lock(&mut s, 10);
    let version = s.version();
    let history = s.history_lengths();
    let plan = s.plan_available_motion_export(&id(10)).unwrap();
    assert_eq!(plan.omitted().len(), 1);
    let exported: serde_json::Value =
        serde_json::from_str(&s.export_motion_plan(&plan).unwrap()).unwrap();
    assert_eq!(exported["Curves"], serde_json::json!([]));
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    assert_eq!(s.motion(&id(10)).unwrap().tracks.len(), 1);
}

#[test]
fn attachment_planning_preserves_resolved_scene_metadata() {
    use zipzi_core::document::{Model3Settings, ModelHitArea, ModelTargetRef};
    let mut s = session();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = zipzi_sdk::prepare_png_asset(&id(50), "fixture", &path).unwrap();
    let mesh = zipzi_sdk::rectangle_mesh(
        &id(51),
        "mesh",
        &id(50),
        Vec2::default(),
        Vec2::new(10., 10.),
    )
    .unwrap();
    let settings = Model3Settings {
        hit_areas: Some(vec![ModelHitArea {
            id: zipzi_core::document::ModelHitArea::default().id,
            name: "Head".into(),
            mesh: ModelTargetRef::Resolved { object_id: id(51) },
            extensions: Default::default(),
        }]),
        ..Default::default()
    };
    s.edit("Scene", None, |e| {
        e.create_asset(asset)?;
        e.create_mesh(mesh)?;
        e.set_model3_settings(settings.clone())
    })
    .unwrap();
    attachment(
        &mut s,
        A::Create {
            id: id(40),
            path: "user.json".into(),
        },
    );
    attachment(
        &mut s,
        A::SetUserData {
            reference: Some(AttachmentRef::Resolved {
                attachment_id: id(40),
            }),
        },
    );
    assert_eq!(s.document().model3_settings().hit_areas, settings.hit_areas);
    assert!(s
        .plan_attachment_edit(A::SetUserData {
            reference: Some(AttachmentRef::Resolved {
                attachment_id: id(99)
            }),
        })
        .is_err());
}

#[test]
fn locked_registration_cannot_be_reordered_through_either_entrypoint() {
    for grouped in [false, true] {
        let mut s = session();
        create(&mut s, 10);
        create(&mut s, 20);
        let destination = if grouped {
            group(&mut s, 40);
            apply(
                &mut s,
                E::MoveRegistrations {
                    ids: vec![id(11), id(21)],
                    destination: C::Group(id(40)),
                    before: None,
                },
            );
            C::Group(id(40))
        } else {
            C::Root
        };
        lock(&mut s, 11);
        let version = s.version();
        assert!(s
            .plan_motion_edit(E::MoveRegistrations {
                ids: vec![id(11)],
                destination,
                before: None,
            })
            .is_err());
        let mut groups = s.document().motion_groups().to_vec();
        let mut roots = s.document().motion_roots().to_vec();
        if grouped {
            groups[0].entries.reverse();
        } else {
            roots.reverse();
        }
        assert!(s
            .edit("Bypass registration lock", None, |e| e
                .set_motion_library(groups, roots))
            .is_err());
        assert_eq!(s.version(), version);
    }
}

#[test]
fn locked_track_cannot_be_reordered_by_replacing_clip() {
    let mut s = session();
    create(&mut s, 10);
    for n in [30, 40] {
        apply(
            &mut s,
            E::CreateTrack {
                clip: id(10),
                id: id(n),
                key_id: id(n + 1),
                time: 0.,
            },
        );
    }
    lock(&mut s, 30);
    let mut clip = s.motion(&id(10)).unwrap();
    clip.track_order = vec![id(40), id(30)];
    assert!(s
        .edit("Bypass track lock", None, |e| e.replace_motion(clip))
        .is_err());
}

#[test]
fn bezier_track_transfer_rejects_a_different_runtime_mode_atomically() {
    use zipzi_core::document::{MotionKey, MotionPoint, MotionSegment};
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 20);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    let mut clip = s.motion(&id(10)).unwrap();
    clip.tracks[0].segments = std::sync::Arc::new(vec![MotionSegment::Bezier {
        control1: MotionPoint {
            time: 0.1,
            value: 0.8,
        },
        control2: MotionPoint {
            time: 0.2,
            value: 0.9,
        },
        end: MotionKey {
            id: id(32),
            time: 1.,
            value: 1.,
        },
    }]);
    s.edit("Curve", None, |e| e.replace_motion(clip)).unwrap();
    let mut destination = s.motion(&id(20)).unwrap();
    destination.restricted_beziers = true;
    s.edit("Restricted destination", None, |e| {
        e.replace_motion(destination)
    })
    .unwrap();
    let version = s.version();
    let history = s.history_lengths();
    for copy in [false, true] {
        let error = s
            .plan_motion_edit(E::MoveTracks {
                ids: vec![id(30)],
                destination: id(20),
                before: None,
                copy,
            })
            .unwrap_err();
        assert_eq!(error.code.as_ref(), "INCOMPATIBLE_BEZIER_MODE");
    }
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
}

#[test]
fn attachment_recreated_in_one_transaction_invalidates_old_handle_and_load() {
    for replacement in [false, true] {
        let mut s = session();
        attachment(
            &mut s,
            A::Create {
                id: id(40),
                path: "voice.wav".into(),
            },
        );
        let handle = s.handle(ObjectKind::Attachment, &id(40)).unwrap();
        let token = s.attachment_load_token(&id(40)).unwrap();
        let original = s.document().get_attachment(&id(40)).unwrap().clone();
        let (_, receipt) = s
            .edit("Recreate attachment", None, |e| {
                if replacement {
                    e.set_package_attachments(vec![])?;
                    e.set_package_attachments(vec![original])
                } else {
                    e.edit_attachment(A::Delete { id: id(40) })?;
                    e.edit_attachment(A::Create {
                        id: id(40),
                        path: "voice.wav".into(),
                    })
                }
            })
            .unwrap();
        assert!(receipt.changed);
        assert!(s.resolve_handle(&handle).is_err());
        assert!(s
            .replace_attachment_content(&token, Some(std::sync::Arc::new(vec![1])))
            .is_err());
        s.undo().unwrap();
        assert!(s.resolve_handle(&handle).is_err());
    }
}

#[test]
fn erasing_last_root_registration_is_a_true_no_op() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    s.undo().unwrap();
    let handle = s.handle(ObjectKind::MotionRegistration, &id(11)).unwrap();
    let version = s.version();
    let history = s.history_lengths();
    let (_, receipt) = s
        .edit("Keep last root", None, |e| e.erase_object(&id(11)))
        .unwrap();
    assert!(!receipt.changed);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    assert!(s.resolve_handle(&handle).is_ok());
}
#[test]
fn roots_groups_and_pending_content_roundtrip_with_stable_ids() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    attachment(
        &mut s,
        A::Create {
            id: id(40),
            path: "voice.wav".into(),
        },
    );
    attachment(
        &mut s,
        A::SetSound {
            registration: id(11),
            reference: Some(AttachmentRef::Resolved {
                attachment_id: id(40),
            }),
        },
    );
    let path = std::env::temp_dir().join(format!("c2-motion-roundtrip-{}", std::process::id()));
    s.save_project(&path, None).unwrap();
    let mut reopened = session();
    reopened.open_project(&path, None).unwrap();
    assert_eq!(
        reopened.document().motion_groups(),
        s.document().motion_groups()
    );
    assert_eq!(
        reopened.document().motion_roots(),
        s.document().motion_roots()
    );
    assert_eq!(reopened.motion(&id(10)), s.motion(&id(10)));
    assert_eq!(
        reopened.document().get_attachment(&id(40)).unwrap().bytes,
        None
    );
    let p = reopened.document().motion_projection(&id(10)).unwrap();
    assert_eq!(p.authored_to_runtime, vec![None]);
    assert_eq!(
        reopened.export_motion3(&id(10)).unwrap_err().code.as_ref(),
        "NEEDS_CONFIGURATION"
    );
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn moving_registration_preserves_handle_overrides_and_evaluation_revision() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    let mut e = s.document().motion_roots()[0].clone();
    let expected = e.clone();
    e.fade_in = Some(0.75);
    apply(
        &mut s,
        E::PatchRegistration {
            id: e.id.clone(),
            expected,
            entry: e.clone(),
        },
    );
    let handle = s.handle(ObjectKind::MotionRegistration, &e.id).unwrap();
    let rev = s.evaluation_revision();
    let r = apply(
        &mut s,
        E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        },
    );
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert_eq!(s.evaluation_revision(), rev);
    assert!(s.resolve_handle(&handle).is_ok());
    assert_eq!(s.document().get_motion_registration(&id(11)).unwrap().1, &e);
    let r = apply(
        &mut s,
        E::RenameGroup {
            id: id(20),
            name: "Renamed".into(),
        },
    );
    assert_eq!(s.evaluation_revision(), rev);
    assert!(r.changes.categories.contains(&ChangeCategory::Labels));
    s.undo().unwrap();
    assert_eq!(s.evaluation_revision(), rev);
}
#[test]
fn group_deletion_migrates_last_original_registration_and_keeps_shared_clip() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    apply(
        &mut s,
        E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        },
    );
    let e = s
        .document()
        .get_motion_registration(&id(11))
        .unwrap()
        .1
        .clone();
    let p = s.plan_motion_edit(E::DeleteGroup { id: id(20) }).unwrap();
    assert_eq!(p.migrated_to_root(), &[id(11)]);
    s.execute_motion_plan(&p).unwrap();
    assert_eq!(s.document().motion_roots(), &[e]);
    assert!(s.motion(&id(10)).is_some());
    s.undo().unwrap();
    assert!(s.document().get_motion_group(&id(20)).is_some());
    apply(
        &mut s,
        E::DuplicateRegistrations {
            ids: vec![id(11)],
            destination: C::Root,
            before: None,
        },
    );
    let p = s.plan_motion_edit(E::DeleteGroup { id: id(20) }).unwrap();
    assert!(p.migrated_to_root().is_empty());
    s.execute_motion_plan(&p).unwrap();
    assert!(s.document().get_motion_registration(&id(11)).is_none());
    assert!(s.motion(&id(10)).is_some());
}
#[test]
fn group_copy_shares_clip_and_content_copy_remaps_owned_ids() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    apply(
        &mut s,
        E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        },
    );
    apply(
        &mut s,
        E::DuplicateGroup {
            source: id(20),
            id: id(21),
            name: "Copy".into(),
        },
    );
    let copied = &s.document().get_motion_group(&id(21)).unwrap().entries[0];
    assert_ne!(copied.id, id(11));
    assert_eq!(copied.clip_id, id(10));
    apply(
        &mut s,
        E::DuplicateClip {
            source: id(10),
            id: id(40),
            name: "Content copy".into(),
            registration_id: id(41),
            destination: C::Root,
        },
    );
    let a = s.motion(&id(10)).unwrap();
    let b = s.motion(&id(40)).unwrap();
    assert_ne!(a.tracks[0].id, b.tracks[0].id);
    assert_ne!(a.tracks[0].initial.id, b.tracks[0].initial.id);
    let r = apply(&mut s, E::DeleteClip { id: id(10) });
    assert!(s
        .document()
        .motion_registrations()
        .all(|e| e.clip_id != id(10)));
    assert!(r.changes.deleted.iter().any(|e| e.id == id(31)));
    assert!(s.motion(&id(40)).is_some());
}
#[test]
fn track_display_order_does_not_change_duplicate_target_execution() {
    let mut s = session();
    create(&mut s, 10);
    s.edit("parameter", None, |e| {
        e.create_parameter(Parameter {
            id: id(50),
            runtime_id: "P".into(),
            name: "P".into(),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })
    })
    .unwrap();
    for (t, k) in [(30, 31), (32, 33)] {
        apply(
            &mut s,
            E::CreateTrack {
                clip: id(10),
                id: id(t),
                key_id: id(k),
                time: 0.,
            },
        );
        apply(
            &mut s,
            E::SetTrackTarget {
                id: id(t),
                target: T::Parameter {
                    parameter_id: id(50),
                },
            },
        );
    }
    let mut c = s.motion(&id(10)).unwrap();
    c.tracks[0].initial.value = 0.25;
    c.tracks[1].initial.value = 0.75;
    c.fade_in = Some(0.);
    s.edit("values", None, |e| e.replace_motion(c)).unwrap();
    let sample = |s: &AuthoringSession| {
        let mut p = s.motion_preview();
        p.schedule_motion(&id(10), 0.).unwrap();
        p.advance(0.5).unwrap().parameters[&id(50)]
    };
    let value = sample(&s);
    let rev = s.evaluation_revision();
    let r = apply(
        &mut s,
        E::ReorderTracks {
            clip: id(10),
            ids: vec![id(32), id(30)],
        },
    );
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert_eq!(s.evaluation_revision(), rev);
    assert_eq!(sample(&s), value);
    assert_eq!(s.motion(&id(10)).unwrap().tracks[0].id, id(30));
    apply(
        &mut s,
        E::MoveTracks {
            ids: vec![id(30)],
            destination: id(10),
            before: Some(id(32)),
            copy: false,
        },
    );
    assert_eq!(s.motion(&id(10)).unwrap().tracks[0].id, id(30));
    assert_eq!(sample(&s), value);
}
#[test]
fn track_move_is_atomic_and_retains_identity_and_validation() {
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 20);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 1.5,
        },
    );
    let handle = s.handle(ObjectKind::MotionKey, &id(31)).unwrap();
    apply(
        &mut s,
        E::MoveTracks {
            ids: vec![id(30)],
            destination: id(20),
            before: None,
            copy: false,
        },
    );
    assert!(s.resolve_handle(&handle).is_ok());
    assert!(s.motion(&id(10)).unwrap().tracks.is_empty());
    assert_eq!(s.document().get_motion_track(&id(30)).unwrap().0.id, id(20));
    let mut c = s.motion(&id(10)).unwrap();
    c.duration = 1.;
    s.edit("short clip", None, |e| e.replace_motion(c)).unwrap();
    let v = s.version();
    assert!(s
        .plan_motion_edit(E::MoveTracks {
            ids: vec![id(30)],
            destination: id(10),
            before: None,
            copy: false
        })
        .is_err());
    assert_eq!(s.version(), v);
}
#[test]
fn deleted_owned_handles_stay_dead_after_undo_and_redo() {
    let mut s = session();
    create(&mut s, 10);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    let h = s.handle(ObjectKind::MotionKey, &id(31)).unwrap();
    let r = apply(&mut s, E::DeleteClip { id: id(10) });
    assert!(s.resolve_handle(&h).is_err());
    let undo = s.undo().unwrap();
    assert_eq!(r.changes.deleted, undo.changes.created);
    assert!(s.resolve_handle(&h).is_err());
    let restored = s.handle(ObjectKind::MotionKey, &id(31)).unwrap();
    s.redo().unwrap();
    assert!(s.resolve_handle(&restored).is_err());
}
#[test]
fn registration_addresses_do_not_revive_after_move_and_undo() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    let address = s.motion_registration_address(&id(11)).unwrap();
    let entry = s.document().motion_roots()[0].clone();
    apply(
        &mut s,
        E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        },
    );
    s.undo().unwrap();
    assert!(s.plan_motion_registration_patch(&address, entry).is_err());
}
#[test]
fn no_op_preserves_redo_and_stale_or_forked_plans_are_rejected() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    s.undo().unwrap();
    let history = s.history_lengths();
    let r = apply(&mut s, E::DeleteRegistrations { ids: vec![id(11)] });
    assert!(!r.changed);
    assert_eq!(s.history_lengths(), history);
    let p = s
        .plan_motion_edit(E::CreateGroup {
            id: id(20),
            name: "A".into(),
        })
        .unwrap();
    create(&mut s, 30);
    assert!(s.execute_motion_plan(&p).is_err());
    let mut branch = s.document().clone();
    let p = s
        .document()
        .plan_motion_edit(E::CreateGroup {
            id: id(20),
            name: "A".into(),
        })
        .unwrap();
    let e = branch
        .plan_motion_edit(E::CreateGroup {
            id: id(21),
            name: "B".into(),
        })
        .unwrap();
    branch.apply_motion_edit(&e);
    assert_eq!(branch.apply_motion_edit(&p).status.code, "STALE_PLAN");
}
#[test]
fn attachment_content_epochs_and_incarnations_never_rewind() {
    let mut s = session();
    attachment(
        &mut s,
        A::Create {
            id: id(40),
            path: "voice.wav".into(),
        },
    );
    let token = s.attachment_load_token(&id(40)).unwrap();
    let bytes = Some(std::sync::Arc::new(vec![]));
    s.replace_attachment_content(&token, bytes.clone()).unwrap();
    assert_eq!(s.document().get_attachment(&id(40)).unwrap().bytes, bytes);
    s.undo().unwrap();
    assert!(s.replace_attachment_content(&token, bytes).is_err());
    let token = s.attachment_load_token(&id(40)).unwrap();
    attachment(&mut s, A::Delete { id: id(40) });
    s.undo().unwrap();
    assert!(s.replace_attachment_content(&token, None).is_err());
}
#[test]
fn sound_references_follow_attachment_identity_and_block_delete() {
    let mut s = session();
    create(&mut s, 10);
    attachment(
        &mut s,
        A::Create {
            id: id(40),
            path: "voice.wav".into(),
        },
    );
    let reference = AttachmentRef::Resolved {
        attachment_id: id(40),
    };
    attachment(
        &mut s,
        A::SetSound {
            registration: id(11),
            reference: Some(reference.clone()),
        },
    );
    let expected = s.document().get_attachment(&id(40)).unwrap().clone();
    let mut changed = expected.clone();
    changed.path = "renamed.wav".into();
    attachment(
        &mut s,
        A::Replace {
            expected,
            attachment: changed,
        },
    );
    assert_eq!(
        s.document().attachment_path(&reference),
        Some("renamed.wav")
    );
    assert!(s.plan_attachment_edit(A::Delete { id: id(40) }).is_err());
}
#[test]
fn locks_apply_to_plans_and_low_level_replacements() {
    let mut s = session();
    create(&mut s, 10);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id(31),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    s.edit("lock", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert!(s
        .plan_motion_edit(E::DeleteTracks { ids: vec![id(30)] })
        .is_err());
    let mut c = s.motion(&id(10)).unwrap();
    c.tracks[0].initial.value = 1.;
    assert!(s.edit("bypass", None, |e| e.replace_motion(c)).is_err());
}
#[test]
fn event_time_order_transfer_and_key_cut_use_one_atomic_history() {
    use zipzi_core::document::MotionEvent;
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 20);
    for (clip, t, k) in [(10, 30, 31), (20, 40, 41), (10, 50, 51)] {
        apply(
            &mut s,
            E::CreateTrack {
                clip: id(clip),
                id: id(t),
                key_id: id(k),
                time: 0.,
            },
        );
        apply(
            &mut s,
            E::SetTrackTarget {
                id: id(t),
                target: T::Model {
                    runtime_id: "Opacity".into(),
                },
            },
        );
    }
    s.edit("keys", None, |e| {
        e.insert_motion_key(&id(10), &id(30), &id(32), 0.5, Some(0.5))
    })
    .unwrap();
    let h = s.handle(ObjectKind::MotionKey, &id(32)).unwrap();
    let untouched = s
        .document()
        .get_motion_track(&id(50))
        .unwrap()
        .1
        .segments
        .clone();
    let history = s.history_lengths().0;
    apply(
        &mut s,
        E::TransferKeys {
            ids: vec![id(32)],
            destination_track: id(40),
            time_offset: 0.,
            copy: false,
        },
    );
    assert_eq!(s.history_lengths().0, history + 1);
    assert_eq!(s.document().get_motion_key(&id(32)).unwrap().1.id, id(40));
    assert!(s.resolve_handle(&h).is_ok());
    assert!(std::sync::Arc::ptr_eq(
        &untouched,
        &s.document().get_motion_track(&id(50)).unwrap().1.segments
    ));
    s.undo().unwrap();
    assert_eq!(s.document().get_motion_key(&id(32)).unwrap().1.id, id(30));
    let event = |n, time| MotionEvent {
        id: id(n),
        time,
        value: format!("event {n}"),
        extensions: Default::default(),
    };
    for e in [event(60, 1.), event(61, 0.5), event(62, 0.5)] {
        apply(
            &mut s,
            E::SetEvent {
                clip: id(10),
                event: e,
            },
        );
    }
    assert_eq!(
        s.motion(&id(10))
            .unwrap()
            .events
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
        vec![id(61), id(62), id(60)]
    );
    apply(
        &mut s,
        E::MoveEvents {
            ids: vec![id(61), id(62)],
            destination: id(20),
            time_offset: 0.25,
            copy: false,
        },
    );
    assert_eq!(s.motion(&id(20)).unwrap().events[0].id, id(61));
    let version = s.version();
    assert!(s
        .plan_motion_edit(E::MoveEvents {
            ids: vec![id(60)],
            destination: id(20),
            time_offset: 10.,
            copy: false
        })
        .is_err());
    assert_eq!(s.version(), version);
}
#[test]
fn available_export_lists_omissions_and_rejects_stale_acceptance() {
    let mut s = session();
    create(&mut s, 10);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    let p = s.plan_available_motion_export(&id(10)).unwrap();
    assert_eq!(p.omitted()[0].track_id, id(30));
    let text = s.export_motion_plan(&p).unwrap();
    assert!(!text.contains(&id(30)));
    group(&mut s, 20);
    assert!(s.export_motion_plan(&p).is_err());
}

#[test]
#[ignore = "release motion library planning measurement"]
fn library_planning_shares_curves_in_a_large_library() {
    use zipzi_core::document::{MotionKey, MotionSegment, MotionTrack};
    let mut s = session();
    create(&mut s, 10);
    let template = s.motion(&id(10)).unwrap();
    s.edit("large library", None, |e| {
        for n in 0..64u32 {
            let mut clip = template.clone();
            clip.id = id(1000 + n);
            clip.name = format!("Clip {n}");
            for t in 0..32u32 {
                let base = 100000 + n * 256 + t * 8;
                clip.tracks.push(MotionTrack {
                    id: id(base),
                    target: T::Model {
                        runtime_id: "Opacity".into(),
                    },
                    initial: MotionKey {
                        id: id(base + 1),
                        time: 0.,
                        value: 0.5,
                    },
                    segments: std::sync::Arc::new(
                        (1..4)
                            .map(|k| MotionSegment::Linear {
                                end: MotionKey {
                                    id: id(base + 1 + k),
                                    time: k as f32 * 0.5,
                                    value: 0.5,
                                },
                            })
                            .collect(),
                    ),
                    fade_in: None,
                    fade_out: None,
                    extensions: Default::default(),
                });
            }
            e.create_motion(clip)?;
        }
        Ok(())
    })
    .unwrap();
    group(&mut s, 20);
    let before = s.motion(&id(1000)).unwrap().tracks[0].segments.clone();
    let began = std::time::Instant::now();
    for _ in 0..100 {
        let p = s
            .plan_motion_edit(E::MoveRegistrations {
                ids: vec![id(11)],
                destination: C::Group(id(20)),
                before: None,
            })
            .unwrap();
        std::hint::black_box(p);
    }
    let elapsed = began.elapsed();
    let p = s
        .plan_motion_edit(E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        })
        .unwrap();
    let commit = std::time::Instant::now();
    s.execute_motion_plan(&p).unwrap();
    eprintln!(
        "65 clips / 2048 tracks / 8192 keys: 100 plans {:?}, commit {:?}",
        elapsed,
        commit.elapsed()
    );
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &s.motion(&id(1000)).unwrap().tracks[0].segments
    ));
}

#[test]
fn deleting_hidden_owned_content_cleans_flags_and_undo_restores_with_new_incarnation() {
    let mut s = session();
    create(&mut s, 10);
    apply(
        &mut s,
        E::CreateTrack {
            clip: id(10),
            id: id(30),
            key_id: id(31),
            time: 0.,
        },
    );
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id(31),
        zipzi_core::document::ObjectEditorState {
            locked: false,
            hidden_in_editor: true,
        },
    );
    s.edit("hide key", None, |e| e.replace_editor_state(state))
        .unwrap();
    let handle = s.handle(ObjectKind::MotionKey, &id(31)).unwrap();
    apply(&mut s, E::DeleteTracks { ids: vec![id(30)] });
    assert!(!s.document().editor_state().objects.contains_key(&id(31)));
    s.undo().unwrap();
    assert!(s.document().editor_state().get(&id(31)).hidden_in_editor);
    assert!(s.resolve_handle(&handle).is_err());
    let mut clip = s.motion(&id(10)).unwrap();
    clip.tracks.clear();
    clip.track_order.clear();
    s.edit("replace content", None, |e| e.replace_motion(clip))
        .unwrap();
    assert!(!s.document().editor_state().objects.contains_key(&id(31)));
}
