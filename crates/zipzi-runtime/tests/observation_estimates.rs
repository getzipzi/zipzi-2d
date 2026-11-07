use zipzi_contract::{
    kernel_commands::{Command, ImageInput},
    observation::{ObservationTexturePlanning, ObservationTexturePolicy},
};
use zipzi_core::Vec2;
use zipzi_observe::planning::{plan_textures, texture_sources};
use zipzi_runtime::*;

fn id(n: u32) -> String {
    format!("79700000-0000-4000-8000-{n:012}")
}

fn fixture(limits: RuntimeLimits, image: bool) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| {
            c.create_empty_asset(id(2), "Texture".into())?;
            c.create_mesh(zipzi_sdk::rectangle_mesh(
                &id(3),
                "Mesh",
                &id(2),
                Vec2::default(),
                Vec2::new(1., 1.),
            )?)
        })
        .unwrap();
    let draft = if image {
        rt.commands(
            client,
            draft,
            vec![Command::ReplaceImage {
                id: id(2),
                input: ImageInput::Rgba {
                    width: 4,
                    height: 4,
                    bytes: (0..16)
                        .flat_map(|i| [255, 20, 100, if i % 2 == 0 { 0 } else { 255 }])
                        .collect(),
                },
            }],
        )
        .unwrap()
        .0
    } else {
        draft
    };
    let version = rt.publish(client, draft, "Fixture").unwrap().version;
    (rt, client, version)
}

fn fit(bytes: u64) -> ObservationTexturePolicy {
    ObservationTexturePolicy::FitBudget {
        max_rgba_bytes: bytes,
    }
}

#[test]
fn texture_planning_on_a_published_source_is_metadata_only() {
    let (rt, client, version) = fixture(Default::default(), true);
    let before = rt.read_snapshot(client, version.session).unwrap();
    let budget = rt.resources().budget().usage();
    let sources = texture_sources(before.snapshot.document()).unwrap();
    assert!(matches!(
        plan_textures(sources.clone(), Default::default(), 16).unwrap(),
        ObservationTexturePlanning::InsufficientBudget {
            minimum_required_rgba_bytes: 64,
            ..
        }
    ));
    let ObservationTexturePlanning::Ready { plan } =
        plan_textures(sources.clone(), fit(16), 16).unwrap()
    else {
        panic!("expected fit plan")
    };
    assert_eq!((plan.source_rgba_bytes, plan.render_rgba_bytes), (64, 16));
    assert_eq!(
        (
            plan.textures[0].render_size.width,
            plan.textures[0].render_size.height
        ),
        (2, 2)
    );
    assert!(plan.textures[0].source.source_content_identity.is_none());
    for maximum in [0, 17] {
        assert_eq!(
            plan_textures(sources.clone(), fit(maximum), 16)
                .unwrap_err()
                .code,
            "INVALID_TEXTURE_POLICY"
        );
    }
    assert_eq!(rt.resources().budget().usage(), budget);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
    assert_eq!(rt.outstanding_jobs(), 0);
}

#[test]
fn missing_captured_image_rejects_texture_planning_without_reservations() {
    let (rt, client, version) = fixture(Default::default(), false);
    let source = rt.read_snapshot(client, version.session).unwrap();
    let error = texture_sources(source.snapshot.document()).unwrap_err();
    assert_eq!(error.asset_id.as_deref(), Some(id(2).as_str()));
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[test]
fn a_small_result_budget_bounds_sampling_even_after_admission() {
    use zipzi_contract::kernel_tasks::{SampleInput, SampleRequest};
    let (mut rt, client, version) = fixture(
        RuntimeLimits {
            result_bytes: 32,
            ..Default::default()
        },
        true,
    );
    let source = QuerySource::Published(version);
    let job = rt
        .request_samples(
            client,
            source,
            SampleRequest {
                input: SampleInput::Parameters {
                    values: vec![Default::default()],
                },
                queries: vec![],
                quality: true,
            },
        )
        .unwrap();
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    assert_eq!(
        rt.job(client, job)
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .code,
        "RESULT_LIMIT"
    );
    rt.release_job(client, job).unwrap();
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[cfg(feature = "gpu-observation")]
#[test]
fn observed_texture_plan_matches_render_and_result_retains_its_lease() {
    use zipzi_contract::{kernel_tasks::SampleInput, observation::ObservationView};
    let (mut rt, client, version) = fixture(Default::default(), true);
    let source = rt.read_snapshot(client, version.session).unwrap();
    let policy = fit(16);
    let textures = texture_sources(source.snapshot.document()).unwrap();
    let ObservationTexturePlanning::Ready { plan } =
        plan_textures(textures, policy.clone(), 64 << 20).unwrap()
    else {
        panic!("expected fit plan")
    };
    let before = rt.task_bytes();
    let job = rt
        .request_observation(
            client,
            source.source,
            SampleInput::Parameters {
                values: vec![Default::default(); 2],
            },
            ObservationView {
                width: 32,
                height: 32,
                roi: [0., 0., 1., 1.],
                padding_model: 0.,
                quality: true,
                topology: None,
                texture_policy: policy,
            },
        )
        .unwrap();
    let admitted = rt.task_bytes();
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    let Some(Ok(JobValue::Read(result))) = rt.job(client, job).unwrap().result.clone() else {
        panic!("observation job: {:?}", rt.job(client, job).unwrap());
    };
    let bytes = result.bytes();
    let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    let header: serde_json::Value = serde_json::from_slice(&bytes[4..4 + length]).unwrap();
    assert_eq!(
        header["texture_report"]["render_rgba_bytes"],
        plan.render_rgba_bytes
    );
    assert_eq!(
        header["texture_report"]["textures"][0]["render_size"],
        serde_json::to_value(plan.textures[0].render_size).unwrap()
    );
    assert_eq!(result.source, source.source);
    assert!(rt.task_bytes() < admitted);
    rt.release_job(client, job).unwrap();
    assert!(rt.task_bytes() > before);
    drop(result);
    assert_eq!(rt.task_bytes(), before);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}
