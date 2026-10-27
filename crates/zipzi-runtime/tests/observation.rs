use std::collections::VecDeque;
use zipzi_runtime::{authoring::*, observation::*, *};
#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, w: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(w);
        Ok(())
    }
}
fn id(n: u32) -> String {
    format!("79660000-0000-4000-8000-{n:012x}")
}
fn view() -> ObservationView {
    ObservationView {
        width: 16,
        height: 16,
        roi: [0., 0., 1., 1.],
        padding_model: 0.,
    }
}
fn fixture() -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::default();
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(c, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let image = rt
        .prepare_image(c, 1, 1, vec![255, 0, 0, 255])
        .unwrap()
        .image;
    let e = rt.begin_edit(c, s, "red", None).unwrap();
    rt.apply_authoring(
        c,
        e,
        vec![EditOperation::Resource(ResourceOperation::CreateImage {
            spec: zipzi_sdk::ImageCreateSpec {
                id: Some(id(2)),
                name: "Red".into(),
            },
            image,
        })],
    )
    .unwrap();
    rt.apply_edit(c, e, |edit| {
        edit.create_mesh(zipzi_core::Mesh {
            id: id(3),
            name: "Quad".into(),
            texture_asset_id: id(2).into(),
            vertex_ids: vec![10, 20, 30, 40],
            base_positions: vec![
                zipzi_core::Vec2::new(0., 0.),
                zipzi_core::Vec2::new(1., 0.),
                zipzi_core::Vec2::new(1., 1.),
                zipzi_core::Vec2::new(0., 1.),
            ],
            uvs: vec![
                zipzi_core::Vec2::new(0., 0.),
                zipzi_core::Vec2::new(1., 0.),
                zipzi_core::Vec2::new(1., 1.),
                zipzi_core::Vec2::new(0., 1.),
            ],
            triangles: vec![[10, 20, 30], [10, 30, 40]],
            ..Default::default()
        })
    })
    .unwrap();
    rt.commit_edit(c, e).unwrap();
    rt.release_prepared_image(c, image).unwrap();
    (rt, c, s)
}
fn replace(rt: &mut Runtime, c: ClientId, e: EditId, rgba: Vec<u8>) {
    let image = rt.prepare_image(c, 1, 1, rgba).unwrap().image;
    rt.apply_authoring(
        c,
        e,
        vec![EditOperation::Resource(ResourceOperation::ReplaceImage {
            id: id(2),
            image,
        })],
    )
    .unwrap();
    rt.release_prepared_image(c, image).unwrap();
}
fn finish(rt: &mut Runtime, c: ClientId, j: JobId) -> JobValue {
    let mut exec = Manual::default();
    while !rt.job_info(c, j).unwrap().state.is_terminal() {
        rt.dispatch(&mut exec);
        assert!(rt.complete(exec.0.pop_front().unwrap().run_quantum()));
    }
    let value = rt.job_info(c, j).unwrap().result.unwrap().unwrap();
    rt.release_job(c, j).unwrap();
    value
}
fn pixel(image: &ObservationImage) -> &[u8] {
    let index = (8 * 16 + 8) * 4;
    &image.rgba[index..index + 4]
}
#[test]
#[ignore = "requires GPU; run --test observation -- --include-ignored"]
fn frozen_formal_and_candidate_images_never_mix_texture_replacements() {
    let (mut rt, c, s) = fixture();
    assert_eq!(rt.runtime_capabilities().gpu, None);
    let before = rt.session_info(s).unwrap();
    let j = rt
        .request_observation(c, s, None, vec![Default::default(); 2], view(), None)
        .unwrap();
    // Capture the first pose, then publish a new texture while this same batch
    // is yielded. The remaining acquisition and rendering must stay frozen.
    let mut exec = Manual::default();
    rt.dispatch(&mut exec);
    let completion = exec.0.pop_front().unwrap().run_quantum();
    assert!(!completion.is_terminal());
    assert!(rt.complete(completion));
    let e = rt.begin_edit(c, s, "green", None).unwrap();
    replace(&mut rt, c, e, vec![0, 255, 0, 255]);
    rt.commit_edit(c, e).unwrap();
    let JobValue::Observed(batch) = finish(&mut rt, c, j) else {
        panic!("images")
    };
    assert_eq!(batch.source.version(), before.version);
    assert_eq!(rt.runtime_capabilities().gpu, Some(true));
    let a = &batch.samples.frames[0];
    assert_eq!(pixel(a), [255, 0, 0, 255]);
    assert_eq!(a.png[..8], [137, 80, 78, 71, 13, 10, 26, 10]);
    assert_eq!(a.capture_id, batch.samples.frames[1].capture_id);
    assert_eq!(
        a.textures[0].sha256,
        batch.samples.frames[1].textures[0].sha256
    );
    assert_eq!(a.scale, 16.);
    assert_eq!(a.offset, [0., 16.]);
    let e = rt.begin_edit(c, s, "blue candidate", None).unwrap();
    replace(&mut rt, c, e, vec![0, 0, 255, 255]);
    let j = rt
        .request_observation(c, s, Some(e), vec![Default::default()], view(), None)
        .unwrap();
    rt.dispatch(&mut exec);
    assert!(rt.complete(exec.0.pop_front().unwrap().run_quantum()));
    replace(&mut rt, c, e, vec![255, 255, 0, 255]);
    rt.release_edit(c, e).unwrap();
    let JobValue::Observed(candidate) = finish(&mut rt, c, j) else {
        panic!("candidate image")
    };
    assert_eq!(pixel(&candidate.samples.frames[0]), [0, 0, 255, 255]);
    assert!(matches!(
        candidate.source,
        SourceRef::Edit { sequence: 1, .. }
    ));
    assert_ne!(
        a.textures[0].sha256,
        candidate.samples.frames[0].textures[0].sha256
    );
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (2, 0));
}
#[test]
#[ignore = "requires GPU; run --test observation -- --include-ignored"]
fn preview_images_use_the_captured_source_without_changing_history() {
    let (mut rt, c, s) = fixture();
    let before = rt.session_info(s).unwrap();
    let (p, source) = rt.create_preview(c, s, None, None).unwrap();
    let e = rt.begin_edit(c, s, "green", None).unwrap();
    replace(&mut rt, c, e, vec![0, 255, 0, 255]);
    rt.commit_edit(c, e).unwrap();
    let j = rt
        .request_preview_frames(c, p, vec![0., 0.25, 0.], false, false, vec![], Some(view()))
        .unwrap();
    let JobValue::Previewed(batch) = finish(&mut rt, c, j) else {
        panic!("preview image")
    };
    assert_eq!(batch.source, source);
    assert_eq!(source.version(), before.version);
    for frame in &batch.samples.frames {
        assert!(frame.geometry.is_none());
        assert_eq!(pixel(frame.observation.as_ref().unwrap()), [255, 0, 0, 255]);
    }
    assert_eq!(rt.preview_state(c, p).unwrap().1.time, 0.);
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (2, 0));
}
#[test]
fn invalid_views_and_cancelled_queued_images_preserve_sources_and_release_budget() {
    let (mut rt, c, s) = fixture();
    let before = rt.session_info(s).unwrap();
    let bytes = rt.accounted_bytes();
    assert!(rt
        .request_observation(c, s, None, vec![Default::default(); 65], view(), None)
        .is_err());
    assert!(rt
        .request_observation(
            c,
            s,
            None,
            vec![Default::default()],
            ObservationView {
                roi: [0.; 4],
                ..view()
            },
            None
        )
        .is_err());
    assert!(rt
        .request_observation(
            c,
            s,
            None,
            vec![Default::default(); 64],
            ObservationView {
                width: 4096,
                height: 4096,
                ..view()
            },
            None
        )
        .is_err());
    for _ in 0..100 {
        let j = rt
            .request_observation(c, s, None, vec![Default::default()], view(), None)
            .unwrap();
        rt.cancel_job(c, j).unwrap();
        rt.release_job(c, j).unwrap();
    }
    assert_eq!(rt.accounted_bytes(), bytes);
    assert_eq!(rt.session_info(s).unwrap(), before);
    assert_eq!(rt.runtime_capabilities().gpu, None);
}
