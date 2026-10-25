use std::collections::VecDeque;
use zipzi_runtime::authoring::ModelingSampleQuery;
use zipzi_runtime::*;
use zipzi_sdk::*;
#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}
fn fixture() -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::default();
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(
            c,
            &uuid::Uuid::new_v4().to_string(),
            zipzi_core::Canvas::default(),
        )
        .unwrap();
    (rt, c, s)
}
fn input(session: SessionId) -> ModelingSampleQuery {
    ModelingSampleQuery {
        session,
        edit: None,
        parameters: vec![Default::default()],
        checks: Default::default(),
        expected: None,
    }
}
#[test]
fn frozen_reads_keep_the_formal_ready_and_keep_one_worker_for_publication() {
    let (mut rt, c, s) = fixture();
    let before = rt.session_info(s).unwrap();
    let first = rt.request_modeling_samples(c, input(s)).unwrap();
    let second = rt.request_modeling_samples(c, input(s)).unwrap();
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    assert_eq!(workers.0.len(), 1);
    assert_eq!(rt.job_info(c, first).unwrap().state, JobState::Running);
    assert_eq!(rt.job_info(c, second).unwrap().state, JobState::Queued);
    assert_eq!(rt.session_info(s).unwrap().version, before.version);
    assert_eq!(rt.session_info(s).unwrap().state, SessionState::Ready);
    let edit = rt.begin_edit(c, s, "new content", None).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    rt.apply_edit(c, edit, |e| {
        e.create_part(zipzi_core::Part {
            id: id.clone(),
            name: "Part".into(),
            ..Default::default()
        })
    })
    .unwrap();
    let publish = rt.request_commit(c, edit).unwrap();
    rt.dispatch(&mut workers);
    assert_eq!(workers.0.len(), 2);
    assert_eq!(rt.job_info(c, publish).unwrap().state, JobState::Running);
    // Publish first, then deliver the earlier fixed read. Its old source is
    // intentional and cannot replace or lock the newer formal state.
    rt.complete(workers.0.pop_back().unwrap().run());
    let after = rt.session_info(s).unwrap();
    assert_ne!(after.version, before.version);
    assert!(rt.complete(workers.0.pop_front().unwrap().run()));
    let info = rt.job_info(c, first).unwrap();
    let JobValue::Sampled(batch) = info.result.unwrap().unwrap() else {
        panic!("sample result")
    };
    assert_eq!(batch.source.version(), before.version);
    assert_eq!(rt.session_info(s).unwrap().version, after.version);
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (1, 0));
    rt.cancel_job(c, second).unwrap();
    assert_eq!(rt.job_info(c, second).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.session_info(s).unwrap().state, SessionState::Ready);
    rt.release_job(c, first).unwrap();
    rt.release_job(c, second).unwrap();
    rt.release_job(c, publish).unwrap();
}
#[test]
fn cancelled_plan_jobs_release_the_snapshot_and_do_not_register_plans() {
    let (mut rt, c, s) = fixture();
    let base = rt.accounted_bytes();
    let job = rt
        .request_modeling_plan(
            c,
            s,
            None,
            ModelingIntent::Document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::Layout(None),
                value: DocumentValue::Layout(Some(Default::default())),
            }),
        )
        .unwrap();
    assert!(rt.accounted_bytes() > base);
    rt.cancel_job(c, job).unwrap();
    assert_eq!(rt.job_info(c, job).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.modeling_plan_count(), 0);
    rt.release_job(c, job).unwrap();
    assert_eq!(rt.accounted_bytes(), base);
}
#[test]
fn plan_completion_retains_the_original_candidate_sequence() {
    let (mut rt, c, s) = fixture();
    let edit = rt.begin_edit(c, s, "candidate", None).unwrap();
    let job = rt
        .request_modeling_plan(
            c,
            s,
            Some(edit),
            ModelingIntent::Document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::Layout(None),
                value: DocumentValue::Layout(Some(Default::default())),
            }),
        )
        .unwrap();
    rt.apply_edit(c, edit, |_| Ok(())).unwrap();
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    rt.complete(workers.0.pop_front().unwrap().run());
    let JobValue::Planned(result) = rt.job_info(c, job).unwrap().result.unwrap().unwrap() else {
        panic!("plan result")
    };
    assert!(matches!(result.source, SourceRef::Edit { sequence: 0, .. }));
    assert!(
        matches!(rt.apply_modeling_plan(c,edit,result.plan),Err(RuntimeError::Sdk(e)) if e.code.as_ref()=="STALE_PLAN")
    );
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (0, 0));
    rt.release_modeling_plan(c, result.plan).unwrap();
    rt.release_job(c, job).unwrap();
}
