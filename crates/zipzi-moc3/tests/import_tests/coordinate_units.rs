use super::helpers::*;
use zipzi_core::types::*;
use zipzi_core::{RotationKeyform, RotationTransform, SceneTrack, TransformData};
use zipzi_moc3::{encode_moc3, import_from_bare_moc3, inspect_moc3};

fn runtime_fixture() -> Vec<u8> {
    // Independent fixture: WarpRoot -> RotChild -> MeshQuad. Model-space
    // lengths are runtime units, while the mesh uses the Rotation's own frame.
    let mut bytes =
        std::fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let offsets = inspect_moc3(&bytes).unwrap().section_offsets;
    for (section, value) in [(1, 5800.0f32), (65, 1. / 5800.)] {
        let offset = offsets[section] as usize;
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[test]
fn normalizes_first_rotation_beneath_warp_and_preserves_local_pixels() {
    let bytes = runtime_fixture();
    let imported = import_from_bare_moc3(&bytes, &fixture_textures()).unwrap();
    let doc = &imported.document;
    let rotation = doc
        .get_transform(&imported.report.id_mapping.deformers["RotChild"])
        .unwrap();
    near(rotation.rotation().unwrap().pose.scale, 1., 1.);
    assert_eq!(rotation.rotation_scale_unit, Some(5800.));
    let warp = doc
        .get_transform(&imported.report.id_mapping.deformers["WarpRoot"])
        .unwrap();
    assert_eq!(warp.warp().unwrap().points[0], Vec2::new(-5800., -5800.));
    let binding = doc.binding_for_mesh(&doc.mesh_order()[0]).unwrap();
    assert_eq!(binding.keyforms[0].positions[0], Vec2::new(-0.7, -0.5));
    assert_runtime_matches(
        doc,
        &bytes,
        &[vec![0., 0.], vec![-1., 1.], vec![0.37, -0.52]],
    );

    // Repeated exports must not apply PPU a second time.
    let mut doc = imported.document;
    for _ in 0..3 {
        let encoded = encode_moc3(&doc).unwrap();
        let imported = import_from_bare_moc3(&encoded.bytes, &fixture_textures()).unwrap();
        let rotation = imported
            .document
            .get_transform(&imported.report.id_mapping.deformers["RotChild"])
            .unwrap();
        near(rotation.rotation().unwrap().pose.scale, 1., 1.);
        assert_runtime_matches(&imported.document, &bytes, &[vec![0.37, -0.52]]);
        doc = imported.document;
    }
    let mut settings = doc.export_settings();
    settings.pixels_per_unit = 37.5;
    assert!(doc.replace_export_settings(settings).status.is_ok());
    assert_runtime_matches(&doc, &bytes, &[vec![0.37, -0.52]]);
}

#[test]
fn normalized_scale_clamps_and_nested_frames_survive_project_and_moc3_roundtrip() {
    let bytes = runtime_fixture();
    let imported = import_from_bare_moc3(&bytes, &fixture_textures()).unwrap();
    let rotation_id = imported.report.id_mapping.deformers["RotChild"].clone();
    let mut doc = imported.document;
    let pose = doc
        .get_transform(&rotation_id)
        .unwrap()
        .rotation()
        .unwrap()
        .pose;
    assert!(doc
        .create_scene_binding(SceneBinding {
            id: id(914),
            axes: vec![BindingAxis {
                parameter_id: imported.report.id_mapping.parameters["ParamX"].clone(),
                keys: vec![-1., 0., 1.],
                ..Default::default()
            }],
            track: SceneTrack::Rotation {
                target_id: rotation_id.clone().into(),
                keyforms: [-1., 0., 1.]
                    .into_iter()
                    .map(|key| RotationKeyform {
                        keys: vec![key],
                        rotation: RotationPose {
                            origin: Vec2::new(key * 0.01, -key * 0.02).into(),
                            angle: key * 23.,
                            scale: 1. + key * 0.2,
                            ..pose
                        },
                        ..Default::default()
                    })
                    .collect(),
            },
        })
        .status
        .is_ok());
    let child_id = id(910);
    assert!(doc
        .create_transform(Transform {
            id: child_id.clone(),
            runtime_id: "NestedRotation".into(),
            parent_id: Some(rotation_id.clone().into()),
            data: TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    origin: Vec2::new(0.012, -0.007).into(),
                    angle: 13.,
                    scale: 0.8,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut mesh = doc.get_mesh(&doc.mesh_order()[0]).unwrap().clone();
    mesh.deformer_id = child_id.clone();
    // Keep even the upper-clamped pose within normal pixel magnitudes, so the
    // numerical tolerance measures conversion rather than large-coordinate ULPs.
    for point in &mut mesh.base_positions {
        point.x *= 0.01;
        point.y *= 0.01;
    }
    assert!(doc.replace_mesh(mesh).status.is_ok());
    let mut binding = doc.binding_for_mesh(&doc.mesh_order()[0]).unwrap().clone();
    for form in &mut binding.keyforms {
        for point in &mut form.positions {
            point.x *= 0.01;
            point.y *= 0.01;
        }
    }
    assert!(doc.replace_binding(binding).status.is_ok());

    assert!(doc
        .create_parameter(Parameter {
            id: id(911),
            runtime_id: "ScaleBlend".into(),
            kind: ParameterKind::BlendShape,
            minimum: -1.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            id: id(912),
            parameter_id: id(911),
            keys: vec![-1., 0., 1.],
            base_key_idx: 1,
            samples: Default::default(),
            base_sample_id: Default::default(),
        })
        .status
        .is_ok());
    // Both ends exceed the runtime clamp after applying these deltas. Preserve
    // that clamp in normalized units, including an origin and angle animation.
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
            id: id(913),
            target_id: rotation_id.clone(),
            target_kind: BlendShapeTargetKind::Rotation,
            key_table_id: id(912),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Rotation(vec![
                DeltaRotationKeyform {
                    scale: Some(-2.),
                    ..Default::default()
                },
                DeltaRotationKeyform::default(),
                DeltaRotationKeyform {
                    origin: Some(Vec2::new(0.02, -0.01)),
                    angle: Some(17.),
                    scale: Some(5800. * 101.),
                    ..Default::default()
                },
            ]),
        })
        .status
        .is_ok());
    let exported = encode_moc3(&doc).unwrap();
    let imported = import_from_bare_moc3(&exported.bytes, &fixture_textures()).unwrap();
    let root = imported
        .document
        .get_transform(&imported.report.id_mapping.deformers["RotChild"])
        .unwrap();
    let child = imported
        .document
        .get_transform(&imported.report.id_mapping.deformers["NestedRotation"])
        .unwrap();
    near(root.rotation().unwrap().pose.scale, 0.8, 1.);
    let SceneTrack::Rotation { keyforms, .. } =
        &imported.document.binding_for_scene(&root.id).unwrap().track
    else {
        panic!("not Rotation")
    };
    for (form, expected) in keyforms.iter().zip([0.8, 1., 1.2]) {
        near(form.rotation.scale, expected, 1.);
    }
    near(child.rotation().unwrap().pose.scale, 0.8, 1.);
    assert_eq!(child.rotation_scale_unit, None);
    assert_eq!(
        child.rotation().unwrap().pose.origin,
        Vec2::new(0.012, -0.007).into()
    );

    let mut project = imported.document;
    for asset_id in project.asset_order().to_vec() {
        let mut asset = project.get_asset(&asset_id).unwrap().clone();
        asset.content.as_mut().unwrap().source = "assets/texture.png".into();
        assert!(project.replace_asset(asset).status.is_ok());
    }
    let restored =
        zipzi_project::decode_project(&zipzi_project::encode_project(&project).unwrap()).unwrap();
    let samples = [
        vec![0., 0., 0.],
        vec![0.3, -0.4, -1.],
        vec![-0.7, 0.9, 0.37],
        vec![1., -1., 1.],
    ];
    for doc in [&doc, &restored] {
        assert_runtime_matches(doc, &exported.bytes, &samples);
    }
}

#[test]
fn mao_normalization_preserves_parameter_animation_and_export() {
    let path = local_mao_moc3();
    if !path.exists() {
        eprintln!("Skipping Mao normalization: {} not found", path.display());
        return;
    }
    let bytes = std::fs::read(&path).unwrap();
    let imported = import_from_bare_moc3(&bytes, &model_textures(&path)).unwrap();
    let doc = &imported.document;
    for runtime_id in ["Rotation29", "Rotation65", "Rotation63", "Rotation60"] {
        let transform = doc
            .get_transform(&imported.report.id_mapping.deformers[runtime_id])
            .unwrap();
        near(transform.rotation().unwrap().pose.scale, 1., 1.);
        if let Some(binding) = doc.binding_for_scene(&transform.id) {
            let SceneTrack::Rotation { keyforms, .. } = &binding.track else {
                panic!("not Rotation")
            };
            for form in keyforms {
                near(form.rotation.scale, 1., 1.);
            }
        }
    }
    let child = doc
        .get_transform(&imported.report.id_mapping.deformers["Rotation36"])
        .unwrap();
    assert!((child.rotation().unwrap().pose.origin.x + 734.21826).abs() < 0.001);
    // Rotation70's first key is 10.281%, but its default pose (and Rotation60's)
    // is 100%. Both bones must start with the same width at parameter defaults.
    for runtime_id in ["Rotation60", "Rotation70"] {
        near(
            doc.rotation_bone_reference_scale(&imported.report.id_mapping.deformers[runtime_id])
                .unwrap(),
            1.,
            1.,
        );
    }
    let defaults: Vec<_> = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap().default_value)
        .collect();
    let mut samples = vec![defaults.clone()];
    for (index, id) in doc.parameter_order().iter().enumerate() {
        let parameter = doc.get_parameter(id).unwrap();
        for value in [
            parameter.minimum,
            parameter.maximum,
            parameter.minimum * 0.37 + parameter.maximum * 0.63,
        ] {
            let mut sample = defaults.clone();
            sample[index] = value;
            samples.push(sample);
        }
    }
    for phase in 0..12 {
        samples.push(
            doc.parameter_order()
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let p = doc.get_parameter(id).unwrap();
                    let weight = ((index * 17 + phase * 13) % 101) as f32 / 100.;
                    p.minimum * (1. - weight) + p.maximum * weight
                })
                .collect(),
        );
    }
    // Account for accumulated float32 rounding in this PPU=5800 hierarchy,
    // while bounding every evaluated coordinate to a tenth of a model pixel.
    let max_error = assert_runtime_matches_with_position_tolerance(doc, &bytes, &samples, 0.1);
    println!(
        "Mao: {} parameter samples, max position error {max_error}px",
        samples.len()
    );
}
