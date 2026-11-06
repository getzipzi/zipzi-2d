mod support;

use support::prepare;
use zipzi_core::state::{DocumentState, RecordDomain};
use zipzi_core::SceneTrackEvaluation;
use zipzi_core::*;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn document() -> DocumentState {
    let doc =
        DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 1.).into()).unwrap();
    let mut builder = doc.builder();
    builder
        .create_parameter(Parameter {
            id: id(2),
            ..Default::default()
        })
        .unwrap();
    builder
        .create_transform(Transform {
            id: id(3),
            runtime_id: "Warp".into(),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(0., 1.),
                    Vec2::new(1., 1.),
                ],
            }),
            ..Default::default()
        })
        .unwrap();
    builder.freeze().unwrap().state
}
fn axes() -> Vec<BindingAxis> {
    vec![BindingAxis {
        id: Default::default(),
        samples: Default::default(),
        parameter_id: id(2),
        keys: vec![-1., 1.],
    }]
}

#[test]
fn warp_shape_changes_record_one_transform_and_keep_context() {
    use zipzi_core::document::{SceneField as F, SceneFieldValue as V};
    let before = document();
    let mut warp = before.transform(&id(3)).unwrap().clone();
    let TransformData::Warp(grid) = &mut warp.data else {
        unreachable!()
    };
    grid.points[0].x = -1.;
    let edited = prepare(&before, |builder| builder.replace_transform(warp)).unwrap();
    assert_eq!(edited.writes.records().len(), 1);
    assert_eq!(edited.writes.records()[0].domain, RecordDomain::Transform);
    assert_eq!(edited.writes.records()[0].id, id(3));
    let after = edited.state;
    assert_ne!(
        before.scene_field(&id(3), F::Warp),
        after.scene_field(&id(3), F::Warp)
    );

    let mut warp = after.transform(&id(3)).unwrap().clone();
    let TransformData::Warp(grid) = &mut warp.data else {
        unreachable!()
    };
    grid.columns = 2;
    grid.points = vec![Vec2::default(); 6];
    let after = prepare(&after, |builder| builder.replace_transform(warp))
        .unwrap()
        .state;
    assert_eq!(after.transform(&id(3)).unwrap().warp().unwrap().columns, 2);
    assert_eq!(
        before.scene_field(&id(3), F::Context),
        after.scene_field(&id(3), F::Context)
    );
    assert_eq!(
        V::Context {
            part_id: None,
            parent_id: None
        }
        .into_references(),
        Some(vec![
            SceneReference::Unspecified,
            SceneReference::Unspecified
        ]),
    );
}

#[test]
fn wrong_track_and_single_form_types_fail_without_mutation() {
    let doc = document();
    let invalid = SceneBinding {
        id: id(4),
        axes: axes(),
        track: SceneTrack::Rotation {
            target_id: id(3).into(),
            keyforms: vec![
                RotationKeyform {
                    keys: vec![-1.],
                    ..Default::default()
                },
                RotationKeyform {
                    keys: vec![1.],
                    ..Default::default()
                },
            ],
        },
    };
    assert_eq!(
        prepare(&doc, |builder| builder.create_scene_binding(invalid))
            .unwrap_err()
            .code,
        "INVALID_BINDING_TARGET"
    );
    assert!(doc.scene_binding_order().is_empty());
    let positions = doc
        .transform(&id(3))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    let doc = prepare(&doc, |builder| {
        builder.create_scene_binding(SceneBinding {
            id: id(4),
            axes: axes(),
            track: SceneTrack::Warp {
                target_id: id(3).into(),
                keyforms: [1., -1.]
                    .into_iter()
                    .map(|key| WarpKeyform {
                        bezier: None,
                        keys: vec![key],
                        positions: positions.clone(),
                        ..Default::default()
                    })
                    .collect(),
            },
        })
    })
    .unwrap()
    .state;
    let binding = doc.scene_binding(&id(4)).unwrap();
    assert_eq!(binding.track.sample(0).keys, [-1.]);
    assert_eq!(binding.track.sample(1).keys, [1.]);
    let before = doc.content_id();
    let mut builder = doc.builder();
    assert_eq!(
        builder
            .set_scene_keyform(
                &id(4),
                SceneKeyform::Part(PartKeyform {
                    samples: Default::default(),
                    keys: vec![-1.],
                    draw_order: 2.,
                })
            )
            .unwrap_err()
            .code,
        "INVALID_KEYFORM_TYPE"
    );
    assert_eq!(builder.freeze().unwrap_err().code, "INVALID_KEYFORM_TYPE");
    assert_eq!(doc.content_id(), before);
}

#[test]
fn typed_parent_references_and_rotation_precision_survive_edits() {
    let doc = document();
    let origin = PreciseVec2::new(16777217.125, -16777217.375);
    let rotation = Transform {
        id: id(5),
        runtime_id: "Rotation".into(),
        parent_id: Some(id(3).into()),
        data: TransformData::Rotation(RotationTransform {
            base_angle: 12.,
            pose: RotationPose {
                origin,
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    let doc = prepare(&doc, |builder| builder.create_transform(rotation))
        .unwrap()
        .state;
    let mut changed = doc.transform(&id(5)).unwrap().clone();
    assert_eq!(changed.rotation().unwrap().pose.origin, origin);
    changed.parent_id = Some("".into());
    assert_eq!(
        prepare(&doc, |builder| builder.replace_transform(changed))
            .unwrap_err()
            .code,
        "INVALID_ID"
    );
    let mut changed = doc.transform(&id(5)).unwrap().clone();
    changed.parent_id = None;
    let doc = prepare(&doc, |builder| builder.replace_transform(changed))
        .unwrap()
        .state;
    assert_eq!(
        doc.transform(&id(5))
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .origin,
        origin
    );
    assert!(doc.transform(&id(5)).unwrap().parent_id.is_none());
}
