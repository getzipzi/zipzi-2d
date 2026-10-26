use std::collections::VecDeque;
use zipzi_runtime::*;
use zipzi_sdk::{ObjectTarget as T, SpatialQuery as Q, SpatialResult as R};

#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}
fn id(n: u32) -> String {
    format!("79650000-0000-4000-8000-{n:012x}")
}
fn fixture() -> (Runtime, ClientId, SessionId) {
    let mut rt = Runtime::default();
    let c = rt.register_client().unwrap();
    let s = rt
        .create_session(c, &id(1), zipzi_core::ModelSetup::default())
        .unwrap();
    let edit = rt.begin_edit(c, s, "geometry", None).unwrap();
    rt.apply_edit(c, edit, |e| {
        e.create_part(zipzi_core::Part {
            id: id(4),
            name: "Part".into(),
            ..Default::default()
        })?;
        for n in [2, 3] {
            e.create_mesh(zipzi_core::Mesh {
                id: id(n),
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
    rt.commit_edit(c, edit).unwrap();
    (rt, c, s)
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
fn finish(rt: &mut Runtime, c: ClientId, j: JobId) -> std::sync::Arc<spatial::SpatialSampleBatch> {
    let mut executor = Manual::default();
    while !rt.job_info(c, j).unwrap().state.is_terminal() {
        rt.dispatch(&mut executor);
        assert!(rt.complete(executor.0.pop_front().unwrap().run_quantum()));
    }
    let JobValue::Spatial(data) = rt.job_info(c, j).unwrap().result.unwrap().unwrap() else {
        panic!("spatial batch")
    };
    rt.release_job(c, j).unwrap();
    data
}
#[test]
fn frozen_geometry_bounds_triangle_identity_and_metadata_do_not_follow_newer_edits() {
    let (mut rt, c, s) = fixture();
    let before = rt.session_info(s).unwrap();
    let j = rt
        .request_spatial_samples(
            c,
            s,
            None,
            vec![Default::default(), Default::default()],
            queries(),
            None,
        )
        .unwrap();
    let edit = rt.begin_edit(c, s, "new name", None).unwrap();
    rt.apply_modeling(
        c,
        edit,
        vec![modeling::ModelingCommand::RenameMesh {
            id: id(3),
            name: "New".into(),
        }],
    )
    .unwrap();
    rt.commit_edit(c, edit).unwrap();
    let batch = finish(&mut rt, c, j);
    assert_eq!(batch.source.version(), before.version);
    assert_eq!(batch.samples.frames.len(), 2);
    for frame in &batch.samples.frames {
        let R::Bounds(bounds) = &frame.results[0] else {
            panic!("bounds")
        };
        assert_eq!(bounds.model_bounds, Some([0., 0., 1., 1.]));
        assert_eq!(bounds.mesh_ids, vec![id(2), id(3)]);
        let R::Hits(hits) = &frame.results[1] else {
            panic!("hits")
        };
        assert_eq!(hits.total, 2);
        assert!(hits.truncated);
        assert_eq!(hits.hits[0].name, "Mesh3");
        assert_eq!(hits.hits[0].part_path, vec![id(4)]);
        assert_eq!(hits.hits[0].triangles[0].vertex_ids, Some([10, 20, 30]));
        assert_eq!(hits.hits[0].triangles[0].barycentric, [0.5, 0.25, 0.25]);
        assert_eq!(hits.hits[0].triangles[0].uv, Some([0.25, 0.25]));
    }
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (2, 0));
}
#[test]
fn invalid_spatial_target_never_aborts_candidate_and_preview_rejects_it_before_seek() {
    let (mut rt, c, s) = fixture();
    let edit = rt.begin_edit(c, s, "candidate", None).unwrap();
    let q = vec![Q::Bounds {
        targets: vec![T::Mesh(id(99))],
        include_hidden: true,
    }];
    let j = rt
        .request_spatial_samples(c, s, Some(edit), vec![Default::default()], q.clone(), None)
        .unwrap();
    let mut executor = Manual::default();
    rt.dispatch(&mut executor);
    rt.complete(executor.0.pop_front().unwrap().run());
    assert!(
        matches!(rt.job_info(c,j).unwrap().result,Some(Err(RuntimeError::Sdk(e))) if e.code.as_ref()=="MISSING_MESH")
    );
    rt.release_job(c, j).unwrap();
    assert_eq!(rt.edit_state(c, edit).unwrap(), EditState::Editing);
    let (p, source) = rt.create_preview(c, s, Some(edit), None).unwrap();
    let j = rt
        .request_preview_samples_with_queries(c, p, vec![0.5], false, false, q)
        .unwrap();
    rt.dispatch(&mut executor);
    rt.complete(executor.0.pop_front().unwrap().run());
    assert_eq!(rt.job_info(c, j).unwrap().state, JobState::Failed);
    assert_eq!(rt.preview_state(c, p).unwrap().1.time, 0.);
    rt.release_job(c, j).unwrap();
    let j = rt
        .request_preview_samples_with_queries(c, p, vec![0.5], false, false, queries())
        .unwrap();
    rt.dispatch(&mut executor);
    rt.complete(executor.0.pop_front().unwrap().run());
    let JobValue::Previewed(result) = rt.job_info(c, j).unwrap().result.unwrap().unwrap() else {
        panic!("preview")
    };
    assert_eq!(result.source, source);
    assert!(result.samples.frames[0].geometry.is_none());
    assert_eq!(result.samples.frames[0].spatial.len(), 2);
    assert_eq!(rt.session_info(s).unwrap().history_lengths, (1, 0));
}
#[test]
fn selection_admission_and_cancel_release_frozen_work_without_side_effects() {
    let (mut rt, c, s) = fixture();
    let before = rt.session_info(s).unwrap();
    let bytes = rt.accounted_bytes();
    assert!(rt
        .request_spatial_samples(c, s, None, vec![], vec![queries()[1].clone(); 65], None)
        .is_err());
    for _ in 0..100 {
        let j = rt
            .request_spatial_samples(c, s, None, vec![Default::default()], queries(), None)
            .unwrap();
        rt.cancel_job(c, j).unwrap();
        assert_eq!(rt.job_info(c, j).unwrap().state, JobState::Cancelled);
        rt.release_job(c, j).unwrap();
    }
    assert_eq!(rt.accounted_bytes(), bytes);
    assert_eq!(rt.session_info(s).unwrap(), before);
    let j = rt
        .request_spatial_samples(c, s, None, vec![], queries(), None)
        .unwrap();
    assert!(finish(&mut rt, c, j).samples.frames.is_empty());
}
