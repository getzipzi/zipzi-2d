use zipzi_core::{ModelSetup, Parameter, PreviewValues, Vec2};
use zipzi_sdk::{
    AuthoringSession, MeshBindingAxisSpec, MeshBindingCreateSpec, MeshBindingOperation,
    MeshCreateSpec, MeshGeometry, MeshKeyformSpec,
};
fn id(n: u32) -> String {
    format!("79700000-0000-4000-8000-{n:012x}")
}
fn keyform(value: f32, shift: f32) -> MeshKeyformSpec {
    MeshKeyformSpec {
        keys: vec![value],
        positions: vec![
            Vec2::new(shift, 0.),
            Vec2::new(shift + 1., 0.),
            Vec2::new(shift, 1.),
        ],
        opacity: None,
        multiply: None,
        screen: None,
        draw_order: None,
    }
}
fn spec() -> MeshBindingCreateSpec {
    MeshBindingCreateSpec {
        mesh_id: id(2),
        axes: vec![MeshBindingAxisSpec {
            parameter_id: id(3),
            keys: vec![0., 1.],
        }],
        keyforms: vec![keyform(0., 0.), keyform(1., 10.)],
        id: None,
    }
}
fn setup() -> (AuthoringSession, zipzi_sdk::EditSession<'static>) {
    let sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    let mut e = sdk.begin_owned_edit("bound mesh", None).unwrap();
    e.create_parameter(Parameter {
        id: id(3),
        minimum: 0.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    })
    .unwrap();
    e.create_mesh_spec(MeshCreateSpec {
        id: Some(id(2)),
        name: "Face".into(),
        texture_asset_id: String::new(),
        geometry: MeshGeometry {
            vertex_ids: vec![42, 7, 99],
            positions: keyform(0., 0.).positions,
            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            triangles: vec![[42, 7, 99]],
            meshing: None,
        },
    })
    .unwrap();
    (sdk, e)
}
#[test]
fn numeric_specs_allocate_canonical_identities_and_interpolate_without_preview() {
    let (mut sdk, mut e) = setup();
    let input = spec();
    let encoded = serde_json::to_value(&input).unwrap();
    assert_eq!(encoded["axes"][0]["keys"], serde_json::json!([0., 1.]));
    let id = e.create_mesh_binding_spec(input).unwrap();
    uuid::Uuid::parse_str(&id).unwrap();
    let binding = e.candidate_document().get_binding(&id).unwrap().clone();
    assert_eq!(binding.keyforms[0].appearance, Default::default());
    assert_eq!(
        binding.keyforms[0].samples,
        [binding.axes[0].samples[0].id.clone()]
    );
    MeshBindingOperation::SetKeyform {
        id: id.clone(),
        keyform: keyform(1., 20.),
    }
    .apply(&mut e)
    .unwrap();
    assert_eq!(
        e.candidate_document().get_binding(&id).unwrap().axes,
        binding.axes
    );
    e.commit_to(&mut sdk).unwrap();
    let before = sdk.version();
    let frame = sdk
        .read_snapshot()
        .sample_modeling(
            vec![PreviewValues::from([(self::id(3), 0.5)])],
            Default::default(),
        )
        .unwrap();
    assert_eq!(frame.frames[0].drawables[0].positions[0].x, 10.);
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.history_lengths(), (1, 0));
}
#[test]
fn caught_invalid_grid_or_keyform_failure_cannot_publish_partial_creation() {
    for invalid_grid in [false, true] {
        let (mut sdk, mut e) = setup();
        let mut input = spec();
        if invalid_grid {
            input.axes[0].keys = vec![0., 0.];
        }
        let created = e.create_mesh_binding_spec(input);
        if invalid_grid {
            assert!(created.is_err());
        } else {
            let mut bad = keyform(1., 10.);
            bad.positions.pop();
            assert!(MeshBindingOperation::SetKeyform {
                id: created.unwrap(),
                keyform: bad
            }
            .apply(&mut e)
            .is_err());
        }
        assert!(e.commit_to(&mut sdk).is_err());
        assert!(sdk.document().mesh_order().is_empty());
        assert_eq!(sdk.history_lengths(), (0, 0));
    }
}

