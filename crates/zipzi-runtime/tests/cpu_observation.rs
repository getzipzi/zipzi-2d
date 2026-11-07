#![cfg(not(feature = "gpu-observation"))]

use zipzi_contract::{
    kernel_tasks::{SampleInput, SampleRequest},
    observation::ObservationView,
};
use zipzi_runtime::{JobValue, Runtime};

#[test]
fn cpu_host_rejects_images_before_admission_and_keeps_sampling_usable() {
    let mut runtime = Runtime::default();
    let client = runtime.register_client().unwrap();
    let session = runtime
        .create_session(
            client,
            "79660000-0000-4000-8000-000000000001",
            Default::default(),
        )
        .unwrap();
    let before = runtime.read_snapshot(client, session).unwrap();
    let bytes = runtime.task_bytes();
    let view = ObservationView {
        texture_policy: Default::default(),
        width: 16,
        height: 16,
        roi: [0., 0., 1., 1.],
        padding_model: 0.,
        topology: None,
        quality: false,
    };
    let input = SampleInput::Animation {
        times: vec![0., 0.25],
        motions: vec![],
        expressions: vec![],
        inputs: vec![],
        physics: false,
        stabilized_start: false,
    };
    let failure = runtime
        .request_observation(client, before.source, input.clone(), view)
        .unwrap_err();
    assert_eq!(failure.code, "UNSUPPORTED_OPERATION");
    assert_eq!(runtime.task_bytes(), bytes);
    assert!(runtime.next_work().is_none());
    let after = runtime.read_snapshot(client, session).unwrap();
    assert_eq!(after.source, before.source);
    assert_eq!(after.snapshot.content_id(), before.snapshot.content_id());

    let job = runtime
        .request_samples(
            client,
            before.source,
            SampleRequest {
                input,
                queries: vec![],
                quality: false,
            },
        )
        .unwrap();
    while let Some(work) = runtime.next_work() {
        runtime.complete(work.run()).unwrap();
    }
    let result = runtime.job(client, job).unwrap().result.as_ref().unwrap();
    let JobValue::Read(result) = result.as_ref().unwrap() else {
        panic!("expected a read result");
    };
    assert_eq!(result.source, before.source);
    let value: serde_json::Value = serde_json::from_slice(result.bytes()).unwrap();
    let frames = value["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 2);
    assert!(frames.iter().all(|frame| frame["animation"].is_object()));
    runtime.release_job(client, job).unwrap();
    assert_eq!(runtime.task_bytes(), bytes);
}
