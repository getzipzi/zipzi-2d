use zipzi_core::document::AllocateModelMetadata;
use zipzi_core::document::{
    AttachmentEdit as A, AttachmentRef, MotionContainer as C, MotionEdit as E,
    MotionTrackTarget as T,
};
use zipzi_core::state::{RecordDomain, RootDomain};
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_sdk::{AuthoringSession, MotionCurveEdit};
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 10.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}
fn apply(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let v = s.version();
    let h = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let p = crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_motion(e))
        .unwrap();
    assert_eq!(s.version(), v);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        h
    );
    s.publish(p.1).unwrap()
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
fn attachment(s: &mut AuthoringSession, e: A) -> zipzi_sdk::Publication {
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_attachment(e)
    })
    .unwrap();
    s.publish(p.1).unwrap()
}

fn lock(s: &mut AuthoringSession, object: u32) {
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.entry(id(object)).or_default().locked = true;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock object")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let captured = s.read_snapshot();
    assert_eq!(
        captured
            .snapshot
            .document()
            .motion_projection(&id(10))
            .unwrap()
            .disabled
            .len(),
        1
    );
    let exported: serde_json::Value = serde_json::from_str(
        &zipzi_project::export_available_motion3(captured.snapshot.document(), &id(10)).unwrap(),
    )
    .unwrap();
    assert_eq!(exported["Curves"], serde_json::json!([]));
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert_eq!(
        s.snapshot()
            .document()
            .motion(&id(10))
            .cloned()
            .unwrap()
            .tracks
            .len(),
        1
    );
}

#[test]
fn attachment_planning_preserves_resolved_scene_metadata() {
    use zipzi_core::document::{Model3Settings, ModelHitArea, ModelTargetRef};
    let mut s = session();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = crate::support::image_fixture(&id(50), "fixture", &path).unwrap();
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
            id: zipzi_core::document::ModelHitArea::allocate().id,
            name: "Head".into(),
            mesh: ModelTargetRef::Resolved { object_id: id(51) },
            extensions: Default::default(),
        }]),
        ..Default::default()
    };
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_image(e, asset)?;
                e.create_mesh(mesh)?;
                e.replace_model3_settings(settings.clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Scene")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    assert_eq!(
        s.snapshot().document().model3_settings().hit_areas,
        settings.hit_areas
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_attachment(
            A::SetUserData {
                reference: Some(AttachmentRef::Resolved {
                    attachment_id: id(99)
                }),
            }
        ))
        .is_err()
    );
}

