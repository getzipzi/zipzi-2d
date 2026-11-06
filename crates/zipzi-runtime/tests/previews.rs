use serde_json::Value;
use zipzi_contract::kernel_tasks::{ParameterActivation, SampleInput, SampleRequest};
use zipzi_runtime::*;

fn id(n: u32) -> String {
    format!("79630000-0000-4000-8000-{n:012x}")
}

fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| c.create_parameter(parameter(2, 0.)))
        .unwrap();
    let version = rt.publish(client, draft, "Parameter").unwrap().version;
    (rt, client, version)
}

fn parameter(n: u32, default_value: f32) -> zipzi_core::Parameter {
    zipzi_core::Parameter {
        id: id(n),
        runtime_id: id(n),
        name: "P".into(),
        minimum: -1.,
        maximum: 1.,
        default_value,
        ..Default::default()
    }
}

fn animation(times: Vec<f32>, value: Option<f32>, motions: usize) -> SampleRequest {
    SampleRequest {
        input: SampleInput::Animation {
            times,
            motions: vec![(id(2), 0.); motions],
            expressions: vec![],
            inputs: value
                .into_iter()
                .map(|value| ParameterActivation {
                    id: id(2),
                    time: 0.,
                    value,
                })
                .collect(),
            physics: false,
            stabilized_start: false,
        },
        queries: vec![],
        quality: false,
    }
}

fn complete(rt: &mut Runtime, job: JobId) {
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
}

fn finish(rt: &mut Runtime, client: ClientId, job: JobId) -> Result<ReadResult> {
    while !rt.job(client, job).unwrap().state.is_terminal() {
        complete(rt, job);
    }
    let value = rt.job(client, job).unwrap().result.clone().unwrap();
    rt.release_job(client, job).unwrap();
    let JobValue::Read(value) = value? else {
        panic!("expected sample result")
    };
    Ok(value)
}

fn frames(result: &ReadResult) -> Vec<Value> {
    let value: Value = serde_json::from_slice(result.bytes()).unwrap();
    value["frames"].as_array().unwrap().clone()
}

#[test]
fn independent_animation_requests_keep_frozen_inputs_and_formal_history() {
    let (mut rt, client, version) = fixture(Default::default());
    let peer = rt.register_client().unwrap();
    rt.attach(peer, version.session).unwrap();
    let source = QuerySource::Published(version);
    let a = rt
        .request_samples(
            client,
            source,
            animation(vec![0., 0.5, 0.25, 0.5], Some(0.75), 0),
        )
        .unwrap();
    let b = rt
        .request_samples(client, source, animation(vec![0.5], Some(-0.5), 0))
        .unwrap();
    let remote = rt
        .request_samples(peer, source, animation(vec![0.5], Some(0.25), 0))
        .unwrap();
    assert_eq!(rt.cancel_job(peer, a).unwrap_err().code, "NOT_OWNER");
    complete(&mut rt, a);
    assert_eq!(rt.job(client, a).unwrap().state, JobState::Queued);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| c.create_parameter(parameter(3, 0.5)))
        .unwrap();
    let after = rt.publish(client, draft, "New parameter").unwrap();
    let result = finish(&mut rt, client, a).unwrap();
    assert_eq!(result.source, source);
    let sampled = frames(&result);
    let times: Vec<_> = sampled
        .iter()
        .map(|f| f["animation"]["time"].as_f64().unwrap())
        .collect();
    assert_eq!(times, [0., 0.5, 0.25, 0.5]);
    for frame in &sampled {
        assert_eq!(frame["animation"]["parameters"][id(2)], 0.75);
        assert!(frame["animation"]["parameters"].get(id(3)).is_none());
        assert!(frame["geometry"].is_object());
    }
    assert_eq!(
        frames(&finish(&mut rt, client, b).unwrap())[0]["animation"]["parameters"][id(2)],
        -0.5
    );
    assert_eq!(
        frames(&finish(&mut rt, peer, remote).unwrap())[0]["animation"]["parameters"][id(2)],
        0.25
    );
    let fresh = rt
        .request_samples(
            client,
            QuerySource::Published(after.version),
            animation(vec![0.], None, 0),
        )
        .unwrap();
    assert_eq!(
        frames(&finish(&mut rt, client, fresh).unwrap())[0]["animation"]["parameters"][id(3)],
        0.5
    );
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        2
    );
}

