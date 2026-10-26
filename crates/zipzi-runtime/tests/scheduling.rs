use std::collections::VecDeque;
use zipzi_runtime::{authoring::ModelingSampleQuery, *};

#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}
fn fixture(workers: usize) -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::new(RuntimeLimits {
        running_jobs: workers,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            zipzi_core::ModelSetup::default(),
        )
        .unwrap();
    (rt, client, session)
}
fn samples(rt: &mut Runtime, client: ClientId, session: SessionId, count: usize) -> JobId {
    rt.request_modeling_samples(
        client,
        ModelingSampleQuery {
            session,
            edit: None,
            parameters: vec![Default::default(); count],
            checks: Default::default(),
            expected: None,
        },
    )
    .unwrap()
}
fn step(rt: &mut Runtime, workers: &mut Manual) -> JobId {
    rt.dispatch(workers);
    assert_eq!(workers.0.len(), 1);
    let work = workers.0.pop_front().unwrap();
    let id = work.job_id();
    assert!(rt.complete(work.run_quantum()));
    id
}
#[test]
fn clients_rotate_despite_a_flood_and_batches_yield_to_other_jobs() {
    let (mut rt, a, s) = fixture(2);
    let b = rt.register_client().unwrap();
    let c = rt.register_client().unwrap();
    let a1 = samples(&mut rt, a, s, 3);
    let a2 = samples(&mut rt, a, s, 1);
    let a3 = samples(&mut rt, a, s, 1);
    let b1 = samples(&mut rt, b, s, 1);
    let c1 = samples(&mut rt, c, s, 1);
    let before = rt.session_info(s).unwrap();
    let mut workers = Manual::default();
    assert_eq!(step(&mut rt, &mut workers), a1);
    assert_eq!(rt.job_info(a, a1).unwrap().state, JobState::Running);
    assert_eq!(step(&mut rt, &mut workers), b1);
    assert_eq!(step(&mut rt, &mut workers), c1);
    assert_eq!(step(&mut rt, &mut workers), a2);
    assert_eq!(step(&mut rt, &mut workers), a3);
    assert_eq!(step(&mut rt, &mut workers), a1);
    assert_eq!(step(&mut rt, &mut workers), a1);
    let JobValue::Sampled(batch) = rt.job_info(a, a1).unwrap().result.unwrap().unwrap() else {
        panic!()
    };
    assert_eq!(batch.samples.frames.len(), 3);
    assert_eq!(batch.samples.parameters.len(), 3);
    assert_eq!(batch.source.version(), before.version);
    assert_eq!(rt.session_info(s).unwrap(), before);
}
#[test]
fn a_publication_preempts_a_continuation_even_on_a_single_worker_host() {
    let (mut rt, c, s) = fixture(1);
    let batch = samples(&mut rt, c, s, 64);
    let mut workers = Manual::default();
    assert_eq!(step(&mut rt, &mut workers), batch);
    let edit = rt.begin_edit(c, s, "resize", None).unwrap();
    rt.apply_edit(c, edit, |e| {
        e.replace_model_setup(zipzi_core::Canvas::new(20., 20., Default::default(), 1.))
    })
    .unwrap();
    let publish = rt.request_commit(c, edit).unwrap();
    assert_eq!(step(&mut rt, &mut workers), publish);
    assert_eq!(rt.job_info(c, publish).unwrap().state, JobState::Succeeded);
    assert_eq!(rt.job_info(c, batch).unwrap().state, JobState::Running);
    rt.cancel_job(c, batch).unwrap();
    assert_eq!(rt.job_info(c, batch).unwrap().state, JobState::Cancelled);
    rt.dispatch(&mut workers);
    assert!(workers.0.is_empty());
}
#[test]
fn cancelling_a_yielded_preview_releases_busy_and_preserves_completed_targets() {
    let (mut rt, c, s) = fixture(2);
    let before = rt.session_info(s).unwrap();
    let (preview, _) = rt.create_preview(c, s, None, None).unwrap();
    let job = rt
        .request_preview_samples(c, preview, vec![0.25, 0.5, 1.], true, false)
        .unwrap();
    let mut workers = Manual::default();
    assert_eq!(step(&mut rt, &mut workers), job);
    assert!(matches!(
        rt.preview_state(c, preview),
        Err(RuntimeError::Busy)
    ));
    rt.cancel_job(c, job).unwrap();
    assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.preview_state(c, preview).unwrap().1.time, 0.25);
    rt.release_job(c, job).unwrap();
    rt.release_preview(c, preview).unwrap();
    assert_eq!(rt.session_info(s).unwrap(), before);
    assert_eq!(rt.preview_count(), 0);
}
#[test]
fn rejected_submission_preserves_the_client_turn_and_explicit_dispatch_cannot_bypass_it() {
    struct Reject;
    impl Executor for Reject {
        fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
            Err(Box::new(work))
        }
    }
    let (mut rt, a, s) = fixture(2);
    let b = rt.register_client().unwrap();
    let a1 = samples(&mut rt, a, s, 2);
    let b1 = samples(&mut rt, b, s, 1);
    rt.dispatch(&mut Reject);
    assert!(matches!(rt.take_work(b1), Err(RuntimeError::Busy)));
    let mut workers = Manual::default();
    assert_eq!(step(&mut rt, &mut workers), a1);
    assert!(matches!(rt.take_work(a1), Err(RuntimeError::Busy)));
    let work = rt.take_work(b1).unwrap();
    assert!(rt.complete(work.run_quantum()));
    assert_eq!(step(&mut rt, &mut workers), a1);
    assert!(rt.job_info(a, a1).unwrap().state.is_terminal());
}