#[test]
fn locked_root_and_group_registrations_cannot_be_reordered() {
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
        assert!(
            crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_motion(
                E::MoveRegistrations {
                    ids: vec![id(11)],
                    destination,
                    before: None,
                }
            ))
            .is_err()
        );
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
    let mut clip = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    clip.track_order = vec![id(40), id(30)];
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(clip))
            .and_then(|value| {
                let prepared = draft.prepare("Bypass track lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    let mut clip = s.snapshot().document().motion(&id(10)).cloned().unwrap();
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(clip))
            .and_then(|value| {
                let prepared = draft.prepare("Curve")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut destination = s.snapshot().document().motion(&id(20)).cloned().unwrap();
    destination.restricted_beziers = true;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(destination))
            .and_then(|value| {
                let prepared = draft.prepare("Restricted destination")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = s.version();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    for copy in [false, true] {
        let error = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_motion(E::MoveTracks {
                ids: vec![id(30)],
                destination: id(20),
                before: None,
                copy,
            })
        })
        .unwrap_err();
        assert_eq!(error.code.as_str(), "INCOMPATIBLE_BEZIER_MODE");
    }
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
}

#[test]
fn attachment_recreated_in_one_transaction_invalidates_old_handle_and_load() {
    let mut s = session();
    attachment(
        &mut s,
        A::Create {
            id: id(40),
            path: "voice.wav".into(),
        },
    );
    let before = s.read_snapshot();
    let (_, prepared) = crate::support::prepared(&s, |e| {
        e.edit_attachment(A::Delete { id: id(40) })?;
        e.edit_attachment(A::Create {
            id: id(40),
            path: "voice.wav".into(),
        })
    })
    .unwrap();
    let receipt = s.publish(prepared).unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(s.read_snapshot().source, before.source);
    assert_eq!(
        s.snapshot().document().attachment(&id(40)),
        before.snapshot.document().attachment(&id(40))
    );
}

#[test]
fn erasing_last_root_registration_is_a_true_no_op() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    s.undo(s.version()).unwrap();
    let original = s
        .snapshot()
        .document()
        .motion_registration(&id(11))
        .unwrap()
        .1
        .clone();
    let version = s.version();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_motion(E::DeleteRegistrations { ids: vec![id(11)] })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Keep last root")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert_eq!(
        s.snapshot()
            .document()
            .motion_registration(&id(11))
            .unwrap()
            .1,
        &original
    );
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
    let reopened = AuthoringSession::new(crate::support::roundtrip(s.snapshot()));
    assert_eq!(
        reopened.snapshot().document().motion_groups(),
        s.snapshot().document().motion_groups()
    );
    assert_eq!(
        reopened.snapshot().document().motion_roots(),
        s.snapshot().document().motion_roots()
    );
    assert_eq!(
        reopened.snapshot().document().motion(&id(10)).cloned(),
        s.snapshot().document().motion(&id(10)).cloned()
    );
    assert_eq!(
        reopened
            .snapshot()
            .document()
            .attachment(&id(40))
            .unwrap()
            .bytes,
        None
    );
    let p = reopened
        .snapshot()
        .document()
        .motion_projection(&id(10))
        .unwrap();
    assert_eq!(p.authored_to_runtime, vec![None]);
    assert_eq!(
        zipzi_project::export_motion3(reopened.snapshot().document(), &id(10))
            .unwrap_err()
            .code
            .as_str(),
        "NEEDS_CONFIGURATION"
    );
}
#[test]
fn moving_registration_preserves_handle_overrides_and_evaluation_revision() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    let mut e = s.snapshot().document().motion_roots()[0].clone();
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
    let baseline = s.snapshot().document().motion(&id(10)).unwrap().clone();
    let rev = s.snapshot().content_id();
    let r = apply(
        &mut s,
        E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        },
    );
    assert!(!r.writes.is_empty());
    assert_ne!(s.snapshot().content_id(), rev);
    assert_eq!(s.snapshot().document().motion(&id(10)).unwrap(), &baseline);
    assert_eq!(
        s.snapshot()
            .document()
            .motion_registration(&id(11))
            .unwrap()
            .1,
        &e
    );
    let r = apply(
        &mut s,
        E::RenameGroup {
            id: id(20),
            name: "Renamed".into(),
        },
    );
    assert_ne!(s.snapshot().content_id(), rev);
    assert!(r.writes.root_changed(RootDomain::MotionGroups));
    s.undo(s.version()).unwrap();
    assert_ne!(s.snapshot().content_id(), rev);
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
        .snapshot()
        .document()
        .motion_registration(&id(11))
        .unwrap()
        .1
        .clone();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_motion(E::DeleteGroup { id: id(20) })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert_eq!(s.snapshot().document().motion_roots(), &[e]);
    assert!(s.snapshot().document().motion(&id(10)).cloned().is_some());
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().motion_group(&id(20)).is_some());
    apply(
        &mut s,
        E::DuplicateRegistrations {
            ids: vec![id(11)],
            destination: C::Root,
            before: None,
        },
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_motion(E::DeleteGroup { id: id(20) })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .motion_registration(&id(11))
        .is_none());
    assert!(s.snapshot().document().motion(&id(10)).cloned().is_some());
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
    let copied = &s
        .snapshot()
        .document()
        .motion_group(&id(21))
        .unwrap()
        .entries[0];
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
    let a = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    let b = s.snapshot().document().motion(&id(40)).cloned().unwrap();
    assert_ne!(a.tracks[0].id, b.tracks[0].id);
    assert_ne!(a.tracks[0].initial.id, b.tracks[0].initial.id);
    let r = apply(&mut s, E::DeleteClip { id: id(10) });
    assert!(s
        .snapshot()
        .document()
        .motion_registrations()
        .all(|e| e.clip_id != id(10)));
    assert!(r
        .writes
        .records()
        .iter()
        .any(|record| record.domain == RecordDomain::Motion && record.id == id(10)));
    assert!(s.snapshot().document().motion_key(&id(31)).is_none());
    assert!(s.snapshot().document().motion(&id(40)).cloned().is_some());
}
#[test]
fn track_display_order_does_not_change_duplicate_target_execution() {
    let mut s = session();
    create(&mut s, 10);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter(Parameter {
                    id: id(50),
                    runtime_id: "P".into(),
                    name: "P".into(),
                    minimum: 0.,
                    maximum: 1.,
                    ..Default::default()
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("parameter")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    let mut c = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    c.tracks[0].initial.value = 0.25;
    c.tracks[1].initial.value = 0.75;
    c.fade_in = Some(0.);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(c))
            .and_then(|value| {
                let prepared = draft.prepare("values")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let sample = |s: &AuthoringSession| {
        let mut p = zipzi_animation::MotionPreview::new(s.snapshot().document());
        p.schedule_motion(&id(10), 0.).unwrap();
        p.advance(0.5).unwrap().parameters[&id(50)]
    };
    let value = sample(&s);
    let rev = s.snapshot().content_id();
    let r = apply(
        &mut s,
        E::ReorderTracks {
            clip: id(10),
            ids: vec![id(32), id(30)],
        },
    );
    assert!(!r.writes.is_empty());
    assert_ne!(s.snapshot().content_id(), rev);
    assert_eq!(sample(&s), value);
    assert_eq!(
        s.snapshot()
            .document()
            .motion(&id(10))
            .cloned()
            .unwrap()
            .tracks[0]
            .id,
        id(30)
    );
    apply(
        &mut s,
        E::MoveTracks {
            ids: vec![id(30)],
            destination: id(10),
            before: Some(id(32)),
            copy: false,
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .motion(&id(10))
            .cloned()
            .unwrap()
            .tracks[0]
            .id,
        id(30)
    );
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
    let original_key = s
        .snapshot()
        .document()
        .motion_key(&id(31))
        .unwrap()
        .2
        .clone();
    apply(
        &mut s,
        E::MoveTracks {
            ids: vec![id(30)],
            destination: id(20),
            before: None,
            copy: false,
        },
    );
    assert_eq!(
        s.snapshot().document().motion_key(&id(31)).unwrap().2,
        &original_key
    );
    assert!(s
        .snapshot()
        .document()
        .motion(&id(10))
        .cloned()
        .unwrap()
        .tracks
        .is_empty());
    assert_eq!(
        s.snapshot().document().motion_track(&id(30)).unwrap().0.id,
        id(20)
    );
    let mut c = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    c.duration = 1.;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(c))
            .and_then(|value| {
                let prepared = draft.prepare("short clip")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let v = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_motion(
            E::MoveTracks {
                ids: vec![id(30)],
                destination: id(10),
                before: None,
                copy: false
            }
        ))
        .is_err()
    );
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
    let stale = s.begin_draft();
    apply(&mut s, E::DeleteClip { id: id(10) });
    assert!(s.snapshot().document().motion_key(&id(31)).is_none());
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().motion_key(&id(31)).is_some());
    assert_eq!(
        s.publish(stale.prepare("deleted source").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let restored = s.begin_draft();
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().motion_key(&id(31)).is_none());
    assert_eq!(
        s.publish(restored.prepare("redo source").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
}

#[test]
fn registration_addresses_do_not_revive_after_move_and_undo() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    let mut stale = s.begin_draft();
    let entry = s.snapshot().document().motion_roots()[0].clone();
    stale
        .batch(stale.source(), |candidate_commands| {
            candidate_commands.edit_motion(E::PatchRegistration {
                id: id(11),
                expected: entry.clone(),
                entry,
            })
        })
        .unwrap();
    apply(
        &mut s,
        E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("registration").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
}
#[test]
fn no_op_preserves_redo_and_stale_or_forked_plans_are_rejected() {
    let mut s = session();
    create(&mut s, 10);
    group(&mut s, 20);
    s.undo(s.version()).unwrap();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let r = apply(&mut s, E::DeleteRegistrations { ids: vec![id(11)] });
    assert!(r.writes.is_empty());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_motion(E::CreateGroup {
            id: id(20),
            name: "A".into(),
        })
    })
    .unwrap();
    create(&mut s, 30);
    assert!(s.publish(p.1).is_err());
    let branch = AuthoringSession::new(s.snapshot().clone());
    let (_, foreign) = crate::support::prepared(&branch, |candidate_commands| {
        candidate_commands.edit_motion(E::CreateGroup {
            id: id(20),
            name: "A".into(),
        })
    })
    .unwrap();
    assert_eq!(s.publish(foreign).unwrap_err().code, "STALE_SOURCE");
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
    let expected = s.snapshot().document().attachment(&id(40)).unwrap().clone();
    let mut loaded = expected.clone();
    loaded.bytes = Some(std::sync::Arc::new(vec![]));
    let write = A::Replace {
        expected,
        attachment: loaded.clone(),
    };
    let (_, stale) = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_attachment(write.clone())
    })
    .unwrap();
    attachment(&mut s, write.clone());
    assert_eq!(
        s.snapshot().document().attachment(&id(40)).unwrap(),
        &loaded
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    let (_, stale) = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_attachment(write)
    })
    .unwrap();
    attachment(&mut s, A::Delete { id: id(40) });
    s.undo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
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
    let expected = s.snapshot().document().attachment(&id(40)).unwrap().clone();
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
        s.snapshot().document().attachment_path(&reference),
        Some("renamed.wav")
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_attachment(A::Delete { id: id(40) }))
        .is_err()
    );
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
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id(31),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_motion(E::DeleteTracks { ids: vec![id(30)] }))
        .is_err()
    );
    let mut c = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    c.tracks[0].initial.value = 1.;
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(c))
            .and_then(|value| {
                let prepared = draft.prepare("bypass")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_motion_curve(
                    &id(10),
                    MotionCurveEdit::InsertKey {
                        track: id(30),
                        id: id(32),
                        time: 0.5,
                        value: Some(0.5),
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("keys")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = s
        .snapshot()
        .document()
        .motion_key(&id(32))
        .unwrap()
        .2
        .clone();
    let untouched = s
        .snapshot()
        .document()
        .motion_track(&id(50))
        .unwrap()
        .1
        .segments
        .clone();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    }
    .0;
    apply(
        &mut s,
        E::TransferKeys {
            ids: vec![id(32)],
            destination_track: id(40),
            time_offset: 0.,
            copy: false,
        },
    );
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        }
        .0,
        history + 1
    );
    assert_eq!(
        s.snapshot().document().motion_key(&id(32)).unwrap().1.id,
        id(40)
    );
    assert_eq!(
        s.snapshot().document().motion_key(&id(32)).unwrap().2,
        &original
    );
    assert!(std::sync::Arc::ptr_eq(
        &untouched,
        &s.snapshot()
            .document()
            .motion_track(&id(50))
            .unwrap()
            .1
            .segments
    ));
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().motion_key(&id(32)).unwrap().1.id,
        id(30)
    );
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
        s.snapshot()
            .document()
            .motion(&id(10))
            .cloned()
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
    assert_eq!(
        s.snapshot()
            .document()
            .motion(&id(20))
            .cloned()
            .unwrap()
            .events[0]
            .id,
        id(61)
    );
    let version = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_motion(
            E::MoveEvents {
                ids: vec![id(60)],
                destination: id(20),
                time_offset: 10.,
                copy: false
            }
        ))
        .is_err()
    );
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
    let captured = s.read_snapshot();
    let stale = s.begin_draft();
    assert_eq!(
        captured
            .snapshot
            .document()
            .motion_projection(&id(10))
            .unwrap()
            .disabled[0]
            .track_id,
        id(30)
    );
    let text =
        zipzi_project::export_available_motion3(captured.snapshot.document(), &id(10)).unwrap();
    assert!(!text.contains(&id(30)));
    group(&mut s, 20);
    assert_eq!(
        s.publish(stale.prepare("accept omissions").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        zipzi_project::export_available_motion3(captured.snapshot.document(), &id(10)).unwrap(),
        text
    );
}

