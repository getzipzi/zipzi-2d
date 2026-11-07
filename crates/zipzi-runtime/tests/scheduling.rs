use std::collections::VecDeque;
use zipzi_runtime::*;

#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}

fn fixture(workers: usize) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(RuntimeLimits {
        running_jobs: workers,
        ..Default::default()
    });
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    (rt, client, version)
}

fn read(rt: &mut Runtime, client: ClientId, version: Version, count: u8) -> JobId {
    let mut steps = 0;
    rt.request_read_steps(
        client,
        QuerySource::Published(version),
        TaskBudget {
            workspace_bytes: 128,
            result_bytes: 16,
        },
        move |_, cancel| {
            cancel.check()?;
            steps += 1;
            Ok(if steps == count {
                ReadProgress::Complete(vec![steps])
            } else {
                ReadProgress::Yield
            })
        },
    )
    .unwrap()
}

fn step(rt: &mut Runtime) -> JobId {
    let work = rt.next_work().unwrap();
    let id = work.job_id();
    rt.complete(work.run()).unwrap();
    id
}

#[test]
fn cooperative_reads_return_to_the_queue_and_yield_to_other_clients() {
    let (mut rt, a, version) = fixture(1);
    let b = rt.register_client().unwrap();
    rt.attach(b, version.session).unwrap();
    let batch = read(&mut rt, a, version, 3);
    let one = read(&mut rt, b, version, 1);
    assert_eq!(step(&mut rt), batch);
    assert_eq!(rt.job(a, batch).unwrap().state, JobState::Queued);
    assert_eq!(step(&mut rt), one);
    assert_eq!(rt.job(b, one).unwrap().state, JobState::Succeeded);
    assert_eq!(step(&mut rt), batch);
    assert_eq!(step(&mut rt), batch);
    let Some(Ok(JobValue::Read(result))) = &rt.job(a, batch).unwrap().result else {
        panic!("expected read result")
    };
    assert_eq!(result.source, QuerySource::Published(version));
    assert_eq!(result.bytes(), [3]);
    assert_eq!(rt.subscribe(a, version.session).unwrap().cursor, version);
}

#[test]
fn a_yielded_read_leaves_one_worker_available_for_publication() {
    let (mut rt, client, version) = fixture(1);
    let read = read(&mut rt, client, version, 64);
    assert_eq!(step(&mut rt), read);
    let draft = rt.begin_draft(client, version).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (draft, ()) = rt
        .batch(client, draft, |c| {
            c.create_part(zipzi_core::Part {
                id: id.clone(),
                runtime_id: id,
                name: "Part".into(),
                ..Default::default()
            })
        })
        .unwrap();
    let publish = rt
        .request_publish(client, draft, "Create part", Default::default())
        .unwrap();
    let work = rt.take_work(publish).unwrap();
    rt.complete(work.run()).unwrap();
    assert_eq!(rt.job(client, publish).unwrap().state, JobState::Succeeded);
    assert_eq!(rt.job(client, read).unwrap().state, JobState::Queued);
    rt.cancel_job(client, read).unwrap();
    assert_eq!(step(&mut rt), read);
    assert_eq!(rt.job(client, read).unwrap().state, JobState::Cancelled);
    assert!(rt.next_work().is_none());
}

#[test]
fn rejected_executor_submission_requeues_the_same_work() {
    struct Reject(Vec<JobId>);
    impl Executor for Reject {
        fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
            self.0.push(work.job_id());
            Err(Box::new(work))
        }
    }
    let (mut rt, client, version) = fixture(1);
    let first = read(&mut rt, client, version, 2);
    let second = read(&mut rt, client, version, 1);
    let mut reject = Reject(Vec::new());
    rt.dispatch(&mut reject);
    assert_eq!(reject.0, [first]);
    assert_eq!(rt.job(client, first).unwrap().state, JobState::Queued);
    assert_eq!(step(&mut rt), first);
    assert_eq!(step(&mut rt), second);
    assert_eq!(step(&mut rt), first);
    assert_eq!(rt.job(client, first).unwrap().state, JobState::Succeeded);
}