#[test]
fn resumable_sampling_keeps_frozen_numeric_results_and_batch_order() {
    use zipzi_sdk::{ModelingSampler, ObjectTarget, SpatialQuery, SpatialResult, SpatialSampler};
    let (mut sdk, mut edit) = setup();
    edit.create_mesh_binding_spec(spec()).unwrap();
    edit.commit_to(&mut sdk).unwrap();
    let snapshot = std::sync::Arc::new(sdk.read_snapshot());
    let poses: Vec<_> = [0.5, 0., 0.5]
        .into_iter()
        .map(|v| PreviewValues::from([(id(3), v)]))
        .collect();
    let queries = vec![SpatialQuery::Bounds {
        targets: vec![ObjectTarget::Mesh(id(2))],
        include_hidden: false,
    }];
    let mut modeling = ModelingSampler::new(snapshot.clone(), poses.clone(), Default::default());
    let mut spatial = SpatialSampler::new(snapshot.clone(), poses.clone(), queries.clone());
    let control = Default::default();
    for _ in 0..2 {
        assert!(modeling.step(&control).unwrap().is_none());
        assert!(spatial.step(&control).unwrap().is_none());
    }
    // A changed formal source cannot enter the remaining pose of either sampler.
    sdk.edit("replace canvas", None, |e| {
        e.replace_model_setup(zipzi_core::Canvas::new(50., 50., Vec2::default(), 1.))
    })
    .unwrap();
    let result = modeling.step(&control).unwrap().unwrap();
    assert_eq!(result.parameters, poses);
    assert_eq!(result.version, snapshot.version());
    assert_eq!(
        result
            .frames
            .iter()
            .map(|f| f.drawables[0].positions[0].x)
            .collect::<Vec<_>>(),
        vec![5., 0., 5.]
    );
    let bounds = spatial.step(&control).unwrap().unwrap();
    assert_eq!(bounds.requested, poses);
    for (frame, x) in bounds.frames.iter().zip([5., 0., 5.]) {
        let SpatialResult::Bounds(b) = &frame.results[0] else {
            panic!()
        };
        assert_eq!(b.model_bounds, Some([x, 0., x + 1., 1.]));
    }
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::to_value(
            snapshot
                .sample_modeling(poses.clone(), Default::default())
                .unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        serde_json::to_value(&bounds).unwrap(),
        serde_json::to_value(
            snapshot
                .sample_spatial_with_control(poses, queries, &control)
                .unwrap()
        )
        .unwrap()
    );
    assert!(modeling.step(&control).is_err());
    assert!(spatial.step(&control).is_err());
}

#[test]
fn resumable_sampling_checks_empty_queries_and_cancellation_between_steps() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use zipzi_sdk::{
        ConversionControl, ModelingSampler, ObjectTarget, SpatialQuery, SpatialSampler,
    };
    let (mut sdk, edit) = setup();
    edit.commit_to(&mut sdk).unwrap();
    let snapshot = sdk.read_snapshot();
    let mut empty = ModelingSampler::new(&snapshot, vec![], Default::default());
    assert!(empty
        .step(&Default::default())
        .unwrap()
        .unwrap()
        .frames
        .is_empty());
    let mut invalid = SpatialSampler::new(
        &snapshot,
        vec![],
        vec![SpatialQuery::Bounds {
            targets: vec![ObjectTarget::Mesh(id(999))],
            include_hidden: false,
        }],
    );
    assert_eq!(
        invalid.step(&Default::default()).unwrap_err().code.as_ref(),
        "MISSING_MESH"
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let control = ConversionControl::from_cancel_flag(cancel.clone());
    let mut modeling =
        ModelingSampler::new(&snapshot, vec![Default::default(); 2], Default::default());
    let mut spatial = SpatialSampler::new(&snapshot, vec![Default::default(); 2], vec![]);
    assert!(modeling.step(&control).unwrap().is_none());
    assert!(spatial.step(&control).unwrap().is_none());
    cancel.store(true, Ordering::Release);
    assert_eq!(
        modeling.step(&control).unwrap_err().code.as_ref(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(
        spatial.step(&control).unwrap_err().code.as_ref(),
        "CANCELLED"
    );
}
