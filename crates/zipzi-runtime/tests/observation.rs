#![cfg(feature = "gpu-observation")]

use serde_json::Value;
use zipzi_contract::{
    kernel_commands::{Command, ImageInput},
    kernel_tasks::SampleInput,
    observation::ObservationView,
};
use zipzi_core::Vec2;
use zipzi_runtime::*;

fn id(n: u32) -> String {
    format!("79660000-0000-4000-8000-{n:012x}")
}

fn view() -> ObservationView {
    ObservationView {
        texture_policy: Default::default(),
        width: 16,
        height: 16,
        roi: [0., 0., 1., 1.],
        padding_model: 0.,
        topology: None,
        quality: false,
    }
}

fn replace(rgba: [u8; 4]) -> Command {
    Command::ReplaceImage {
        id: id(2),
        input: ImageInput::Rgba {
            width: 1,
            height: 1,
            bytes: rgba.to_vec(),
        },
    }
}

fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![
                Command::CreateAsset {
                    id: id(2),
                    name: "Red".into(),
                },
                replace([255, 0, 0, 255]),
            ],
        )
        .unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| {
            c.create_mesh(zipzi_sdk::rectangle_mesh(
                &id(3),
                "Quad",
                &id(2),
                Vec2::default(),
                Vec2::new(1., 1.),
            )?)
        })
        .unwrap();
    let version = rt.publish(client, draft, "Fixture").unwrap().version;
    (rt, client, version)
}

fn params(count: usize) -> SampleInput {
    SampleInput::Parameters {
        values: vec![Default::default(); count],
    }
}

fn finish(rt: &mut Runtime, client: ClientId, job: JobId) -> ReadResult {
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    let result = rt
        .job(client, job)
        .unwrap()
        .result
        .clone()
        .unwrap()
        .unwrap();
    rt.release_job(client, job).unwrap();
    let JobValue::Read(result) = result else {
        panic!("expected observation")
    };
    result
}

fn frames(result: &ReadResult) -> Vec<(Value, [u8; 4])> {
    let mut rest = result.bytes();
    let mut frames = Vec::new();
    while !rest.is_empty() {
        let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        let header: Value = serde_json::from_slice(&rest[4..4 + length]).unwrap();
        rest = &rest[4 + length..];
        let png_bytes = header["png_bytes"].as_u64().unwrap() as usize;
        let png = &rest[..png_bytes];
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        let decoded = zipzi_core::image::decode_png(png).unwrap();
        let index = (8 * 16 + 8) * 4;
        assert_eq!((decoded.width, decoded.height), (16, 16));
        frames.push((header, decoded.rgba[index..index + 4].try_into().unwrap()));
        rest = &rest[png_bytes..];
    }
    frames
}

#[test]
#[ignore = "requires GPU; run --test observation -- --include-ignored"]
fn frozen_formal_and_candidate_images_never_mix_texture_replacements() {
    let (mut rt, client, version) = fixture(Default::default());
    let source = QuerySource::Published(version);
    let job = rt
        .request_observation(client, source, params(2), view())
        .unwrap();
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![replace([0, 255, 0, 255])])
        .unwrap();
    let green = rt.publish(client, draft, "Green").unwrap();
    let result = finish(&mut rt, client, job);
    assert_eq!(result.source, source);
    let formal = frames(&result);
    assert_eq!(formal.len(), 2);
    assert!(formal.iter().all(|(header, color)| {
        header["source"] == serde_json::to_value(source).unwrap() && *color == [255, 0, 0, 255]
    }));
    let header = &formal[0].0;
    assert_eq!(header["view"]["scale"], 16.);
    assert_eq!(header["view"]["offset"], serde_json::json!([0., 16.]));
    let textures = header["texture_report"]["textures"].as_array().unwrap();
    assert_eq!(textures.len(), 1);
    let draft = rt.begin_draft(client, green.version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![replace([0, 0, 255, 255])])
        .unwrap();
    let source = QuerySource::Draft(draft);
    let job = rt
        .request_observation(client, source, params(1), view())
        .unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![replace([255, 255, 0, 255])])
        .unwrap();
    rt.cancel_draft(client, draft.draft).unwrap();
    let result = finish(&mut rt, client, job);
    assert_eq!(result.source, source);
    let candidate = frames(&result);
    assert_eq!(candidate[0].1, [0, 0, 255, 255]);
    assert_ne!(
        textures[0]["render_rgba_hash"],
        candidate[0].0["texture_report"]["textures"][0]["render_rgba_hash"]
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
#[ignore = "requires GPU; run --test observation -- --include-ignored"]
fn animation_images_use_the_captured_source_without_changing_history() {
    let (mut rt, client, version) = fixture(Default::default());
    let source = QuerySource::Published(version);
    let input = SampleInput::Animation {
        times: vec![0., 0.25, 0.],
        motions: vec![],
        expressions: vec![],
        inputs: vec![],
        physics: false,
        stabilized_start: false,
    };
    let job = rt
        .request_observation(client, source, input, view())
        .unwrap();
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![replace([0, 255, 0, 255])])
        .unwrap();
    let green = rt.publish(client, draft, "Green").unwrap();
    let result = finish(&mut rt, client, job);
    assert_eq!(result.source, source);
    let frames = frames(&result);
    assert_eq!(frames.len(), 3);
    assert!(frames.iter().all(|(_, color)| *color == [255, 0, 0, 255]));
    let after = rt.subscribe(client, version.session).unwrap();
    assert_eq!(after.cursor, green.version);
    assert_eq!(after.history.undo_steps, 2);
}

#[test]
fn invalid_and_cancelled_observations_keep_the_source_and_release_budget() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        task_bytes: 384 << 20,
        ..Default::default()
    });
    let source = QuerySource::Published(version);
    let before = rt.task_bytes();
    assert!(rt
        .request_observation(client, source, params(65), view())
        .is_err());
    assert!(rt
        .request_observation(
            client,
            source,
            params(1),
            ObservationView {
                roi: [0.; 4],
                ..view()
            }
        )
        .is_err());
    assert_eq!(
        rt.request_observation(
            client,
            source,
            params(64),
            ObservationView {
                width: 4096,
                height: 4096,
                ..view()
            }
        )
        .unwrap_err()
        .code,
        "BUDGET_EXCEEDED"
    );
    for _ in 0..100 {
        let job = rt
            .request_observation(client, source, params(1), view())
            .unwrap();
        assert!(rt.task_bytes() > before);
        rt.cancel_job(client, job).unwrap();
        let work = rt.take_work(job).unwrap();
        rt.complete(work.run()).unwrap();
        assert_eq!(rt.job(client, job).unwrap().state, JobState::Cancelled);
        rt.release_job(client, job).unwrap();
    }
    assert_eq!(rt.task_bytes(), before);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}
