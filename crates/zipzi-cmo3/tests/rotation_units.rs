mod common;
use common::*;
use zipzi_cmo3::*;
use zipzi_core::{Document, DrawableFrame, evaluation::evaluate_frame_including_hidden, types::*};

fn id(n: u32) -> String {
    format!("ca030002-0000-4000-8000-{n:012x}")
}

fn warp(points: Vec<Vec2>) -> TransformData {
    TransformData::Warp(WarpTransform {
        rows: 1,
        columns: 1,
        quad: true,
        points,
        bezier: None,
    })
}

fn rotation(origin: Vec2, scale: f32) -> TransformData {
    TransformData::Rotation(RotationTransform {
        base_angle: 7.,
        pose: RotationPose {
            origin: origin.into(),
            angle: 13.,
            scale,
            ..Default::default()
        },
    })
}

fn fixture_with_units(root_warp: bool, nested_warp: bool, flag: u8) -> (Document, Vec<u8>) {
    let (mut doc, png) = fixture();
    assert!(
        doc.replace_model_setup(Canvas::with_flag(400., 400., Vec2::default(), 5800., flag))
            .status
            .is_ok()
    );
    let mut parameter = doc.get_parameter(PARAM).unwrap().clone();
    parameter.repeat = false;
    parameter.default_value = 0.25;
    assert!(doc.replace_parameter(parameter).status.is_ok());
    if root_warp {
        assert!(
            doc.create_imported_transform(Transform {
                id: id(1),
                data: warp(vec![
                    Vec2::new(-100., -80.),
                    Vec2::new(100., -80.),
                    Vec2::new(-100., 80.),
                    Vec2::new(100., 80.)
                ]),
                ..Default::default()
            })
            .status
            .is_ok()
        );
    }
    for (n, parent, unit, origin, scale) in [
        (
            2,
            root_warp.then(|| id(1)),
            5800.,
            if root_warp {
                Vec2::new(0.25, 0.5)
            } else {
                Vec2::new(20., -15.)
            },
            1.,
        ),
        (
            4,
            Some(id(if nested_warp { 3 } else { 2 })),
            0.25,
            if nested_warp {
                Vec2::new(0.5, 0.5)
            } else {
                Vec2::new(0.02, -0.01)
            },
            0.5,
        ),
    ] {
        assert!(
            doc.create_imported_transform(Transform {
                id: id(n),
                parent_id: parent.map(Into::into),
                rotation_scale_unit: Some(unit),
                data: rotation(origin, scale),
                ..Default::default()
            })
            .status
            .is_ok()
        );
        assert!(
            doc.create_scene_binding(SceneBinding {
                id: id(n + 10),
                axes: vec![BindingAxis::new(PARAM, vec![-1., 0., 1.])],
                track: SceneTrack::Rotation {
                    target_id: id(n).into(),
                    keyforms: [-1., 0., 1.]
                        .into_iter()
                        .map(|key| RotationKeyform {
                            keys: vec![key],
                            rotation: RotationPose {
                                origin: Vec2::new(origin.x + key * 0.001, origin.y - key * 0.002)
                                    .into(),
                                angle: 13. + key * 11.,
                                scale: scale * (1. + key * 0.2),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .collect()
                }
            })
            .status
            .is_ok()
        );
        if n == 2 && nested_warp {
            assert!(
                doc.create_imported_transform(Transform {
                    id: id(3),
                    parent_id: Some(id(2).into()),
                    data: warp(vec![
                        Vec2::new(-0.02, -0.01),
                        Vec2::new(0.02, -0.01),
                        Vec2::new(-0.02, 0.01),
                        Vec2::new(0.02, 0.01)
                    ]),
                    ..Default::default()
                })
                .status
                .is_ok()
            );
            let points = doc
                .get_transform(&id(3))
                .unwrap()
                .warp()
                .unwrap()
                .points
                .clone();
            assert!(
                doc.create_scene_binding(SceneBinding {
                    id: id(13),
                    axes: vec![BindingAxis::new(PARAM, vec![-1., 0., 1.])],
                    track: SceneTrack::Warp {
                        target_id: id(3).into(),
                        keyforms: [-1., 0., 1.]
                            .into_iter()
                            .map(|key| WarpKeyform {
                                keys: vec![key],
                                positions: points
                                    .iter()
                                    .map(|p| Vec2::new(p.x + key * 0.001, p.y))
                                    .collect(),
                                ..Default::default()
                            })
                            .collect()
                    }
                })
                .status
                .is_ok()
            );
        }
    }
    // A native child shares the accumulated units without owning a conversion.
    assert!(
        doc.create_imported_transform(Transform {
            id: id(5),
            parent_id: Some(id(4).into()),
            data: rotation(Vec2::new(0.003, -0.002), 0.8),
            ..Default::default()
        })
        .status
        .is_ok()
    );
    let mut mesh = doc.get_mesh(MESH).unwrap().clone();
    mesh.deformer_id = id(5);
    mesh.base_positions = vec![
        Vec2::new(-0.004, -0.003),
        Vec2::new(0.004, -0.003),
        Vec2::new(-0.004, 0.003),
    ];
    assert!(doc.replace_mesh(mesh.clone()).status.is_ok());
    let mut binding = doc.binding_for_mesh(MESH).unwrap().clone();
    for form in &mut binding.keyforms {
        form.positions = mesh
            .base_positions
            .iter()
            .map(|p| Vec2::new(p.x + form.keys[0] * 0.001, p.y))
            .collect();
    }
    assert!(doc.replace_binding(binding).status.is_ok());
    assert!(
        doc.create_parameter(Parameter {
            id: id(20),
            runtime_id: "Blend".into(),
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })
        .status
        .is_ok()
    );
    assert!(
        doc.create_blend_key_table(BlendShapeKeyTable {
            id: id(21),
            parameter_id: id(20),
            keys: vec![-1., 0., 1.],
            base_key_idx: 1,
            samples: Default::default(),
            base_sample_id: Default::default()
        })
        .status
        .is_ok()
    );
    for n in [2, 4] {
        assert!(
            doc.create_blend_binding(BlendShapeBinding {
                id: id(n + 30),
                target_id: id(n),
                target_kind: BlendShapeTargetKind::Rotation,
                key_table_id: id(21),
                constraint_ids: vec![],
                keyforms: DeltaKeyforms::Rotation(vec![
                    DeltaRotationKeyform {
                        scale: Some(-0.9),
                        ..Default::default()
                    },
                    DeltaRotationKeyform::default(),
                    DeltaRotationKeyform {
                        origin: Some(Vec2::new(0.001, -0.002)),
                        angle: Some(9.),
                        scale: Some(if n == 2 { 5800. * 101. } else { 30. }),
                        ..Default::default()
                    }
                ])
            })
            .status
            .is_ok()
        );
    }
    if nested_warp {
        assert!(
            doc.create_blend_binding(BlendShapeBinding {
                id: id(33),
                target_id: id(3),
                target_kind: BlendShapeTargetKind::Warp,
                key_table_id: id(21),
                constraint_ids: vec![],
                keyforms: DeltaKeyforms::Warp(
                    [-1., 0., 1.]
                        .into_iter()
                        .map(|key| DeltaWarpKeyform {
                            points: vec![Vec2::new(key * 0.001, -key * 0.002); 4],
                            ..Default::default()
                        })
                        .collect()
                )
            })
            .status
            .is_ok()
        );
    }
    assert!(
        doc.create_blend_binding(BlendShapeBinding {
            id: id(36),
            target_id: MESH.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: id(21),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(
                [-1., 0., 1.]
                    .into_iter()
                    .map(|key| DeltaMeshKeyform {
                        positions: vec![Vec2::new(key * 0.0001, -key * 0.0002); 3],
                        ..Default::default()
                    })
                    .collect()
            )
        })
        .status
        .is_ok()
    );
    (doc, png)
}

#[test]
fn normalized_rotation_units_preserve_cmo3_geometry_and_both_blend_clamps() {
    for root_warp in [false, true] {
        for nested_warp in [false, true] {
            for flag in [0, 1] {
                let (source, png) = fixture_with_units(root_warp, nested_warp, flag);
                let before = source.clone();
                let mut current = source.clone();
                for pass in 0..2 {
                    let artifact = encode_cmo3(
                        &current,
                        &|_: &ImageAsset| Ok(png.clone()),
                        &Default::default(),
                    )
                    .unwrap();
                    assert!(!artifact.report.has_loss());
                    // Verify the external scale channel independently of the importer.
                    let entries = archive(&artifact.bytes);
                    let xml = roxmltree::Document::parse(
                        std::str::from_utf8(&entries.last().unwrap().3).unwrap(),
                    )
                    .unwrap();
                    assert!(
                        xml.descendants()
                            .filter(|n| n.has_tag_name("CRotationDeformerForm"))
                            .filter_map(|n| n.attribute("scale"))
                            .filter_map(|s| s.parse::<f32>().ok())
                            .any(|s| (s - 1. / 5800.).abs() < 1e-8)
                    );
                    current = import_model(&artifact.bytes).unwrap().document;
                    assert!(current.transform_order().iter().all(|id| {
                        current
                            .get_transform(id)
                            .unwrap()
                            .rotation_scale_unit
                            .is_none()
                    }));
                    for normal in [-1., -0.37, 0.25, 1.] {
                        for blend in [-1., -0.4, 0., 0.37, 1.] {
                            let values = [(PARAM.into(), normal), (id(20), blend)].into();
                            let frame = |doc: &Document| {
                                let mut frame = DrawableFrame::default();
                                assert!(
                                    evaluate_frame_including_hidden(doc, &values, &mut frame)
                                        .is_ok()
                                );
                                frame
                            };
                            let (expected, actual) = (frame(&source), frame(&current));
                            // Compare inherited scale as well: tiny lower-clamped meshes
                            // must not disappear inside the absolute position tolerance.
                            for (n, unit) in [(2, 5800.), (4, 1450.), (5, 1450.)] {
                                let scale = |frame: &DrawableFrame| {
                                    let [x, _] = frame
                                        .deformers
                                        .iter()
                                        .find(|d| d.id == id(n))
                                        .unwrap()
                                        .rotation_basis
                                        .unwrap();
                                    x.x.hypot(x.y)
                                };
                                let a = scale(&expected);
                                let b = scale(&actual) * unit;
                                assert!(
                                    (a - b).abs() <= 1e-9 + a.abs().max(b.abs()) * 2e-6,
                                    "inherited scale: transform={n}, pass={pass}, normal={normal}, blend={blend}: {a} != {b}"
                                );
                            }
                            let (expected, actual) = (&expected.drawables[0], &actual.drawables[0]);
                            assert_eq!(expected.positions.len(), actual.positions.len());
                            for (a, b) in expected.positions.iter().zip(&actual.positions) {
                                for (a, b) in [(a.x, b.x), (a.y, b.y)] {
                                    let tolerance = 0.01 + a.abs().max(b.abs()) * 2e-6;
                                    assert!(
                                        (a - b).abs() <= tolerance,
                                        "root_warp={root_warp}, nested_warp={nested_warp}, flag={flag}, pass={pass}, normal={normal}, blend={blend}: {a} != {b}"
                                    );
                                }
                            }
                            assert_eq!(expected.uvs, actual.uvs);
                            assert_eq!(expected.opacity, actual.opacity);
                        }
                    }
                }
                assert_eq!(source.revision(), before.revision());
                assert!(source.same_content(&before));
            }
        }
    }
}