#[test]
fn client_release_retains_yielded_work_and_leases_until_worker_cleanup() {
    let (mut rt, client, version) = fixture(1);
    let job = read(&mut rt, client, version, 3);
    let completion = rt.take_work(job).unwrap().run();
    let reserved = rt.task_bytes();
    assert!(reserved > 0);
    rt.release_client(client);
    rt.complete(completion).unwrap();
    assert_eq!(rt.outstanding_job_ids(), [job]);
    assert_eq!(rt.task_bytes(), reserved);
    assert_eq!(step(&mut rt), job);
    assert!(rt.outstanding_job_ids().is_empty());
    assert_eq!(rt.task_bytes(), 0);
    let peer = rt.register_client().unwrap();
    rt.attach(peer, version.session).unwrap();
    assert_eq!(rt.subscribe(peer, version.session).unwrap().cursor, version);
}

#[test]
fn detaching_a_client_cancels_its_read_without_closing_the_session() {
    let (mut rt, client, version) = fixture(1);
    let job = read(&mut rt, client, version, 2);
    assert_eq!(step(&mut rt), job);
    rt.detach(client, version.session).unwrap();
    assert_eq!(rt.job(client, job).unwrap().state, JobState::Cancelling);
    assert_eq!(rt.release_job(client, job).unwrap_err().code, "JOB_BUSY");
    assert_eq!(step(&mut rt), job);
    assert_eq!(rt.job(client, job).unwrap().state, JobState::Cancelled);
    rt.release_job(client, job).unwrap();
    assert_eq!(rt.task_bytes(), 0);
    rt.attach(client, version.session).unwrap();
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[test]
fn cancelling_entered_work_does_not_free_its_executor_slot_early() {
    let (mut rt, client, version) = fixture(1);
    let first = read(&mut rt, client, version, 2);
    let mut workers = Manual::default();
    rt.dispatch(&mut workers);
    let entered = workers.0.pop_front().unwrap();
    rt.cancel_job(client, first).unwrap();
    let second = read(&mut rt, client, version, 1);
    assert_eq!(rt.take_work(second).unwrap_err().code, "JOB_BUSY");
    rt.dispatch(&mut workers);
    assert!(workers.0.is_empty());
    rt.complete(entered.run()).unwrap();
    assert_eq!(rt.job(client, first).unwrap().state, JobState::Cancelled);
    assert_eq!(step(&mut rt), second);
    assert_eq!(rt.job(client, second).unwrap().state, JobState::Succeeded);
}

#[test]
fn cancelling_a_queued_save_releases_its_target_after_worker_completion() {
    let (mut rt, client, version) = fixture(1);
    let active = read(&mut rt, client, version, 1);
    let entered = rt.take_work(active).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queued.zipzi");
    let io = rt.project_io().clone();
    let queued = rt
        .request_save_as(client, version, &io, &path, None, Default::default())
        .unwrap();
    assert_eq!(rt.take_work(queued).unwrap_err().code, "JOB_BUSY");
    rt.cancel_job(client, queued).unwrap();
    assert_eq!(rt.job(client, queued).unwrap().state, JobState::Cancelling);
    assert_eq!(
        rt.saving_job(client, version.session).unwrap(),
        Some(queued)
    );
    assert!(!path.exists());
    rt.complete(entered.run()).unwrap();
    assert_eq!(step(&mut rt), queued);
    assert_eq!(rt.job(client, queued).unwrap().state, JobState::Cancelled);
    assert!(rt.saving_job(client, version.session).unwrap().is_none());
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
    assert!(!path.exists());
    let retry = rt
        .request_save_as(client, version, &io, &path, None, Default::default())
        .unwrap();
    assert_eq!(step(&mut rt), retry);
    assert_eq!(rt.job(client, retry).unwrap().state, JobState::Succeeded);
    assert!(path.is_file());
}
