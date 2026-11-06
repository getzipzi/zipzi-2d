use std::sync::Arc;
use zipzi_core::SceneTrackEvaluation;
use zipzi_core::WarpBezierEvaluation;

use zipzi_core::{
    BindingAxis, Canvas, Parameter, Part, PartKeyform, PreviewValues, RotationKeyform,
    RotationPose, RotationTransform, SceneBinding, SceneKeyform, SceneTrack, Transform,
    TransformData, Vec2, WarpKeyform, WarpTransform,
};

use zipzi_sdk::{rectangle_mesh, AuthoringSession};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = crate::support::image_fixture(&id(2), "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        &id(3),
        "mesh",
        &id(2),
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
    )
    .unwrap();
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)?;
                edit.create_mesh(mesh)
            })
            .and_then(|value| {
                let prepared = draft.prepare("fixture")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    sdk
}

fn warp() -> Transform {
    Transform {
        id: id(6),
        part_id: Some(id(4).into()),
        parent_id: Some(id(5).into()),
        data: TransformData::Warp(WarpTransform {
            bezier: None,
            rows: 1,
            columns: 1,
            quad: true,
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
        }),
        ..Default::default()
    }
}

#[test]
fn new_warp_defaults_to_five_grid_cells_and_two_bezier_cells_and_undoes_atomically() {
    let mut sdk = session();
    let history = {
        let history = sdk.history_state();
        (history.undo_steps, history.redo_steps)
    };
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.edit_scene(
                    zipzi_core::document::SceneEdit::Create {
                        id: id(6),
                        name: "Warp".into(),
                        object: zipzi_core::document::SceneCreate::Warp {
                            bounds: Some([10., 20., 110., 220.]),
                        },
                        part_id: None,
                        parent_id: None,
                        before: None,
                        deformation_before: None,
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Create Warp")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let transform = sdk
        .snapshot()
        .document()
        .transform(&id(6))
        .cloned()
        .unwrap();
    let grid = transform.warp().unwrap();
    assert_eq!((grid.rows, grid.columns, grid.points.len()), (5, 5, 36));
    assert_eq!(grid.points[0], Vec2::new(10., 20.));
    assert_eq!(grid.points[35], Vec2::new(110., 220.));
    let controller = zipzi_core::WarpBezier::from_lattice_grid(
        grid.rows,
        grid.columns,
        grid.quad,
        &grid.points,
        transform
            .warp_authoring
            .as_ref()
            .unwrap()
            .bezier_grid
            .unwrap(),
    );
    assert_eq!(
        (controller.rows, controller.columns, controller.nodes.len()),
        (2, 2, 9)
    );
    for (actual, expected) in controller
        .lattice(grid.rows, grid.columns)
        .iter()
        .zip(&grid.points)
    {
        assert!((actual.x - expected.x).abs() < 0.001);
        assert!((actual.y - expected.y).abs() < 0.001);
    }
    assert_eq!(
        {
            let history = sdk.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (history.0 + 1, 0)
    );
    sdk.undo(sdk.version()).unwrap();
    assert!(sdk
        .snapshot()
        .document()
        .transform(&id(6))
        .cloned()
        .is_none());
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .transform(&id(6))
            .cloned()
            .unwrap(),
        transform
    );
}

#[test]
fn parts_transforms_and_distinct_parent_relations_publish_atomically() {
    let mut sdk = session();
    let original = sdk.version();
    let mut draft = sdk.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_part(Part {
                id: id(4),
                runtime_id: "Root".into(),
                ..Default::default()
            })?;
            c.create_part(Part {
                id: id(11),
                runtime_id: "Child".into(),
                parent_id: id(4),
                ..Default::default()
            })?;
            crate::support::insert_transform(
                c,
                Transform {
                    id: id(5),
                    part_id: Some(id(4).into()),
                    data: TransformData::Rotation(RotationTransform {
                        base_angle: 5.,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )?;
            let mut transform = warp();
            transform.part_id = Some(id(11).into());
            for p in &mut transform.warp_mut().unwrap().points {
                p.x += 0.1;
            }
            crate::support::insert_transform(c, transform)?;
            let mut mesh = c.read_document()?.mesh(&id(3)).unwrap().clone();
            mesh.part_id = id(11);
            mesh.deformer_id = id(6);
            c.replace_mesh(mesh)
        })
        .unwrap();
    let receipt = sdk.publish(draft.prepare("hierarchy").unwrap()).unwrap();
    assert_eq!(receipt.version.revision, original.revision + 1);
    let state = sdk.snapshot().document();
    assert_eq!(state.part_order(), &[id(4), id(11)]);
    assert_eq!(state.transform_order(), &[id(5), id(6)]);
    assert_eq!(state.mesh(&id(3)).unwrap().deformer_id, id(6));
    assert_eq!(state.transform(&id(6)).unwrap().part(), id(11));
    assert_eq!(state.transform(&id(6)).unwrap().parent(), id(5));
    assert_eq!(
        state
            .transform(&id(5))
            .unwrap()
            .rotation()
            .unwrap()
            .base_angle,
        5.
    );
    assert_eq!(
        state.transform(&id(6)).unwrap().warp().unwrap().points[0].x,
        0.1
    );
    assert!(crate::support::frame(sdk.snapshot(), &Default::default()).is_ok());
    let stale = sdk.begin_draft();
    assert!(crate::support::prepared(&sdk, |c| {
        let mut t = c.read_document()?.transform(&id(5)).unwrap().clone();
        t.parent_id = Some(id(6).into());
        c.replace_transform(t)
    })
    .is_err());
    assert!(crate::support::prepared(&sdk, |c| {
        let mut p = c.read_document()?.part(&id(4)).unwrap().clone();
        p.parent_id = id(11);
        c.replace_part(p)
    })
    .is_err());
    assert_eq!(sdk.version(), receipt.version);
    sdk.undo(sdk.version()).unwrap();
    assert!(sdk.snapshot().document().part_order().is_empty());
    assert!(sdk.snapshot().document().transform_order().is_empty());
    assert!(sdk
        .snapshot()
        .document()
        .mesh(&id(3))
        .unwrap()
        .deformer_id
        .is_empty());
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.publish(stale.prepare("old hierarchy").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
}

#[test]
fn scene_tracks_require_complete_typed_forms_and_support_replacement() {
    let mut sdk = session();
    let axis = BindingAxis {
        id: Default::default(),
        samples: Default::default(),
        parameter_id: id(7),
        keys: vec![0.0, 1.0],
    };
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_parameter({
                    let mut value = Parameter {
                        id: id(7),
                        minimum: 0.0,
                        maximum: 1.0,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                edit.create_part({
                    let mut value = Part {
                        id: id(4),
                        name: "part".into(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                crate::support::insert_transform(
                    edit,
                    Transform {
                        id: id(5),
                        data: TransformData::Rotation(RotationTransform::default()),
                        ..Default::default()
                    },
                )?;
                crate::support::insert_transform(edit, warp())?;
                edit.create_scene_binding(SceneBinding {
                    id: id(8),
                    axes: vec![axis.clone()],
                    track: SceneTrack::Part {
                        target_id: id(4).into(),
                        keyforms: vec![
                            PartKeyform {
                                samples: Default::default(),
                                keys: vec![1.0],
                                draw_order: 10.0,
                            },
                            PartKeyform {
                                samples: Default::default(),
                                keys: vec![0.0],
                                draw_order: 0.0,
                            },
                        ],
                    },
                })?;
                edit.create_scene_binding(SceneBinding {
                    id: id(9),
                    axes: vec![axis.clone()],
                    track: SceneTrack::Rotation {
                        target_id: id(5).into(),
                        keyforms: vec![0.0, 1.0]
                            .into_iter()
                            .map(|key| RotationKeyform {
                                keys: vec![key],
                                rotation: RotationPose {
                                    angle: key * 30.0,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .collect(),
                    },
                })?;
                let base = warp().warp().unwrap().points.clone();
                edit.create_scene_binding(SceneBinding {
                    id: id(10),
                    axes: vec![axis],
                    track: SceneTrack::Warp {
                        target_id: id(6).into(),
                        keyforms: vec![0.0, 1.0]
                            .into_iter()
                            .map(|key| WarpKeyform {
                                bezier: None,
                                keys: vec![key],
                                positions: base.iter().map(|p| Vec2::new(p.x + key, p.y)).collect(),
                                ..Default::default()
                            })
                            .collect(),
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("scene")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        sdk.snapshot().document().scene_binding_order(),
        &[id(8), id(9), id(10)]
    );
    assert_eq!(
        sdk.snapshot().document().scene_binding(&id(10)).unwrap().id,
        id(10)
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .scene_binding(&id(8))
            .cloned()
            .unwrap()
            .track
            .sample(0)
            .keys,
        [0.0]
    );
    assert!(crate::support::frame(sdk.snapshot(), &PreviewValues::from([(id(7), 0.5)])).is_ok());

    let mut bad = sdk.begin_draft();
    assert_eq!(
        bad.batch(bad.source(), |c| c.set_scene_keyform(
            &id(9),
            SceneKeyform::Part(PartKeyform {
                samples: Default::default(),
                keys: vec![0.0],
                draw_order: 3.0,
            })
        ))
        .unwrap_err()
        .code
        .as_str(),
        "INVALID_KEYFORM_TYPE"
    );
    assert!(bad.prepare("wrong type").is_err());

    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.set_scene_keyform(
                    &id(8),
                    SceneKeyform::Part(PartKeyform {
                        samples: Default::default(),
                        keys: vec![1.0],
                        draw_order: 20.0,
                    }),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("replace form")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .scene_binding(&id(8))
            .cloned()
            .unwrap()
            .track
            .sample(1)
            .draw_order,
        20.0
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .scene_binding(&id(8))
            .cloned()
            .unwrap()
            .track
            .sample(1)
            .draw_order,
        10.0
    );
}

#[test]
fn incomplete_scene_binding_rolls_back_its_batch() {
    let sdk = session();
    let before = sdk.version();
    let mut edit = sdk.begin_draft();
    edit.batch(edit.source(), |c| {
        c.create_parameter({
            let mut value = Parameter {
                id: id(7),
                minimum: 0.0,
                maximum: 1.0,
                ..Default::default()
            };
            value.runtime_id = value.id.clone();
            value
        })
    })
    .unwrap();
    edit.batch(edit.source(), |c| {
        c.create_part({
            let mut value = Part {
                id: id(4),
                ..Default::default()
            };
            value.runtime_id = value.id.clone();
            value
        })
    })
    .unwrap();
    assert_eq!(
        edit.batch(edit.source(), |c| c.create_scene_binding(SceneBinding {
            id: id(8),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: id(7),
                keys: vec![0.0, 1.0]
            }],
            track: SceneTrack::Part {
                target_id: id(4).into(),
                keyforms: vec![PartKeyform {
                    samples: Default::default(),
                    keys: vec![0.0],
                    draw_order: 0.0
                }],
            },
        }))
        .unwrap_err()
        .code
        .as_str(),
        "INCOMPLETE_KEYFORMS"
    );
    assert!(edit.prepare("incomplete").is_err());
    assert_eq!(sdk.version(), before);
    assert!(sdk.snapshot().document().parameter_order().is_empty());
    assert!(sdk.snapshot().document().part_order().is_empty());
}

#[test]
fn session_preview_caches_frames_and_rejects_bad_requests_without_state_change() {
    let mut sdk = session();
    let (_, p) = crate::support::prepared(&sdk, |c| {
        c.create_parameter(Parameter {
            id: id(7),
            runtime_id: "Param".into(),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })
    })
    .unwrap();
    sdk.publish(p).unwrap();
    let mut preview = zipzi_core::preview::PreviewState::default();
    preview
        .replace(
            sdk.snapshot().document(),
            0,
            PreviewValues::from([(id(7), 0.5)]),
        )
        .unwrap();
    let first = preview.frame(sdk.snapshot().document(), 0).unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &preview.frame(sdk.snapshot().document(), 0).unwrap()
    ));
    let revision = preview.revision();
    assert!(preview
        .replace(
            sdk.snapshot().document(),
            0,
            PreviewValues::from([(id(7), f32::NAN)])
        )
        .is_err());
    assert_eq!(preview.revision(), revision);
    assert!(Arc::ptr_eq(
        &first,
        &preview.frame(sdk.snapshot().document(), 0).unwrap()
    ));
    let (_, p) = crate::support::prepared(&sdk, |c| {
        c.rename(zipzi_sdk::NameTarget::Mesh(&id(3)), "renamed".into())
    })
    .unwrap();
    sdk.publish(p).unwrap();
    let renamed = preview.frame(sdk.snapshot().document(), 0).unwrap();
    assert!(!Arc::ptr_eq(&first, &renamed));
    assert_eq!(first.drawables[0].positions, renamed.drawables[0].positions);
    let updates = sdk
        .snapshot()
        .document()
        .mesh(&id(3))
        .unwrap()
        .vertex_ids
        .iter()
        .copied()
        .zip(
            sdk.snapshot()
                .document()
                .mesh(&id(3))
                .unwrap()
                .base_positions
                .iter()
                .map(|p| Vec2::new(p.x + 1., p.y)),
        )
        .collect::<Vec<_>>();
    let (_, p) =
        crate::support::prepared(&sdk, |c| c.set_vertex_positions(&id(3), &updates)).unwrap();
    sdk.publish(p).unwrap();
    assert_ne!(
        preview
            .frame(sdk.snapshot().document(), 0)
            .unwrap()
            .drawables[0]
            .positions,
        renamed.drawables[0].positions
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(preview.values()[&id(7)], 0.5);
    preview.reset();
    assert!(preview.values().is_empty());
}

#[test]
fn bezier_authoring_rejects_invalid_metadata_and_incomplete_grid_migration_atomically() {
    let mut sdk = session();
    let mut transform = warp();
    transform.parent_id = None;
    transform.part_id = None;
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(e, transform)?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(7),
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_scene_binding(SceneBinding {
                    id: id(10),
                    axes: vec![BindingAxis {
                        id: Default::default(),
                        samples: Default::default(),
                        parameter_id: id(7),
                        keys: vec![0., 1.],
                    }],
                    track: SceneTrack::Warp {
                        target_id: id(6).into(),
                        keyforms: vec![0., 1.]
                            .into_iter()
                            .map(|key| WarpKeyform {
                                keys: vec![key],
                                positions: warp().warp().unwrap().points.clone(),
                                ..Default::default()
                            })
                            .collect(),
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("warp")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = sdk
        .snapshot()
        .document()
        .transform(&id(6))
        .cloned()
        .unwrap();
    let binding = sdk
        .snapshot()
        .document()
        .scene_binding(&id(10))
        .cloned()
        .unwrap();
    let version = sdk.version();
    for invalid_grid in [false, true] {
        let mut changed = original.clone();
        let w = changed.warp_mut().unwrap();
        if invalid_grid {
            w.rows = 2;
            w.points = vec![Vec2::new(0., 0.); 6];
        } else {
            let mut b = zipzi_core::WarpBezier::from_lattice(w.rows, w.columns, w.quad, &w.points);
            b.nodes[0].handles[0].x = f32::NAN;
            w.bezier = Some(b);
        }
        assert!({
            let mut draft = sdk.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.replace_transform(changed)?;
                    e.replace_scene_binding(binding.clone())
                })
                .and_then(|value| {
                    let prepared = draft.prepare("invalid")?;
                    sdk.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .is_err());
        assert_eq!(sdk.version(), version);
        assert_eq!(
            sdk.snapshot().document().transform(&id(6)).cloned(),
            Some(original.clone())
        );
        assert_eq!(
            sdk.snapshot().document().scene_binding(&id(10)).cloned(),
            Some(binding.clone())
        );
    }
}
