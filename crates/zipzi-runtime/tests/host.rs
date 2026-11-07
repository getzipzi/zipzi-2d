//! Owner-thread/worker behavior for resident immutable Sessions.
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};
use zipzi_core::{ModelSetup, Parameter};
use zipzi_runtime::{
    host::{HostError, LocalHost},
    *,
};

fn setup() -> (Arc<LocalHost>, ClientId, SessionKey) {
    let host = Arc::new(LocalHost::new(Default::default(), 16).unwrap());
    let client = host.call(|rt| rt.register_client()).unwrap();
    let session = host
        .call(move |rt| {
            rt.create_session(
                client,
                "71000000-0000-4000-8000-000000000001",
                ModelSetup::default(),
            )
        })
        .unwrap();
    (host, client, session)
}

#[test]
fn owner_reads_formal_state_while_worker_mutates_draft_and_delivers_without_polling() {
    let (host, client, session) = setup();
    let version = host
        .call(move |rt| Ok(rt.session_read(client, session)?.version()))
        .unwrap();
    let draft = host
        .call(move |rt| rt.begin_draft(client, version))
        .unwrap();
    let (entered, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let job = host
        .call(move |rt| {
            rt.request_batch(client, draft, TaskBudget::default(), move |commands, _| {
                entered.send(()).unwrap();
                resume.recv().unwrap();
                commands.create_parameter(Parameter {
                    id: "71000000-0000-4000-8000-000000000002".into(),
                    ..Default::default()
                })
            })
        })
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let snapshot = host
        .call(move |rt| rt.read_snapshot(client, session))
        .unwrap();
    assert_eq!(snapshot.source, QuerySource::Published(version));
    assert!(snapshot.snapshot.document().parameter_order().is_empty());
    release.send(()).unwrap();
    let done = host.wait_job(client, job).unwrap();
    let Some(Ok(JobValue::DraftUpdated(source))) = done.result else {
        panic!("missing candidate completion")
    };
    host.call(move |rt| rt.publish(client, source, "create"))
        .unwrap();
    assert_eq!(
        host.call(move |rt| Ok(rt.session_read(client, session)?.history_state().undo_steps))
            .unwrap(),
        1
    );
    host.close().unwrap();
}

#[test]
fn close_joins_entered_workers_and_cancels_cooperative_continuations() {
    let (host, client, session) = setup();
    let source = host
        .call(move |rt| Ok(rt.read_snapshot(client, session)?.source))
        .unwrap();
    let (entered, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let mut first = true;
    host.call(move |rt| {
        rt.request_read_steps(
            client,
            source,
            TaskBudget {
                workspace_bytes: 1024,
                result_bytes: 0,
            },
            move |_, _| {
                if first {
                    first = false;
                    entered.send(()).unwrap();
                    resume.recv().unwrap();
                }
                Ok(ReadProgress::Yield)
            },
        )
    })
    .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let closing = host.clone();
    let (finished, finish) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        finished.send(closing.close()).unwrap();
    });
    assert!(finish.recv_timeout(Duration::from_millis(30)).is_err());
    release.send(()).unwrap();
    finish
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    thread.join().unwrap();
    assert!(matches!(host.call(|_| Ok(())), Err(HostError::Closed)));
    host.close().unwrap();
}

#[test]
fn failed_command_and_foreign_waiter_do_not_poison_the_host() {
    let (host, client, session) = setup();
    let other = host.call(|rt| rt.register_client()).unwrap();
    let version = host
        .call(move |rt| Ok(rt.session_read(client, session)?.version()))
        .unwrap();
    let draft = host
        .call(move |rt| rt.begin_draft(client, version))
        .unwrap();
    let job = host
        .call(move |rt| {
            rt.request_batch(client, draft, TaskBudget::default(), |_, _| {
                Err(zipzi_sdk::Status::error("TEST_REJECT", "rejected"))
            })
        })
        .unwrap();
    assert!(
        matches!(host.wait_job(other, job), Err(HostError::Runtime(e)) if e.code == "NOT_OWNER")
    );
    let done = host.wait_job(client, job).unwrap();
    assert!(matches!(done.result, Some(Err(e)) if e.code == "TEST_REJECT"));
    assert_eq!(
        host.call(move |rt| Ok(rt.session_read(client, session)?.version()))
            .unwrap(),
        version
    );
    host.close().unwrap();
}
