use std::collections::VecDeque;
use zipzi_runtime::{preview::PreviewCommand as C, *};

#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}
fn id(n: u32) -> String {
    format!("79630000-0000-4000-8000-{n:012x}")
}
fn fixture() -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "parameter", None).unwrap();
    rt.apply_edit(client, edit, |e| {
        e.create_parameter_spec(zipzi_sdk::ParameterCreateSpec {
            id: Some(id(2)),
            name: "P".into(),
            minimum: -1.,
            maximum: 1.,
            default_value: 0.,
            runtime_id: None,
            kind: None,
            decimal_places: None,
            repeat: None,
        })
        .map(|_| ())
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    (rt, client, session)
}
fn finish(
    rt: &mut Runtime,
    c: ClientId,
    job: JobId,
) -> std::sync::Arc<preview::PreviewSampleBatch> {
    let mut workers = Manual::default();
    while !rt.job_info(c, job).unwrap().state.is_terminal() {
        rt.dispatch(&mut workers);
        assert!(rt.complete(workers.0.pop_front().unwrap().run_quantum()));
    }
    let JobValue::Previewed(value) = rt.job_info(c, job).unwrap().result.unwrap().unwrap() else {
        panic!("preview result")
    };
    rt.release_job(c, job).unwrap();
    value
}
#[test]
fn clients_views_frozen_sources_and_formal_history_are_independent() {
    let (mut rt, c, s) = fixture();
    let other = rt.register_client().unwrap();
    rt.attach_session(other, s).unwrap();
    let before = rt.session_info(s).unwrap();
    let (a, source) = rt.create_preview(c, s, None, None).unwrap();
    let (b, _) = rt.create_preview(c, s, None, None).unwrap();
    let (remote, _) = rt.create_preview(other, s, None, None).unwrap();
    rt.apply_preview(
        c,
        a,
        C::SetParameter {
            id: id(2),
            value: 0.75,
        },
    )
    .unwrap();
    rt.apply_preview(
        c,
        b,
        C::SetParameter {
            id: id(2),
            value: -0.5,
        },
    )
    .unwrap();
    assert!(matches!(
        rt.apply_preview(other, a, C::Reset),
        Err(RuntimeError::WrongOwner)
    ));
    assert!(matches!(
        rt.release_preview(other, a),
        Err(RuntimeError::WrongOwner)
    ));
    let job = rt
        .request_preview_samples(c, a, vec![0., 0.5, 0.25, 0.5], true, false)
        .unwrap();
    assert!(matches!(
        rt.apply_preview(c, a, C::Reset),
        Err(RuntimeError::Busy)
    ));
    rt.apply_preview(
        other,
        remote,
        C::SetParameter {
            id: id(2),
            value: 0.25,
        },
    )
    .unwrap();
    let result = finish(&mut rt, c, job);
    assert_eq!(result.source, source);
    assert_eq!(
        result
            .samples
            .frames
            .iter()
            .map(|f| f.motion.time)
            .collect::<Vec<_>>(),
        vec![0., 0.5, 0.25, 0.5]
    );
    for frame in &result.samples.frames {
        assert_eq!(frame.preview_id, a.serial());
        assert_eq!(frame.motion.parameters[&id(2)], 0.75);
        assert!(frame.geometry.is_some());
    }
    let job = rt
        .request_preview_samples(c, b, vec![0.5], false, false)
        .unwrap();
    assert_eq!(
        finish(&mut rt, c, job).samples.frames[0].motion.parameters[&id(2)],
        -0.5
    );
    let job = rt
        .request_preview_samples(other, remote, vec![0.5], false, false)
        .unwrap();
    assert_eq!(
        finish(&mut rt, other, job).samples.frames[0]
            .motion
            .parameters[&id(2)],
        0.25
    );
    assert_eq!(rt.session_info(s).unwrap(), before);
    let edit = rt.begin_edit(c, s, "new parameter", None).unwrap();
    rt.apply_edit(c, edit, |e| {
        e.create_parameter_spec(zipzi_sdk::ParameterCreateSpec {
            id: Some(id(3)),
            name: "new".into(),
            minimum: -1.,
            maximum: 1.,
            default_value: 0.5,
            runtime_id: None,
            kind: None,
            decimal_places: None,
            repeat: None,
        })
        .map(|_| ())
    })
    .unwrap();
    rt.commit_edit(c, edit).unwrap();
    let job = rt
        .request_preview_samples(c, a, vec![0.], false, false)
        .unwrap();
    let old = finish(&mut rt, c, job);
    assert_eq!(old.source, source);
    assert!(!old.samples.frames[0].motion.parameters.contains_key(&id(3)));
    let (fresh, _) = rt.create_preview(c, s, None, None).unwrap();
    let job = rt
        .request_preview_samples(c, fresh, vec![0.], false, false)
        .unwrap();
    assert_eq!(
        finish(&mut rt, c, job).samples.frames[0].motion.parameters[&id(3)],
        0.5
    );
}
#[test]
fn cancelled_queued_work_release_and_budgets_leave_no_preview_roots() {
    let (mut rt, c, s) = fixture();
    let base = rt.accounted_bytes();
    for _ in 0..100 {
        let (p, _) = rt.create_preview(c, s, None, None).unwrap();
        let job = rt
            .request_preview_samples(c, p, vec![100.], true, false)
            .unwrap();
        rt.cancel_job(c, job).unwrap();
        assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Cancelled);
        rt.apply_preview(
            c,
            p,
            C::SetParameter {
                id: id(2),
                value: 0.5,
            },
        )
        .unwrap();
        assert_eq!(rt.preview_state(c, p).unwrap().1.time, 0.);
        rt.release_job(c, job).unwrap();
        rt.release_preview(c, p).unwrap();
    }
    assert_eq!(rt.preview_count(), 0);
    assert_eq!(rt.accounted_bytes(), base);
    let (p, _) = rt.create_preview(c, s, None, None).unwrap();
    assert!(rt
        .request_preview_samples(c, p, vec![0.; 65], false, false)
        .is_err());
    assert!(rt
        .apply_preview(
            c,
            p,
            C::SetParameter {
                id: id(99),
                value: 0.5
            }
        )
        .is_err());
    assert_eq!(rt.preview_state(c, p).unwrap().1.sequence, 0);
    let job = rt
        .request_preview_samples(c, p, vec![0.5], false, false)
        .unwrap();
    rt.release_preview(c, p).unwrap();
    rt.release_preview(c, p).unwrap();
    assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Cancelled);
    rt.release_job(c, job).unwrap();
    assert_eq!(rt.accounted_bytes(), base);
    let mut limited = Runtime::new(RuntimeLimits {
        preview_bytes: 0,
        ..Default::default()
    });
    let c = limited.register_client().unwrap();
    let s = limited
        .create_session(c, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    assert!(matches!(
        limited.create_preview(c, s, None, None),
        Err(RuntimeError::Budget(_))
    ));
    assert_eq!(limited.preview_count(), 0);
}
#[test]
fn candidate_capture_cancel_expiry_and_late_completion_keep_exact_source() {
    let (mut rt, c, s) = fixture();
    let edit = rt.begin_edit(c, s, "candidate", None).unwrap();
    let (p, source) = rt.create_preview(c, s, Some(edit), None).unwrap();
    rt.apply_edit(c, edit, |_| Ok(())).unwrap();
    rt.release_edit(c, edit).unwrap();
    let job = rt
        .request_preview_samples(c, p, vec![0.5], true, false)
        .unwrap();
    let value = finish(&mut rt, c, job);
    assert_eq!(value.source, source);
    assert!(matches!(source, SourceRef::Edit { sequence: 0, .. }));
    let job = rt
        .request_preview_samples(c, p, vec![100.], false, false)
        .unwrap();
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    assert_eq!(workers.0.len(), 1);
    rt.release_client(c);
    assert_eq!(rt.preview_count(), 0);
    assert!(rt.complete(workers.0.pop_front().unwrap().run()));
    assert_eq!(rt.session_info(s).unwrap().state, SessionState::Ready);
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (1, 0));
    let _ = job;
}