#[test]
#[ignore = "release motion library planning measurement"]
fn library_planning_shares_curves_in_a_large_library() {
    use zipzi_core::document::{MotionKey, MotionSegment, MotionTrack};
    let mut s = session();
    create(&mut s, 10);
    let template = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
            .and_then(|value| {
                let prepared = draft.prepare("large library")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    group(&mut s, 20);
    let before = s
        .snapshot()
        .document()
        .motion(&id(1000))
        .cloned()
        .unwrap()
        .tracks[0]
        .segments
        .clone();
    let began = std::time::Instant::now();
    for _ in 0..100 {
        let p = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_motion(E::MoveRegistrations {
                ids: vec![id(11)],
                destination: C::Group(id(20)),
                before: None,
            })
        })
        .unwrap();
        std::hint::black_box(p);
    }
    let elapsed = began.elapsed();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_motion(E::MoveRegistrations {
            ids: vec![id(11)],
            destination: C::Group(id(20)),
            before: None,
        })
    })
    .unwrap();
    let commit = std::time::Instant::now();
    s.publish(p.1).unwrap();
    eprintln!(
        "65 clips / 2048 tracks / 8192 keys: 100 plans {:?}, commit {:?}",
        elapsed,
        commit.elapsed()
    );
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &s.snapshot()
            .document()
            .motion(&id(1000))
            .cloned()
            .unwrap()
            .tracks[0]
            .segments
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
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id(31),
        zipzi_core::document::ObjectEditorState {
            locked: false,
            hidden_in_editor: true,
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("hide key")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let stale = s.begin_draft();
    apply(&mut s, E::DeleteTracks { ids: vec![id(30)] });
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&id(31)));
    s.undo(s.version()).unwrap();
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&id(31))
            .hidden_in_editor
    );
    assert_eq!(
        s.publish(stale.prepare("removed owned content").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let mut clip = s.snapshot().document().motion(&id(10)).cloned().unwrap();
    clip.tracks.clear();
    clip.track_order.clear();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_motion(clip))
            .and_then(|value| {
                let prepared = draft.prepare("replace content")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&id(31)));
}