#[test]
fn client_expiry_drops_a_returned_continuation_and_all_its_reservations() {
    let (mut rt, c, s) = fixture(2);
    let baseline = rt.accounted_bytes();
    let (preview, _) = rt.create_preview(c, s, None, None).unwrap();
    rt.request_preview_samples(c, preview, vec![0.1, 0.2, 0.3], true, false)
        .unwrap();
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    let completion = workers.0.pop_front().unwrap().run_quantum();
    assert!(!completion.is_terminal());
    assert!(rt.accounted_bytes() > baseline);
    rt.release_client(c);
    assert!(rt.complete(completion));
    assert_eq!(rt.preview_count(), 0);
    assert_eq!(rt.live_counts().3, 0);
    assert!(rt.accounted_bytes() <= baseline);
    rt.dispatch(&mut workers);
    assert!(workers.0.is_empty());
}

#[test]
fn detaching_a_view_cancels_its_yielded_preview_without_closing_the_session() {
    let (mut rt, c, s) = fixture(2);
    let before = rt.session_info(s).unwrap();
    let (preview, _) = rt.create_preview(c, s, None, None).unwrap();
    let job = rt
        .request_preview_samples(c, preview, vec![0.1, 0.2], false, false)
        .unwrap();
    let mut workers = Manual::default();
    assert_eq!(step(&mut rt, &mut workers), job);
    assert_eq!(rt.detach_session(c, s), Err(RuntimeError::Busy));
    rt.detach_editor_view(c, s).unwrap();
    assert_eq!(rt.preview_count(), 0);
    assert_eq!(step(&mut rt, &mut workers), job);
    assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.session_info(s).unwrap(), before);
    rt.release_job(c, job).unwrap();
    rt.attach_session(c, s).unwrap();
}

#[test]
fn cancelling_entered_work_does_not_free_its_executor_slot_early() {
    let (mut rt, c, s) = fixture(1);
    let first = samples(&mut rt, c, s, 2);
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    let entered = workers.0.pop_front().unwrap();
    rt.cancel_job(c, first).unwrap();
    let second = samples(&mut rt, c, s, 1);
    assert!(matches!(rt.take_work(second), Err(RuntimeError::Busy)));
    rt.dispatch(&mut workers);
    assert!(workers.0.is_empty());
    assert!(rt.complete(entered.run_quantum()));
    assert_eq!(rt.job_info(c, first).unwrap().state, JobState::Cancelled);
    assert_eq!(step(&mut rt, &mut workers), second);
    assert_eq!(rt.job_info(c, second).unwrap().state, JobState::Succeeded);
}

#[test]
fn queued_ownership_cancellation_restores_session_without_an_executor_slot() {
    let (mut rt, c, s) = fixture(1);
    let dir = tempfile::tempdir().unwrap();
    let active = rt.request_save(c, s, &dir.path().join("active")).unwrap();
    let entered = rt.take_work(active).unwrap();
    let other = rt
        .create_session(
            c,
            &uuid::Uuid::new_v4().to_string(),
            zipzi_core::ModelSetup::default(),
        )
        .unwrap();
    let before = rt.session_info(other).unwrap();
    let path = dir.path().join("rejected");
    let queued = rt.request_save(c, other, &path).unwrap();
    assert!(matches!(rt.take_work(queued), Err(RuntimeError::Busy)));
    rt.cancel_queued_job(c, queued).unwrap();
    assert_eq!(rt.job_info(c, queued).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.session_info(other).unwrap(), before);
    assert!(!path.exists());
    rt.release_job(c, queued).unwrap();
    assert!(matches!(
        rt.cancel_queued_job(c, active),
        Err(RuntimeError::Busy)
    ));
    assert_eq!(rt.job_info(c, active).unwrap().state, JobState::Running);
    assert!(rt.complete(entered.run()));
    // The rejected target can be reserved again immediately after capacity returns.
    let retry = rt.request_save(c, other, &path).unwrap();
    let work = rt.take_work(retry).unwrap();
    assert!(rt.complete(work.run()));
    assert_eq!(rt.job_info(c, retry).unwrap().state, JobState::Succeeded);
}