#[test]
fn cancellation_after_sdk_completion_reports_the_completed_state() {
    let (mut rt, c, s) = fixture();
    let (p, _) = rt.create_preview(c, s, None, None).unwrap();
    let job = rt
        .request_preview_samples(c, p, vec![0.5], false, false)
        .unwrap();
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    let completed = workers.0.pop_front().unwrap().run();
    rt.cancel_job(c, job).unwrap();
    assert!(rt.complete(completed));
    assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Succeeded);
    assert_eq!(rt.preview_state(c, p).unwrap().1.time, 0.5);
}

fn event_fixture(limits: RuntimeLimits, payload: usize) -> (Runtime, ClientId, SessionId) {
    use zipzi_core::document::{MotionClip, MotionEvent};
    let mut rt = Runtime::new(limits);
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(c, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let e = rt.begin_edit(c, s, "events", None).unwrap();
    rt.apply_edit(c, e, |edit| {
        edit.create_motion(MotionClip {
            id: id(2),
            name: "Events".into(),
            duration: 1.,
            fps: 60.,
            looping: false,
            restricted_beziers: false,
            fade_in: Some(1.),
            fade_out: Some(1.),
            track_order: vec![],
            tracks: vec![].into(),
            events: vec![MotionEvent {
                id: id(3),
                time: 0.01,
                value: "x".repeat(payload),
                extensions: Default::default(),
            }]
            .into(),
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })
    .unwrap();
    rt.commit_edit(c, e).unwrap();
    (rt, c, s)
}

#[test]
fn repeated_motion_events_are_admitted_before_advancing_or_allocating_results() {
    let ceiling = 20 << 20;
    let (mut rt, c, s) = event_fixture(
        RuntimeLimits {
            accounted_bytes: ceiling,
            ..Default::default()
        },
        64 << 10,
    );
    let (p, _) = rt.create_preview(c, s, None, None).unwrap();
    for _ in 0..512 {
        rt.apply_preview(
            c,
            p,
            C::ScheduleMotion {
                id: id(2),
                time: 0.,
            },
        )
        .unwrap();
    }
    let before = rt.preview_state(c, p).unwrap();
    let accounted = rt.accounted_bytes();
    for view in [
        None,
        Some(observation::ObservationView {
            width: 8,
            height: 8,
            roi: [0., 0., 1., 1.],
            padding_model: 0.,
            topology: None,
            quality: false,
        }),
    ] {
        for incremental in [false, true] {
            assert!(matches!(
                rt.request_preview_frames(c, p, vec![2. / 60.], false, incremental, vec![], view),
                Err(RuntimeError::Budget(_))
            ));
            assert_eq!(
                rt.preview_state(c, p).unwrap().1.sequence,
                before.1.sequence
            );
            assert_eq!(rt.preview_state(c, p).unwrap().1.time, 0.);
            assert_eq!(rt.accounted_bytes(), accounted);
        }
    }
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    assert!(workers.0.is_empty());
    rt.apply_preview(c, p, C::Reset).unwrap();
}

#[test]
fn sampled_events_remain_charged_until_the_preview_is_released() {
    let (mut rt, c, s) = event_fixture(RuntimeLimits::default(), 16 << 10);
    let base = rt.accounted_bytes();
    let (p, _) = rt.create_preview(c, s, None, None).unwrap();
    for _ in 0..4 {
        rt.apply_preview(
            c,
            p,
            C::ScheduleMotion {
                id: id(2),
                time: 0.,
            },
        )
        .unwrap();
    }
    let before = rt.accounted_bytes();
    let job = rt
        .request_preview_samples(c, p, vec![2. / 60., 2. / 60.], false, false)
        .unwrap();
    let admitted = rt.accounted_bytes();
    let result = finish(&mut rt, c, job);
    for frame in &result.samples.frames {
        assert_eq!(frame.motion.fired_events.len(), 4);
        assert!(frame
            .motion
            .fired_events
            .iter()
            .all(|e| e.value.len() == 16 << 10));
    }
    assert!(rt.accounted_bytes() > before);
    assert!(rt.accounted_bytes() < admitted);
    drop(result);
    rt.release_preview(c, p).unwrap();
    assert_eq!(rt.accounted_bytes(), base);
}
