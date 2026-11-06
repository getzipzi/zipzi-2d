use zipzi_contract::kernel_tasks::{SampleInput, SampleRequest};
use zipzi_runtime::*;
use zipzi_sdk::{ObjectTarget as T, SpatialQuery as Q};

fn id(n: u32) -> String {
    format!("79650000-0000-4000-8000-{n:012x}")
}
fn fixture() -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::default();
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(c, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let version = rt.subscribe(c, s).unwrap().cursor;
    let draft = rt.begin_draft(c, version).unwrap();
    let (draft, _) = rt
        .batch(c, draft, |e| {
            e.create_part(zipzi_core::Part {
                id: id(4),
                runtime_id: id(4),
                name: "Part".into(),
                ..Default::default()
            })?;
            for n in [2, 3] {
                e.create_mesh(zipzi_core::Mesh {
                    id: id(n),
                    runtime_id: id(n),
                    name: format!("Mesh{n}"),
                    part_id: id(4),
                    vertex_ids: vec![10, 20, 30],
                    base_positions: vec![
                        zipzi_core::Vec2::new(0., 0.),
                        zipzi_core::Vec2::new(1., 0.),
                        zipzi_core::Vec2::new(0., 1.),
                    ],
                    uvs: vec![
                        zipzi_core::Vec2::new(0., 0.),
                        zipzi_core::Vec2::new(1., 0.),
                        zipzi_core::Vec2::new(0., 1.),
                    ],
                    triangles: vec![[10, 20, 30]],
                    ..Default::default()
                })?;
            }
            Ok(())
        })
        .unwrap();
    let publication = rt.publish(c, draft, "Geometry").unwrap();
    (rt, c, publication.version)
}
fn queries() -> Vec<Q> {
    vec![
        Q::Bounds {
            targets: vec![T::Part(id(4))],
            include_hidden: false,
        },
        Q::Hits {
            point: [0.25, 0.25],
            include_hidden: false,
            details: true,
            max_candidates: 1,
        },
    ]
}
fn finish(rt: &mut Runtime, c: ClientId, job: JobId) -> Result<ReadResult> {
    while !rt.job(c, job).unwrap().state.is_terminal() {
        let work = rt.take_work(job).unwrap();
        rt.complete(work.run()).unwrap();
    }
    let result = rt.job(c, job).unwrap().result.clone().unwrap();
    rt.release_job(c, job).unwrap();
    match result? {
        JobValue::Read(value) => Ok(value),
        _ => panic!("expected spatial sample result"),
    }
}

fn request(input: SampleInput, queries: Vec<Q>) -> SampleRequest {
    SampleRequest {
        input,
        queries,
        quality: false,
    }
}
fn parameters(count: usize) -> SampleInput {
    SampleInput::Parameters {
        values: vec![Default::default(); count],
    }
}

#[test]
fn frozen_geometry_bounds_triangle_identity_and_metadata_do_not_follow_newer_edits() {
    let (mut rt, c, before) = fixture();
    let source = QuerySource::Published(before);
    let job = rt
        .request_samples(c, source, request(parameters(2), queries()))
        .unwrap();
    let draft = rt.begin_draft(c, before).unwrap();
    let (draft, _) = rt
        .batch(c, draft, |e| {
            e.rename(zipzi_sdk::NameTarget::Mesh(&id(3)), "New".into())
        })
        .unwrap();
    let receipt = rt.publish(c, draft, "New name").unwrap();
    let result = finish(&mut rt, c, job).unwrap();
    assert_eq!(result.source, source);
    let batch: serde_json::Value = serde_json::from_slice(result.bytes()).unwrap();
    let frames = batch["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 2);
    for frame in frames {
        let results = frame["spatial"].as_array().unwrap();
        assert_eq!(results[0]["kind"], "bounds");
        let bounds = &results[0]["value"];
        assert_eq!(bounds["model_bounds"], serde_json::json!([0., 0., 1., 1.]));
        assert_eq!(bounds["mesh_ids"], serde_json::json!([id(2), id(3)]));
        assert_eq!(results[1]["kind"], "hits");
        let hits = &results[1]["value"];
        assert_eq!(hits["total"], 2);
        assert_eq!(hits["truncated"], true);
        let hit = &hits["hits"][0];
        assert_eq!(hit["name"], "Mesh3");
        assert_eq!(hit["part_path"], serde_json::json!([id(4)]));
        assert_eq!(
            hit["triangles"][0]["vertex_ids"],
            serde_json::json!([10, 20, 30])
        );
        assert_eq!(
            hit["triangles"][0]["barycentric"],
            serde_json::json!([0.5, 0.25, 0.25])
        );
        assert_eq!(hit["triangles"][0]["uv"], serde_json::json!([0.25, 0.25]));
    }
    assert_eq!(receipt.history.undo_steps, 2);
}
#[test]
fn invalid_spatial_query_preserves_draft_and_a_valid_animation_sample_can_follow() {
    let (mut rt, c, before) = fixture();
    let draft = rt.begin_draft(c, before).unwrap();
    let source = QuerySource::Draft(draft);
    let input = SampleInput::Animation {
        times: vec![0.5],
        motions: vec![],
        expressions: vec![],
        inputs: vec![],
        physics: false,
        stabilized_start: false,
    };
    let invalid = vec![Q::Bounds {
        targets: vec![T::Mesh(id(99))],
        include_hidden: true,
    }];
    let job = rt
        .request_samples(c, source, request(input.clone(), invalid))
        .unwrap();
    assert_eq!(finish(&mut rt, c, job).unwrap_err().code, "MISSING_MESH");
    assert_eq!(rt.read_draft(c, draft).unwrap().source, source);
    let job = rt
        .request_samples(c, source, request(input, queries()))
        .unwrap();
    let result = finish(&mut rt, c, job).unwrap();
    assert_eq!(result.source, source);
    let value: serde_json::Value = serde_json::from_slice(result.bytes()).unwrap();
    assert_eq!(value["frames"][0]["animation"]["time"], 0.5);
    assert_eq!(value["frames"][0]["spatial"].as_array().unwrap().len(), 2);
    let after = rt.subscribe(c, before.session).unwrap();
    assert_eq!(after.cursor, before);
    assert_eq!(after.history.undo_steps, 1);
}

#[test]
fn selection_admission_and_cancel_release_frozen_work_without_side_effects() {
    let (mut rt, c, before) = fixture();
    let source = QuerySource::Published(before);
    let baseline = rt.task_bytes();
    assert_eq!(
        rt.request_samples(
            c,
            source,
            request(parameters(1), vec![queries()[1].clone(); 65])
        )
        .unwrap_err()
        .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        rt.request_samples(c, source, request(parameters(0), queries()))
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(rt.task_bytes(), baseline);
    for _ in 0..100 {
        let job = rt
            .request_samples(c, source, request(parameters(1), queries()))
            .unwrap();
        rt.cancel_job(c, job).unwrap();
        assert_eq!(finish(&mut rt, c, job).unwrap_err().code, "CANCELLED");
        assert_eq!(rt.task_bytes(), baseline);
    }
    assert_eq!(rt.read_snapshot(c, before.session).unwrap().source, source);
}
