use std::sync::Arc;
use zipzi_core::{preview::PreviewState, state::*, *};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn document() -> DocumentState {
    let mut doc = DocumentState::new(
        id(1),
        Canvas::new(640., 480., Vec2::new(320., 240.), 100.).into(),
    )
    .unwrap()
    .builder();
    doc.create_parameter(Parameter {
        id: id(2),
        runtime_id: "P".into(),
        minimum: -1.,
        maximum: 1.,
        ..Default::default()
    })
    .unwrap();
    doc.freeze().unwrap().state
}

#[test]
fn consumers_share_one_evaluation_and_snapshots_remain_immutable() {
    let doc = document();
    let mut state = PreviewState::default();
    let old = state.frame(&doc, 1).unwrap();
    for _ in 0..5 {
        assert!(Arc::ptr_eq(&old, &state.frame(&doc, 1).unwrap()));
    }
    assert_eq!(state.evaluation_count(), 1);
    assert!(state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap());
    let new = state.frame(&doc, 1).unwrap();
    assert_eq!(state.evaluation_count(), 2);
    assert_eq!(old.parameters[0].value, 0.);
    assert_eq!(new.parameters[0].value, 0.5);
    // Frame metadata alone cannot identify a published animation frame.
    assert_eq!(old.source_revision, new.source_revision);
    assert!(!Arc::ptr_eq(&old, &new));
    assert!(!state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap());
    assert_eq!(state.evaluation_count(), 2);
    // Distinct requests remain observable even when their clamped geometry matches.
    state.replace(&doc, 1, [(id(2), 10.)].into()).unwrap();
    let rev = state.revision();
    state.replace(&doc, 1, [(id(2), 20.)].into()).unwrap();
    assert_eq!(state.revision(), rev + 1);
    assert_eq!(state.frame(&doc, 1).unwrap().parameters[0].requested, 20.);
}

#[test]
fn rejected_requests_preserve_committed_state() {
    let doc = document();
    let mut state = PreviewState::default();
    state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap();
    let frame = state.frame(&doc, 1).unwrap();
    let revision = state.revision();
    for values in [[(id(2), f32::NAN)].into(), [(id(99), 1.)].into()] {
        assert!(state.replace(&doc, 1, values).is_err());
        assert_eq!(state.revision(), revision);
        assert_eq!(state.values()[&id(2)], 0.5);
        assert!(Arc::ptr_eq(&frame, &state.frame(&doc, 1).unwrap()));
    }
}

#[test]
fn document_changes_generation_restore_and_parameter_removal_invalidate() {
    let mut doc = document();
    let saved = doc.clone();
    let mut state = PreviewState::default();
    state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap();
    let first = state.frame(&doc, 1).unwrap();
    let mut p = doc.parameter(&id(2)).unwrap().clone();
    p.maximum = 0.25;
    let mut builder = doc.builder();
    builder.replace_parameter(p).unwrap();
    doc = builder.freeze().unwrap().state;
    assert_eq!(state.frame(&doc, 1).unwrap().parameters[0].value, 0.25);
    doc = saved.clone();
    assert_eq!(state.frame(&doc, 1).unwrap().parameters[0].value, 0.5);
    // A replacement generation must invalidate even for identical content.
    assert!(!Arc::ptr_eq(&first, &state.frame(&saved, 2).unwrap()));
    let mut builder = doc.builder();
    builder.remove_parameter(&id(2)).unwrap();
    doc = builder.freeze().unwrap().state;
    state.retain_parameters(&doc);
    assert!(state.frame(&doc, 2).unwrap().parameters.is_empty());
    state.reset();
    assert!(state.values().is_empty());
}

#[test]
fn late_geometry_failure_preserves_request_and_published_snapshot() {
    let mut doc = document().builder();
    doc.create_asset(AssetRecord {
        id: id(3),
        name: String::new(),
        content: AssetContent::Image {
            resource: ResourceKey::new(),
            width: 1,
            height: 1,
        },
    })
    .unwrap();
    assert!(doc
        .create_mesh(Mesh {
            id: id(4),
            runtime_id: "Mesh".into(),
            texture_asset_id: (id(3)).into(),
            vertex_ids: vec![1, 2, 3],
            base_positions: vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)],
            uvs: vec![Vec2::new(0., 0.); 3],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        })
        .is_ok());
    let doc = doc.freeze().unwrap().state;
    let saved = doc.clone();
    let mut state = PreviewState::default();
    state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap();
    let frame = state.frame(&doc, 1).unwrap();
    let revision = state.revision();
    let mut doc = doc.builder();
    let mut mesh = doc.mesh(&id(4)).unwrap().clone();
    mesh.base_positions[0].x = f32::MAX;
    assert!(doc.replace_mesh(mesh).is_ok());
    let rotation_id = id(9999);
    assert!(doc
        .create_transform(Transform {
            id: rotation_id.clone(),
            runtime_id: "Rotation".into(),
            data: TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    scale: 2.,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        })
        .is_ok());
    let mut overflow_mesh = doc.mesh(&id(4)).unwrap().clone();
    overflow_mesh.deformer_id = rotation_id;
    assert!(doc.replace_mesh(overflow_mesh).is_ok());
    let mut doc = doc.freeze().unwrap().state;
    assert_eq!(
        state
            .replace(&doc, 1, [(id(2), 0.75)].into())
            .unwrap_err()
            .code,
        "NON_FINITE"
    );
    // Failure with unchanged inputs must preserve the cached successful frame too.
    assert_eq!(
        state
            .replace(&doc, 1, [(id(2), 0.5)].into())
            .unwrap_err()
            .code,
        "NON_FINITE"
    );
    assert_eq!(state.values()[&id(2)], 0.5);
    assert_eq!(state.revision(), revision);
    assert!(Arc::ptr_eq(&frame, &state.frame(&saved, 1).unwrap()));
    // The old successful frame must not be reported as current for the changed document.
    let evaluations = state.evaluation_count();
    for _ in 0..3 {
        assert_eq!(state.frame(&doc, 1).unwrap_err().code, "NON_FINITE");
    }
    assert_eq!(state.evaluation_count(), evaluations + 1);
    doc = saved.clone();
    assert_eq!(state.frame(&doc, 1).unwrap().drawables, frame.drawables);
}