#[test]
fn cancelled_and_invalid_samples_release_all_task_bytes() {
    let (mut rt, client, version) = fixture(Default::default());
    let source = QuerySource::Published(version);
    let before = rt.task_bytes();
    assert_eq!(
        rt.request_samples(client, source, animation(vec![0.; 65], None, 0))
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    for _ in 0..100 {
        let job = rt
            .request_samples(client, source, animation(vec![100.], None, 0))
            .unwrap();
        rt.cancel_job(client, job).unwrap();
        assert_eq!(finish(&mut rt, client, job).unwrap_err().code, "CANCELLED");
        assert_eq!(rt.task_bytes(), before);
    }
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[test]
fn jobs_capture_drafts_across_sequence_changes_and_release_on_client_expiry() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| c.create_parameter(parameter(3, 0.5)))
        .unwrap();
    let source = QuerySource::Draft(draft);
    let job = rt
        .request_samples(client, source, animation(vec![0.5], None, 0))
        .unwrap();
    let cleanup = rt
        .request_samples(client, source, animation(vec![1., 2.], None, 0))
        .unwrap();
    let (current, ()) = rt.batch(client, draft, |_| Ok(())).unwrap();
    rt.cancel_draft(client, current.draft).unwrap();
    let result = finish(&mut rt, client, job).unwrap();
    assert_eq!(result.source, source);
    assert_eq!(frames(&result)[0]["animation"]["parameters"][id(3)], 0.5);
    drop(result);
    let entered = rt.take_work(cleanup).unwrap();
    rt.release_client(client);
    rt.complete(entered.run()).unwrap();
    assert!(rt.outstanding_job_ids().is_empty());
    assert_eq!(rt.task_bytes(), 0);
    let peer = rt.register_client().unwrap();
    rt.attach(peer, version.session).unwrap();
    let formal = rt.subscribe(peer, version.session).unwrap();
    assert_eq!(formal.cursor, version);
    assert!(formal
        .snapshot
        .snapshot
        .document()
        .parameter(&id(3))
        .is_none());
}

#[test]
fn cancellation_before_result_installation_discards_a_completed_read() {
    let (mut rt, client, version) = fixture(Default::default());
    let job = rt
        .request_samples(
            client,
            QuerySource::Published(version),
            animation(vec![0.5], None, 0),
        )
        .unwrap();
    let completion = rt.take_work(job).unwrap().run();
    rt.cancel_job(client, job).unwrap();
    rt.complete(completion).unwrap();
    assert_eq!(rt.job(client, job).unwrap().state, JobState::Cancelled);
    rt.release_job(client, job).unwrap();
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

fn event_fixture(limits: RuntimeLimits, payload: usize) -> (Runtime, ClientId, Version) {
    use zipzi_core::document::{MotionClip, MotionEvent};
    let mut rt = Runtime::new(limits);
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(c, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let draft = rt
        .begin_draft(c, rt.subscribe(c, s).unwrap().cursor)
        .unwrap();
    let (draft, ()) = rt
        .batch(c, draft, |edit| {
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
    let version = rt.publish(c, draft, "Events").unwrap().version;
    (rt, c, version)
}

#[test]
fn repeated_motion_event_expansion_is_bounded_before_advancing() {
    let (mut rt, client, version) = event_fixture(
        RuntimeLimits {
            result_bytes: 1 << 20,
            ..Default::default()
        },
        64 << 10,
    );
    let source = QuerySource::Published(version);
    assert_eq!(
        rt.request_samples(client, source, animation(vec![2. / 60.], None, 512))
            .unwrap_err()
            .code,
        "INVALID_SAMPLES"
    );
    assert!(rt.next_work().is_none());
    let job = rt
        .request_samples(client, source, animation(vec![2. / 60.], None, 128))
        .unwrap();
    assert_eq!(
        finish(&mut rt, client, job).unwrap_err().code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[test]
fn sampled_events_remain_charged_until_the_last_result_reader_drops() {
    let (mut rt, client, version) = event_fixture(Default::default(), 16 << 10);
    let source = QuerySource::Published(version);
    let before = rt.task_bytes();
    let job = rt
        .request_samples(client, source, animation(vec![2. / 60., 2. / 60.], None, 4))
        .unwrap();
    let admitted = rt.task_bytes();
    let result = finish(&mut rt, client, job).unwrap();
    let sampled = frames(&result);
    assert_eq!(sampled.len(), 2);
    for frame in sampled {
        let events = frame["animation"]["fired_events"].as_array().unwrap();
        assert_eq!(events.len(), 4);
        assert!(events
            .iter()
            .all(|e| e["value"].as_str().unwrap().len() == 16 << 10));
    }
    assert!(rt.task_bytes() > before);
    assert!(rt.task_bytes() < admitted);
    drop(result);
    assert_eq!(rt.task_bytes(), before);
}

#[test]
fn maximum_sample_batch_retains_bounded_output_after_job_release() {
    let (mut rt, client, version) = fixture(Default::default());
    let source = QuerySource::Published(version);
    let job = rt
        .request_samples(client, source, animation(vec![0.; 64], None, 0))
        .unwrap();
    let admitted = rt.task_bytes();
    let result = finish(&mut rt, client, job).unwrap();
    let sampled = frames(&result);
    assert_eq!(sampled.len(), 64);
    assert!(sampled
        .iter()
        .all(|f| f["animation"]["parameters"][id(2)] == 0.));
    assert!(rt.task_bytes() >= result.bytes().len());
    assert!(rt.task_bytes() < admitted);
    drop(result);
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn low_workspace_budget_rejects_before_sampling_or_reserving_work() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        task_bytes: 70_000,
        ..Default::default()
    });
    let source = QuerySource::Published(version);
    assert_eq!(
        rt.request_samples(client, source, animation(vec![1.; 64], None, 0))
            .unwrap_err()
            .code,
        "BUDGET_EXCEEDED"
    );
    assert!(rt.next_work().is_none());
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}
