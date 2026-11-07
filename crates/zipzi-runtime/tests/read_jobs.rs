use zipzi_runtime::*;

fn fixture() -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(RuntimeLimits {
        running_jobs: 2,
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
    let QuerySource::Published(version) = rt.read_snapshot(client, session).unwrap().source else {
        panic!("expected published source")
    };
    (rt, client, version)
}

fn budget() -> TaskBudget {
    TaskBudget {
        workspace_bytes: 1024,
        result_bytes: 1024,
    }
}

#[test]
fn job_blob_shares_the_result_lease_and_outlives_job_release() {
    let (mut rt, client, version) = fixture();
    let job = rt
        .request_read(
            client,
            QuerySource::Published(version),
            TaskBudget {
                workspace_bytes: 0,
                result_bytes: 4096,
            },
            |_, _| Ok(vec![19; 4096]),
        )
        .unwrap();
    let completion = rt.take_work(job).unwrap().run();
    rt.complete(completion).unwrap();
    let retained = rt.task_bytes();
    let info = rt.read_job_blob(client, job).unwrap();
    let blob = serde_json::from_value(info["blob"].clone()).unwrap();
    assert_eq!(
        rt.task_bytes(),
        retained,
        "readback must not copy/re-reserve the entire result"
    );
    assert!(
        rt.append_blob(client, blob, 0, &[1]).is_err(),
        "readbacks must stay immutable"
    );
    rt.release_job(client, job).unwrap();
    let escaped = rt.blob(client, blob).unwrap();
    assert_eq!(escaped.bytes(), vec![19; 4096]);
    rt.release_blob(client, blob).unwrap();
    assert_eq!(
        rt.task_bytes(),
        retained,
        "last escaped reader owns the allocation lease"
    );
    drop(escaped);
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn shared_job_readback_still_enforces_blob_handle_limit() {
    let (mut rt, client, version) = fixture();
    let job = rt
        .request_read(client, QuerySource::Published(version), budget(), |_, _| {
            Ok(vec![1])
        })
        .unwrap();
    let completion = rt.take_work(job).unwrap().run();
    rt.complete(completion).unwrap();
    for _ in 0..64 {
        rt.read_job_blob(client, job).unwrap();
    }
    assert_eq!(
        rt.read_job_blob(client, job).unwrap_err().code,
        "RESOURCE_LIMIT"
    );
    rt.release_client(client);
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn frozen_reads_remain_available_and_late_results_keep_their_original_source() {
    let (mut rt, client, before) = fixture();
    let source = QuerySource::Published(before);
    let first = rt
        .request_read(client, source, budget(), |snapshot, _| {
            Ok(vec![snapshot.snapshot.document().part_order().len() as u8])
        })
        .unwrap();
    let second = rt
        .request_read(client, source, budget(), |_, _| Ok(vec![]))
        .unwrap();
    let read = rt.take_work(first).unwrap();
    assert_eq!(rt.job(client, first).unwrap().state, JobState::Running);
    assert_eq!(rt.job(client, second).unwrap().state, JobState::Queued);
    assert_eq!(
        rt.read_snapshot(client, before.session).unwrap().source,
        source
    );
    let draft = rt.begin_draft(client, before).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (draft, _) = rt
        .batch(client, draft, |c| {
            c.create_part(zipzi_core::Part {
                runtime_id: id.clone(),
                id,
                name: "Part".into(),
                ..Default::default()
            })
        })
        .unwrap();
    let publish = rt
        .request_publish(client, draft, "New content", Default::default())
        .unwrap();
    let completion = rt.take_work(publish).unwrap().run();
    rt.complete(completion).unwrap();
    let after = rt.subscribe(client, before.session).unwrap();
    assert_ne!(after.snapshot.source, source);
    assert_eq!(after.history.undo_steps, 1);
    rt.complete(read.run()).unwrap();
    let Some(Ok(JobValue::Read(result))) = &rt.job(client, first).unwrap().result else {
        panic!("expected read result")
    };
    assert_eq!(result.source, source);
    assert_eq!(result.bytes(), &[0]);
    assert_eq!(
        rt.read_snapshot(client, before.session).unwrap().source,
        after.snapshot.source
    );
    rt.cancel_job(client, second).unwrap();
    let completion = rt.take_work(second).unwrap().run();
    rt.complete(completion).unwrap();
    assert_eq!(rt.job(client, second).unwrap().state, JobState::Cancelled);
    for job in [first, second, publish] {
        rt.release_job(client, job).unwrap();
    }
}

#[test]
fn cancelled_reads_keep_their_reservations_until_completion() {
    let (mut rt, client, version) = fixture();
    let before = rt.task_bytes();
    let job = rt
        .request_read(client, QuerySource::Published(version), budget(), |_, _| {
            panic!("cancelled read entered computation")
        })
        .unwrap();
    let reserved = rt.task_bytes();
    assert!(reserved > before);
    rt.cancel_job(client, job).unwrap();
    assert_eq!(rt.task_bytes(), reserved);
    assert_eq!(rt.release_job(client, job).unwrap_err().code, "JOB_BUSY");
    let completion = rt.take_work(job).unwrap().run();
    rt.complete(completion).unwrap();
    assert_eq!(rt.job(client, job).unwrap().state, JobState::Cancelled);
    rt.release_job(client, job).unwrap();
    assert_eq!(rt.task_bytes(), before);
}

#[test]
fn draft_read_completion_keeps_the_captured_sequence_after_another_batch() {
    let (mut rt, client, version) = fixture();
    let source = rt.begin_draft(client, version).unwrap();
    let job = rt
        .request_read(
            client,
            QuerySource::Draft(source),
            budget(),
            |snapshot, _| Ok(snapshot.snapshot.document().id().as_bytes().to_vec()),
        )
        .unwrap();
    let (current, _) = rt.batch(client, source, |_| Ok(())).unwrap();
    let completion = rt.take_work(job).unwrap().run();
    rt.complete(completion).unwrap();
    let Some(Ok(JobValue::Read(result))) = &rt.job(client, job).unwrap().result else {
        panic!("expected read result")
    };
    assert_eq!(result.source, QuerySource::Draft(source));
    assert_eq!(
        rt.read_draft(client, source).unwrap_err().code,
        "STALE_SOURCE"
    );
    assert_eq!(
        rt.read_draft(client, current).unwrap().source,
        QuerySource::Draft(current)
    );
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        0
    );
    rt.release_job(client, job).unwrap();
}
