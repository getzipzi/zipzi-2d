use super::*;
use crate::evaluation::{DrawableFrame, FrameEvaluator, PreviewValues};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

// Both deformers affect the mesh, so stale prepared parent/selection data changes
// observable geometry. Bound and unbound variants exercise both base and keyforms.
fn fixture(bound: bool) -> DocumentState {
    let points = vec![
        Vec2::new(0., 0.),
        Vec2::new(10., 0.),
        Vec2::new(0., 10.),
        Vec2::new(10., 10.),
    ];
    let mut input = OwnedDocument {
        id: id(1),
        assets: vec![AssetRecord {
            id: id(2),
            name: "image".into(),
            content: AssetContent::Image {
                resource: ResourceKey::new(),
                width: 1,
                height: 1,
            },
        }],
        parameters: vec![Parameter {
            id: id(3),
            runtime_id: "parameter".into(),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        }],
        transforms: vec![
            Transform {
                id: id(4),
                runtime_id: "rotation".into(),
                ..Default::default()
            },
            Transform {
                id: id(5),
                runtime_id: "warp".into(),
                parent_id: Some(id(4).into()),
                data: crate::TransformData::Warp(crate::WarpTransform {
                    rows: 1,
                    columns: 1,
                    quad: true,
                    points: points.clone(),
                    bezier: None,
                }),
                ..Default::default()
            },
        ],
        meshes: vec![Mesh {
            id: id(6),
            runtime_id: "mesh".into(),
            texture_asset_id: id(2).into(),
            deformer_id: id(5),
            vertex_ids: vec![0, 1, 2],
            base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            triangles: vec![[0, 1, 2]],
            ..Default::default()
        }],
        physics: Some(Arc::new(PhysicsAsset::empty(id(10)))),
        ..Default::default()
    };
    if bound {
        input.bindings.push(MeshBinding {
            id: id(7),
            mesh_id: id(6),
            axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
            keyforms: [0., 1.]
                .map(|key| MeshKeyform {
                    keys: vec![key],
                    positions: input.meshes[0].base_positions.clone(),
                    ..Default::default()
                })
                .into(),
        });
        input.scene_bindings = vec![
            SceneBinding {
                id: id(8),
                axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
                track: SceneTrack::Rotation {
                    target_id: id(4).into(),
                    keyforms: [0., 1.]
                        .map(|key| RotationKeyform {
                            keys: vec![key],
                            rotation: crate::RotationPose {
                                angle: key * 20.,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .into(),
                },
            },
            SceneBinding {
                id: id(9),
                axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
                track: SceneTrack::Warp {
                    target_id: id(5).into(),
                    keyforms: [0., 1.]
                        .map(|key| WarpKeyform {
                            keys: vec![key],
                            positions: points.clone(),
                            ..Default::default()
                        })
                        .into(),
                },
            },
        ];
    }
    DocumentState::from_owned(input).unwrap()
}

#[test]
fn unchanged_binding_payloads_are_not_revalidated_for_unrelated_edits() {
    use crate::model_validation::bindings::take_validation_counts;
    for edit in [
        (|b: &mut DocumentBuilder| {
            let mut mesh = b.mesh(&id(6)).unwrap().clone();
            mesh.name = "Renamed".into();
            b.replace_mesh(mesh)
        }) as fn(&mut DocumentBuilder) -> Result<(), Status>,
        |b| {
            let mut parameter = b.parameter(&id(3)).unwrap().clone();
            parameter.default_value = 0.5;
            b.replace_parameter(parameter)
        },
        |b| {
            b.create_parameter(Parameter {
                id: id(40),
                runtime_id: "unrelated".into(),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
        },
    ] {
        let base = fixture(true);
        take_validation_counts();
        let mut builder = base.builder();
        edit(&mut builder).unwrap();
        let state = builder.freeze().unwrap().state;
        assert_eq!(take_validation_counts(), (0, 0));
        let mut old = FrameEvaluator::default();
        let mut new = FrameEvaluator::default();
        let before = frame(&mut old, &base, 0.5);
        let after = frame(&mut new, &state, 0.5);
        assert_eq!(before.drawables, after.drawables);
        assert_eq!(before.deformers, after.deformers);
    }
}

#[test]
fn metadata_edits_share_warm_evaluation_without_skipping_invalid_defaults() {
    let base = fixture(true);
    let expected = frame(&mut FrameEvaluator::default(), &base, 0.5);
    let mut builder = base.builder();
    let mut mesh = builder.mesh(&id(6)).unwrap().clone();
    mesh.name = "Renamed mesh".into();
    builder.replace_mesh(mesh).unwrap();
    let mut transform = builder.transform(&id(5)).unwrap().clone();
    transform.name = "Renamed warp".into();
    builder.replace_transform(transform).unwrap();
    let mut parameter = builder.parameter(&id(3)).unwrap().clone();
    parameter.default_value = 0.5;
    builder.replace_parameter(parameter).unwrap();
    assert!(builder.validate_metadata_payload().unwrap());
    let full = freeze_full(builder.clone()).unwrap();
    let edited = builder.freeze().unwrap().state;
    assert!(Arc::ptr_eq(&base.read_cache, &edited.read_cache));
    assert_eq!(
        frame(&mut FrameEvaluator::default(), &edited, 0.5),
        expected
    );
    assert_eq!(frame(&mut FrameEvaluator::default(), &full, 0.5), expected);
    assert_eq!(edited.parameter(&id(3)).unwrap().default_value, 0.5);
    assert_eq!(edited.mesh(&id(6)).unwrap().name, "Renamed mesh");
    let mut evaluator = FrameEvaluator::default();
    let mut output = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&base, &PreviewValues::default(), &mut output)
        .is_ok());
    assert!(evaluator
        .evaluate(&edited, &PreviewValues::default(), &mut output)
        .is_ok());
    assert_eq!(output.drawables, expected.drawables);
    for value in [f32::NAN, -1., 2.] {
        let mut builder = base.builder();
        let mut parameter = builder.parameter(&id(3)).unwrap().clone();
        parameter.default_value = value;
        assert!(builder
            .replace_parameter(parameter)
            .and_then(|_| builder.freeze())
            .is_err());
    }
    // Mixing a label with a structural/reference edit must still validate the graph.
    let mut builder = base.builder();
    let mut mesh = builder.mesh(&id(6)).unwrap().clone();
    mesh.name = "Renamed".into();
    mesh.texture_asset_id = id(99).into();
    builder.replace_mesh(mesh).unwrap();
    assert!(!builder.validate_metadata_payload().unwrap());
    assert!(builder.freeze().is_err());
}

#[test]
fn cached_binding_validation_is_invalidated_by_domains_targets_and_ownership() {
    use crate::model_validation::bindings::take_validation_counts;
    let base = fixture(true);
    for edit in [
        (|b: &mut DocumentBuilder| {
            let mut parameter = b.parameter(&id(3)).unwrap().clone();
            parameter.maximum = 0.75;
            b.replace_parameter(parameter)
        }) as fn(&mut DocumentBuilder) -> Result<(), Status>,
        |b| b.remove_parameter(&id(3)),
        |b| {
            let mut mesh = b.mesh(&id(6)).unwrap().clone();
            mesh.vertex_ids.pop();
            mesh.base_positions.pop();
            mesh.uvs.pop();
            mesh.triangles.clear();
            b.replace_mesh(mesh)
        },
        |b| {
            let mut binding = b.binding(&id(7)).unwrap().clone();
            binding.id = id(40);
            binding.axes[0].id = id(41);
            for (index, sample) in binding.axes[0].samples.iter_mut().enumerate() {
                sample.id = id(42 + index as u32);
                binding.keyforms[index].samples = vec![sample.id.clone()];
            }
            b.create_binding(binding)
        },
    ] {
        let mut builder = base.builder();
        edit(&mut builder).unwrap();
        take_validation_counts();
        let error = builder.freeze().unwrap_err();
        assert!(
            matches!(
                error.code.as_str(),
                "INVALID_KEYS" | "MISSING_PARAMETER" | "INVALID_LENGTH" | "BINDING_CONFLICT"
            ),
            "{error:?}"
        );
        let counts = take_validation_counts();
        // Missing references / duplicate ownership can be rejected during
        // identity admission or normalization before payload validation.
        if matches!(error.code.as_str(), "INVALID_KEYS" | "INVALID_LENGTH") {
            assert!(counts.0 > 0);
        }
    }
    let mut builder = base.builder();
    let mut warp = builder.transform(&id(5)).unwrap().clone();
    let grid = warp.warp_mut().unwrap();
    grid.columns = 2;
    grid.points = vec![
        Vec2::new(0., 0.),
        Vec2::new(5., 0.),
        Vec2::new(10., 0.),
        Vec2::new(0., 10.),
        Vec2::new(5., 10.),
        Vec2::new(10., 10.),
    ];
    builder.replace_transform(warp).unwrap();
    take_validation_counts();
    assert_eq!(builder.freeze().unwrap_err().code, "INVALID_LENGTH");
    assert!(take_validation_counts().1 > 0);
}

/// Independent oracle: deliberately bypass payload admission and validation reuse.
fn freeze_full(mut builder: DocumentBuilder) -> Result<DocumentState, Status> {
    builder.normalize_touched()?;
    builder.content.validate(&builder.base.content)?;
    builder.finish_scene_initializations()?;
    builder.content.validate(&builder.base.content)?;
    builder.validation = CandidateValidation::ValidatedWithNewLayout;
    Ok(builder.freeze()?.state)
}

#[test]
fn admitted_keyform_drags_match_full_validation_and_retain_old_snapshots() {
    let base = fixture(true);
    let original = frame(&mut FrameEvaluator::default(), &base, 0.5);
    let mut builder = base.builder();
    builder.validate_candidate().unwrap();
    for step in 0..12 {
        let mut form = builder.binding(&id(7)).unwrap().keyforms[step % 2].clone();
        form.positions[0].x = step as f32 * 0.2;
        form.appearance.opacity = 0.3 + step as f32 * 0.05;
        // Numeric and stable-sample addressing both preserve the admitted grid.
        if step % 2 == 0 {
            form.samples.clear();
        } else {
            form.keys.clear();
        }
        builder.set_mesh_keyform(&id(7), form).unwrap();
        assert_eq!(
            builder.validation,
            CandidateValidation::ValidatedWithSharedLayout
        );
        let actual = builder.preview().unwrap();
        let expected = freeze_full(builder.clone()).unwrap();
        assert_eq!(actual.binding(&id(7)), expected.binding(&id(7)));
        for value in [0., 0.35, 1.] {
            assert_eq!(
                frame(&mut FrameEvaluator::default(), &actual, value),
                frame(&mut FrameEvaluator::default(), &expected, value)
            );
        }
        assert_eq!(frame(&mut FrameEvaluator::default(), &base, 0.5), original);
    }
    let frozen = builder.freeze().unwrap();
    assert!(!frozen.writes.is_empty());
}

#[test]
fn admitted_deformer_keyform_drags_match_full_validation() {
    let base = fixture(true);
    let original = frame(&mut FrameEvaluator::default(), &base, 0.5);
    let mut builder = base.builder();
    builder.validate_candidate().unwrap();
    for step in 0..6 {
        let SceneTrack::Rotation { keyforms, .. } = &builder.scene_binding(&id(8)).unwrap().track
        else {
            unreachable!()
        };
        let mut rotation = keyforms[step % 2].clone();
        rotation.rotation.angle += step as f32 * 10.;
        rotation.keys.clear();
        let SceneTrack::Warp { keyforms, .. } = &builder.scene_binding(&id(9)).unwrap().track
        else {
            unreachable!()
        };
        let mut warp = keyforms[step % 2].clone();
        warp.positions[1].x += step as f32;
        warp.samples.clear();

        let mut oracle = builder.clone();
        for (binding, form) in [
            (id(8), SceneKeyform::Rotation(rotation)),
            (id(9), SceneKeyform::Warp(warp)),
        ] {
            oracle.validation = CandidateValidation::NeedsValidation;
            oracle.set_scene_keyform(&binding, form.clone()).unwrap();
            builder.set_scene_keyform(&binding, form).unwrap();
            assert_eq!(
                builder.validation,
                CandidateValidation::ValidatedWithSharedLayout
            );
        }
        let actual = builder.preview().unwrap();
        let expected = freeze_full(oracle).unwrap();
        for binding in [id(8), id(9)] {
            assert_eq!(
                actual.scene_binding(&binding),
                expected.scene_binding(&binding)
            );
        }
        for value in [0., 0.35, 1.] {
            assert_eq!(
                frame(&mut FrameEvaluator::default(), &actual, value),
                frame(&mut FrameEvaluator::default(), &expected, value)
            );
        }
        assert_eq!(frame(&mut FrameEvaluator::default(), &base, 0.5), original);
    }
}

#[test]
fn admitted_deformer_keyform_edits_preserve_errors_and_recheck_dependencies() {
    let base = fixture(true);
    let SceneTrack::Rotation { keyforms, .. } = &base.scene_binding(&id(8)).unwrap().track else {
        unreachable!()
    };
    let mut rotation = keyforms[0].clone();
    rotation.rotation.scale = f32::NAN;
    let SceneTrack::Warp { keyforms, .. } = &base.scene_binding(&id(9)).unwrap().track else {
        unreachable!()
    };
    let mut warp = keyforms[0].clone();
    warp.positions.pop();
    for (binding, form, code) in [
        (id(8), SceneKeyform::Rotation(rotation), "INVALID_ROTATION"),
        (id(9), SceneKeyform::Warp(warp), "INVALID_LENGTH"),
    ] {
        let mut builder = base.builder();
        builder.validate_candidate().unwrap();
        let mut oracle = builder.clone();
        oracle.validation = CandidateValidation::NeedsValidation;
        let expected = oracle
            .set_scene_keyform(&binding, form.clone())
            .unwrap_err();
        let actual = builder.set_scene_keyform(&binding, form).unwrap_err();
        assert_eq!(actual.code, code);
        assert_eq!(actual, expected);
        assert_eq!(builder.freeze().unwrap_err(), expected);
    }

    let mut builder = base.builder();
    builder.validate_candidate().unwrap();
    let mut parameter = builder.parameter(&id(3)).unwrap().clone();
    parameter.maximum = 0.5;
    builder.replace_parameter(parameter).unwrap();
    let form = SceneKeyform::Warp(keyforms[0].clone());
    assert_eq!(
        builder.set_scene_keyform(&id(9), form).unwrap_err().code,
        "INVALID_KEYS"
    );
}

#[test]
fn admitted_keyform_drag_rejects_bad_payload_and_poisoned_candidates() {
    for (edit, code) in [
        (
            (|f: &mut MeshKeyform| {
                f.positions.pop();
            }) as fn(&mut MeshKeyform),
            "INVALID_LENGTH",
        ),
        (|f| f.positions[0].x = f32::NAN, "NON_FINITE"),
        (|f| f.appearance.opacity = f32::NAN, "INVALID_OPACITY"),
        (|f| f.keys[0] = 0.5, "STALE_SAMPLE_VALUES"),
    ] {
        let base = fixture(true);
        let mut builder = base.builder();
        builder.validate_candidate().unwrap();
        let mut form = builder.binding(&id(7)).unwrap().keyforms[0].clone();
        edit(&mut form);
        let error = builder.set_mesh_keyform(&id(7), form).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(builder.freeze().unwrap_err().code, code);
    }
}

#[test]
fn keyform_drag_revalidates_after_dependency_or_draw_order_changes() {
    let base = fixture(true);
    let mut builder = base.builder();
    builder.validate_candidate().unwrap();
    let mut parameter = builder.parameter(&id(3)).unwrap().clone();
    parameter.maximum = 0.5;
    builder.replace_parameter(parameter).unwrap();
    let form = builder.binding(&id(7)).unwrap().keyforms[0].clone();
    assert_eq!(
        builder.set_mesh_keyform(&id(7), form).unwrap_err().code,
        "INVALID_KEYS"
    );

    let mut builder = base.builder();
    builder.validate_candidate().unwrap();
    let mut form = builder.binding(&id(7)).unwrap().keyforms[0].clone();
    form.draw_order = Some(750.);
    builder.set_mesh_keyform(&id(7), form).unwrap();
    assert_eq!(builder.validation, CandidateValidation::NeedsValidation);
    let expected = freeze_full(builder.clone()).unwrap();
    let actual = builder.freeze().unwrap().state;
    for value in [0., 0.5, 1.] {
        assert_eq!(
            frame(&mut FrameEvaluator::default(), &actual, value),
            frame(&mut FrameEvaluator::default(), &expected, value)
        );
    }
}

#[test]
fn admitted_keyform_drag_keeps_prior_structural_changes_and_new_layout() {
    let base = fixture(true);
    let mut builder = base.builder();
    let mut mesh = builder.mesh(&id(6)).unwrap().clone();
    mesh.deformer_id = id(4);
    builder.replace_mesh(mesh).unwrap();
    builder.validate_candidate().unwrap();
    assert_eq!(
        builder.validation,
        CandidateValidation::ValidatedWithNewLayout
    );
    let mut form = builder.binding(&id(7)).unwrap().keyforms[1].clone();
    form.positions[0].x += 2.;
    builder.set_mesh_keyform(&id(7), form).unwrap();
    assert_eq!(
        builder.validation,
        CandidateValidation::ValidatedWithNewLayout
    );
    let expected = freeze_full(builder.clone()).unwrap();
    let actual = builder.freeze().unwrap().state;
    assert_ne!(base.evaluation_layout_id(), actual.evaluation_layout_id());
    for value in [0., 0.5, 1.] {
        assert_eq!(
            frame(&mut FrameEvaluator::default(), &actual, value),
            frame(&mut FrameEvaluator::default(), &expected, value)
        );
    }
}

fn frame(evaluator: &mut FrameEvaluator, state: &DocumentState, value: f32) -> DrawableFrame {
    let values: PreviewValues = [(id(3), value)].into();
    let mut frame = DrawableFrame::default();
    let status = evaluator.evaluate(state, &values, &mut frame);
    assert!(status.is_ok(), "{status:?}");
    frame
}

fn compare_edit(
    bound: bool,
    edit: fn(&mut DocumentBuilder) -> Result<(), Status>,
    shared: bool,
    error: Option<&str>,
) {
    for warm in [false, true] {
        let base = fixture(bound);
        let mut evaluator = FrameEvaluator::default();
        if warm {
            frame(&mut evaluator, &base, 0.5);
            base.ordered_parameter_ids();
            base.binding_for_mesh(&id(6));
            base.binding_for_scene(&id(5));
        }
        let mut builder = base.builder();
        edit(&mut builder).unwrap();
        if shared {
            match builder.validate_frame_payload() {
                Ok(eligible) => assert!(eligible, "fixture missed the fast path"),
                Err(status) => assert_eq!(Some(status.code.as_str()), error),
            }
        }
        let full = freeze_full(builder.clone());
        let fast = builder.freeze().map(|frozen| frozen.state);
        if let Some(code) = error {
            assert_eq!(fast.unwrap_err().code, code);
            assert_eq!(full.unwrap_err().code, code);
            continue;
        }
        let (fast, full) = (fast.unwrap(), full.unwrap());
        assert_eq!(Arc::ptr_eq(&base.read_cache, &fast.read_cache), shared);
        assert_eq!(
            base.evaluation_layout_id() == fast.evaluation_layout_id(),
            shared
        );
        assert!(!Arc::ptr_eq(&base.read_cache, &full.read_cache));
        assert_eq!(fast.mesh_order(), full.mesh_order());
        assert_eq!(fast.transform_order(), full.transform_order());
        for id in fast.mesh_order() {
            assert_eq!(fast.mesh(id), full.mesh(id));
        }
        for id in fast.transform_order() {
            assert_eq!(fast.transform(id), full.transform(id));
        }
        for id in fast.binding_order() {
            assert_eq!(fast.binding(id), full.binding(id));
        }
        for id in fast.scene_binding_order() {
            assert_eq!(fast.scene_binding(id), full.scene_binding(id));
        }
        assert_eq!(fast.physics(), full.physics());
        assert_eq!(fast.editor_state(), full.editor_state());
        // Reuse one evaluator through edit / undo / redo, including interpolated
        // parameters. The oracle reconstructs caches for the original snapshot.
        let mut cold_base = base.clone();
        cold_base.read_cache = Arc::default();
        for (actual, expected) in [(&fast, &full), (&base, &cold_base), (&fast, &full)] {
            for value in [0., 0.35, 1.] {
                assert_eq!(
                    frame(&mut evaluator, actual, value),
                    frame(&mut FrameEvaluator::default(), expected, value),
                    "warm={warm}, value={value}"
                );
            }
        }
        // Ensure these are observable edits, not vacuous comparisons of empty frames.
        assert_ne!(
            frame(&mut evaluator, &base, 1.),
            frame(&mut evaluator, &fast, 1.)
        );
    }
}

#[test]
fn base_pose_and_appearance_match_full_validation_with_cold_and_warm_caches() {
    for edit in [
        (|b: &mut DocumentBuilder| {
            let mut t = b.transform(&id(4)).unwrap().clone();
            t.rotation_mut().unwrap().pose.angle = 35.;
            t.appearance.opacity = 0.6;
            b.replace_transform(t)
        }) as fn(&mut DocumentBuilder) -> Result<(), Status>,
        |b| {
            let mut t = b.transform(&id(5)).unwrap().clone();
            t.warp_mut().unwrap().points[0].x += 2.;
            b.replace_transform(t)
        },
        |b| {
            let mut m = b.mesh(&id(6)).unwrap().clone();
            m.base_positions[0].x += 0.2;
            m.appearance.opacity = 0.4;
            b.replace_mesh(m)
        },
    ] {
        compare_edit(false, edit, true, None);
    }
}

#[test]
fn mesh_and_scene_keyforms_match_full_validation_through_snapshot_restoration() {
    for edit in [
        (|b: &mut DocumentBuilder| {
            let mut binding = b.binding(&id(7)).unwrap().clone();
            binding.keyforms[1].positions[0].x += 0.2;
            binding.keyforms[1].appearance.opacity = 0.4;
            b.replace_binding(binding)
        }) as fn(&mut DocumentBuilder) -> Result<(), Status>,
        |b| {
            let mut binding = b.scene_binding(&id(8)).unwrap().clone();
            let SceneTrack::Rotation { keyforms, .. } = &mut binding.track else {
                unreachable!()
            };
            keyforms[1].rotation.angle += 15.;
            keyforms[1].appearance.opacity = 0.6;
            b.replace_scene_binding(binding)
        },
        |b| {
            let mut binding = b.scene_binding(&id(9)).unwrap().clone();
            let SceneTrack::Warp { keyforms, .. } = &mut binding.track else {
                unreachable!()
            };
            keyforms[1].positions[0].x += 2.;
            b.replace_scene_binding(binding)
        },
    ] {
        compare_edit(true, edit, true, None);
    }
}

#[test]
fn structural_edits_rebuild_prepared_inputs() {
    compare_edit(
        false,
        |b| {
            let mut t = b.transform(&id(5)).unwrap().clone();
            t.parent_id = None;
            b.replace_transform(t)?;
            let mut m = b.mesh(&id(6)).unwrap().clone();
            m.uvs[0].x = 0.25;
            b.replace_mesh(m)
        },
        false,
        None,
    );
    compare_edit(
        true,
        |b| {
            let mut binding = b.binding(&id(7)).unwrap().clone();
            binding.keyforms[1].draw_order = Some(200.);
            b.replace_binding(binding)
        },
        false,
        None,
    );
}

#[test]
fn invalid_payloads_fail_both_validation_paths() {
    compare_edit(
        false,
        |b| {
            let mut t = b.transform(&id(4)).unwrap().clone();
            t.rotation_mut().unwrap().pose.angle = f32::NAN;
            b.replace_transform(t)
        },
        true,
        Some("INVALID_ROTATION"),
    );
    compare_edit(
        true,
        |b| {
            let mut binding = b.scene_binding(&id(9)).unwrap().clone();
            let SceneTrack::Warp { keyforms, .. } = &mut binding.track else {
                unreachable!()
            };
            keyforms[1].positions.pop();
            b.replace_scene_binding(binding)
        },
        true,
        Some("INVALID_LENGTH"),
    );
}

#[test]
fn validation_proofs_survive_only_validated_physics_patches() {
    for structural in [false, true] {
        let base = fixture(false);
        let mut b = base.builder();
        if structural {
            let mut mesh = b.mesh(&id(6)).unwrap().clone();
            mesh.enabled = !mesh.enabled;
            b.replace_mesh(mesh).unwrap();
        }
        b.validate_candidate().unwrap();
        let proof = b.validation;
        assert_eq!(
            proof,
            if structural {
                CandidateValidation::ValidatedWithNewLayout
            } else {
                CandidateValidation::ValidatedWithSharedLayout
            }
        );
        let mut patch = PhysicsPatch {
            asset_id: id(10),
            asset_changes: vec![PhysicsAssetProperty::Fps(Some(60.))],
            rig: None,
        };
        b.patch_physics(&patch).unwrap();
        assert_eq!(b.validation, proof);
        let full = freeze_full(b.clone()).unwrap();
        let fast = b.clone().freeze().unwrap().state;
        assert_eq!(fast.physics(), full.physics());
        assert_eq!(Arc::ptr_eq(&base.read_cache, &fast.read_cache), !structural);

        patch.asset_changes = vec![PhysicsAssetProperty::Fps(Some(f32::NAN))];
        assert!(b.patch_physics(&patch).is_err());
        assert_eq!(b.validation, CandidateValidation::NeedsValidation);
        assert!(b.validate_candidate().is_err());
        assert!(b.freeze().is_err());
    }
}
