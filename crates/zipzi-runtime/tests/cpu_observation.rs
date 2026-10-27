#![cfg(not(feature = "gpu-observation"))]

use zipzi_runtime::{observation::ObservationView, *};

#[test]
fn cpu_host_rejects_images_before_admission_and_keeps_preview_usable() {
    let mut rt = Runtime::default();
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(
            c,
            "79660000-0000-4000-8000-000000000001",
            zipzi_core::ModelSetup::default(),
        )
        .unwrap();
    let capabilities = rt.runtime_capabilities();
    assert_eq!(capabilities.gpu, Some(false));
    assert_eq!(capabilities.gpu_backend, "None");
    assert_eq!(capabilities.gpu_error.as_deref(), Some("GPU_UNAVAILABLE"));
    assert!(!capabilities.domains.iter().any(|d| d == "observation"));
    let (p, _) = rt.create_preview(c, s, None, None).unwrap();
    let before = rt.session_info(s).unwrap();
    let preview = rt.preview_state(c, p).unwrap();
    let bytes = rt.accounted_bytes();
    let view = ObservationView {
        width: 16,
        height: 16,
        roi: [0., 0., 1., 1.],
        padding_model: 0.,
    };
    for result in [
        rt.request_observation(c, s, None, vec![Default::default()], view, None),
        rt.request_preview_frames(c, p, vec![0., 0.25], false, false, vec![], Some(view)),
    ] {
        assert!(
            matches!(result, Err(RuntimeError::Sdk(e)) if e.code.as_ref() == "GPU_UNAVAILABLE")
        );
    }
    assert_eq!(rt.accounted_bytes(), bytes);
    assert_eq!(rt.session_info(s).unwrap(), before);
    assert_eq!(
        rt.preview_state(c, p).unwrap().1.sequence,
        preview.1.sequence
    );
    let job = rt
        .request_preview_samples(c, p, vec![0., 0.25], true, false)
        .unwrap();
    let work = rt.take_work(job).unwrap();
    assert!(rt.complete(work.run()));
    let result = rt.job_info(c, job).unwrap().result.unwrap().unwrap();
    let JobValue::Previewed(result) = result else {
        panic!("preview result")
    };
    assert_eq!(result.samples.frames.len(), 2);
    assert!(result
        .samples
        .frames
        .iter()
        .all(|f| f.observation.is_none()));
    rt.release_job(c, job).unwrap();
}
