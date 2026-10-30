#[cfg(feature = "gpu-observation")]
use std::collections::VecDeque;
use zipzi_runtime::authoring::{EditOperation, ResourceOperation};
use zipzi_runtime::*;

fn id(n: u32) -> String {
    format!("79700000-0000-4000-8000-{n:012}")
}
fn view(policy: ObservationTexturePolicy) -> ObservationView {
    ObservationView {
        width: 32,
        height: 32,
        roi: [0., 0., 1., 1.],
        padding_model: 0.,
        quality: true,
        topology: None,
        texture_policy: policy,
    }
}
fn fixture(limits: RuntimeLimits, memory: bool) -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(client, session, "fixture", None).unwrap();
    if memory {
        let image = rt
            .prepare_image(
                client,
                4,
                4,
                (0..16)
                    .flat_map(|i| [255, 20, 100, if i % 2 == 0 { 0 } else { 255 }])
                    .collect(),
            )
            .unwrap()
            .image;
        rt.apply_authoring(
            client,
            edit,
            vec![EditOperation::Resource(ResourceOperation::CreateImage {
                spec: zipzi_sdk::ImageCreateSpec {
                    id: Some(id(2)),
                    name: "texture".into(),
                },
                image,
            })],
        )
        .unwrap();
        rt.release_prepared_image(client, image).unwrap();
    } else {
        rt.apply_edit(client, edit, |e| {
            e.create_asset(zipzi_core::ImageAsset {
                id: id(2),
                name: "unread file".into(),
                unresolved_source: None,
                content: Some(zipzi_core::ImageContent {
                    source: "/missing/observation-estimate.png".into(),
                    width: 4,
                    height: 4,
                    sha256: "b".repeat(64),
                }),
            })
        })
        .unwrap();
    }
    rt.apply_edit(client, edit, |e| {
        e.create_mesh(
            zipzi_sdk::rectangle_mesh(
                &id(3),
                "mesh",
                &id(2),
                zipzi_core::Vec2::new(0., 0.),
                zipzi_core::Vec2::new(1., 1.),
            )
            .unwrap(),
        )
    })
    .unwrap();
    rt.commit_edit(client, edit).unwrap();
    (rt, client, session)
}

#[test]
fn estimates_return_plans_diagnostics_and_sources_without_io_device_or_reservations() {
    let (rt, c, s) = fixture(
        RuntimeLimits {
            observation_texture_rgba_bytes: 16,
            ..Default::default()
        },
        false,
    );
    let before = rt.session_info(s).unwrap();
    let bytes = rt.accounted_bytes();
    let gpu = rt.runtime_capabilities().gpu;
    let original = rt
        .estimate_observation(
            c,
            s,
            None,
            vec![Default::default()],
            view(Default::default()),
        )
        .unwrap();
    assert!(matches!(
        original.texture_planning,
        ObservationTexturePlanning::InsufficientBudget {
            minimum_required_rgba_bytes: 64,
            ..
        }
    ));
    assert!(!original.static_feasible);
    let estimate = rt
        .estimate_observation(
            c,
            s,
            None,
            vec![Default::default()],
            view(ObservationTexturePolicy::FitBudget { max_rgba_bytes: 16 }),
        )
        .unwrap();
    let ObservationTexturePlanning::Ready { plan } = estimate.texture_planning else {
        panic!("fit plan")
    };
    assert_eq!((plan.source_rgba_bytes, plan.render_rgba_bytes), (64, 16));
    assert_eq!(
        (
            plan.textures[0].render_size.width,
            plan.textures[0].render_size.height
        ),
        (2, 2)
    );
    assert!(plan.textures[0].source.source_content_identity.is_some());
    assert!(!estimate.source_identities_verified && !estimate.device_verified);
    assert_eq!(estimate.device_max_texture_dimension, None);
    assert_eq!(estimate.gpu_supported, cfg!(feature = "gpu-observation"));
    assert_eq!(rt.accounted_bytes(), bytes);
    assert_eq!(rt.runtime_capabilities().gpu, gpu);
    assert_eq!(rt.session_info(s).unwrap(), before);
    for max_rgba_bytes in [0, 17] {
        let RuntimeError::Sdk(e) = rt
            .estimate_observation(
                c,
                s,
                None,
                vec![Default::default()],
                view(ObservationTexturePolicy::FitBudget { max_rgba_bytes }),
            )
            .unwrap_err()
        else {
            panic!("invalid policy")
        };
        assert_eq!(e.code.as_ref(), "INVALID_TEXTURE_POLICY");
    }
}

#[test]
fn configurable_limits_have_separate_diagnostics_and_metadata_bound() {
    for (limits, code) in [
        (
            RuntimeLimits {
                observation_frames: 1,
                ..Default::default()
            },
            "OBSERVATION_FRAME_LIMIT",
        ),
        (
            RuntimeLimits {
                observation_max_dimension: 16,
                ..Default::default()
            },
            "OUTPUT_SIZE_LIMIT",
        ),
        (
            RuntimeLimits {
                observation_output_bytes: 1,
                ..Default::default()
            },
            "OBSERVATION_OUTPUT_LIMIT",
        ),
        (
            RuntimeLimits {
                observation_workspace_bytes: 1,
                ..Default::default()
            },
            "DECODE_WORKSPACE_LIMIT",
        ),
    ] {
        let (rt, c, s) = fixture(limits, true);
        let estimate = rt
            .estimate_observation(
                c,
                s,
                None,
                vec![Default::default(); 2],
                view(Default::default()),
            )
            .unwrap();
        assert!(!estimate.static_feasible);
        assert!(
            estimate.issues.iter().any(|issue| issue.code == code),
            "{code}: {:?}",
            estimate.issues
        );
        assert!(matches!(
            estimate.texture_planning,
            ObservationTexturePlanning::Ready { .. }
        ));
    }
    let (rt, c, s) = fixture(
        RuntimeLimits {
            observation_metadata_bytes: 1024,
            ..Default::default()
        },
        true,
    );
    assert!(matches!(
        rt.estimate_observation(
            c,
            s,
            None,
            vec![Default::default()],
            view(Default::default())
        ),
        Err(RuntimeError::Budget("observation metadata"))
    ));
}

#[test]
fn preview_estimates_keep_clock_sequence_and_frozen_source_and_reject_busy() {
    let (mut rt, c, s) = fixture(RuntimeLimits::default(), true);
    let (preview, source) = rt.create_preview(c, s, None, None).unwrap();
    let before = rt.preview_state(c, preview).unwrap();
    let bytes = rt.accounted_bytes();
    let policy = view(ObservationTexturePolicy::FitBudget { max_rgba_bytes: 16 });
    let a = rt
        .estimate_preview_observation(c, preview, vec![0., 0.5], policy.clone(), false, vec![])
        .unwrap();
    let b = rt
        .estimate_preview_observation(c, preview, vec![0., 0.5], policy, true, vec![])
        .unwrap();
    assert_eq!(a.source, source);
    assert_eq!(a.preview_id, Some(preview.serial()));
    assert_eq!(a.sequence, Some(before.1.sequence));
    assert!(b.task_bytes > a.task_bytes);
    assert_eq!(
        rt.preview_state(c, preview).unwrap().1.sequence,
        before.1.sequence
    );
    assert_eq!(rt.preview_state(c, preview).unwrap().1.time, before.1.time);
    assert_eq!(rt.accounted_bytes(), bytes);
    let job = rt
        .request_preview_samples(c, preview, vec![0.5], false, false)
        .unwrap();
    assert!(matches!(
        rt.estimate_preview_observation(
            c,
            preview,
            vec![0.],
            view(Default::default()),
            false,
            vec![]
        ),
        Err(RuntimeError::Busy)
    ));
    rt.cancel_job(c, job).unwrap();
    rt.release_job(c, job).unwrap();
}

#[derive(Default)]
#[cfg(feature = "gpu-observation")]
struct Manual(VecDeque<Work>);
#[cfg(feature = "gpu-observation")]
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}
#[cfg(feature = "gpu-observation")]
#[test]
fn admitted_plan_executes_and_result_keeps_its_lease_after_job_release() {
    let (mut rt, c, s) = fixture(
        RuntimeLimits {
            observation_texture_rgba_bytes: 16,
            ..Default::default()
        },
        true,
    );
    let before = rt.session_info(s).unwrap();
    let baseline = rt.accounted_bytes();
    assert!(rt
        .request_observation(
            c,
            s,
            None,
            vec![Default::default()],
            view(Default::default()),
            None
        )
        .is_err());
    let request = view(ObservationTexturePolicy::FitBudget { max_rgba_bytes: 16 });
    let estimate = rt
        .estimate_observation(c, s, None, vec![Default::default(); 2], request.clone())
        .unwrap();
    let ObservationTexturePlanning::Ready { plan } = &estimate.texture_planning else {
        panic!("fit plan")
    };
    let job = rt
        .request_observation(
            c,
            s,
            None,
            vec![Default::default(); 2],
            request,
            Some(estimate.source),
        )
        .unwrap();
    let admitted = rt.accounted_bytes();
    let mut executor = Manual::default();
    while !rt.job_info(c, job).unwrap().state.is_terminal() {
        rt.dispatch(&mut executor);
        assert!(rt.complete(executor.0.pop_front().unwrap().run_quantum()));
    }
    let result = rt.job_info(c, job).unwrap().result.unwrap().unwrap();
    let JobValue::Observed(batch) = result else {
        panic!("observation")
    };
    assert_eq!(
        batch.samples.texture_report.render_rgba_bytes,
        plan.render_rgba_bytes
    );
    assert_eq!(
        batch.samples.texture_report.textures[0].render_size,
        plan.textures[0].render_size
    );
    assert!(rt.accounted_bytes() < admitted);
    rt.release_job(c, job).unwrap();
    assert!(rt.accounted_bytes() > baseline);
    assert!(rt.image_memory_usage().transfer_bytes > 0);
    drop(batch);
    assert_eq!(rt.accounted_bytes(), baseline);
    assert_eq!(rt.session_info(s).unwrap(), before);
}
