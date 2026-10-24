use zipzi_core::{PartKeyform, SceneTrack};

use std::collections::HashMap;
use std::fs;

use super::helpers::*;
use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::types::{BindingAxis, BlendShapeTargetKind, DeltaKeyforms, Parameter};
use zipzi_moc3::{encode_moc3, import_from_bare_moc3, inspect_moc3, Moc3Version};

#[test]
fn blend_colors_and_fractional_orders_match_core_and_roundtrip() {
    use zipzi_core::types::*;
    let mut doc = create_m1_fixture_doc();
    let mesh_id = doc.mesh_order()[0].clone();
    let binding_id = doc.binding_for_mesh(&mesh_id).unwrap().id.clone();
    assert!(doc.erase_object(&binding_id).status.is_ok());
    let mut mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    mesh.draw_order = Some(10.75);
    mesh.appearance.multiply = [0.5; 3];
    mesh.appearance.screen = [0.1; 3];
    assert!(doc.replace_mesh(mesh.clone()).status.is_ok());
    assert!(doc
        .create_parameter(Parameter {
            id: id(400),
            runtime_id: "ReviewBlend".into(),
            minimum: 0.0,
            maximum: 2.0,
            default_value: 0.0,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: id(401),
            parameter_id: id(400),
            keys: vec![0.0, 1.0, 2.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    let base = DeltaMeshKeyform {
        positions: vec![Vec2::default(); mesh.vertex_ids.len()],
        ..Default::default()
    };
    let mut left = base.clone();
    left.multiply = Some([0.2; 3]);
    left.draw_order = Some(0.5);
    let mut right = base.clone();
    right.screen = Some([0.4; 3]);
    right.draw_order = Some(0.5);
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
            id: id(402),
            target_id: mesh_id.clone(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: id(401),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(vec![base, left, right]),
        })
        .status
        .is_ok());
    let encoded = encode_moc3(&doc).unwrap();
    let imported = import_from_bare_moc3(&encoded.bytes, &fixture_textures())
        .unwrap()
        .document;
    let forms = &imported
        .get_blend_binding(&imported.blend_binding_order()[0])
        .unwrap()
        .keyforms;
    let DeltaKeyforms::Mesh(forms) = forms else {
        panic!("wrong target");
    };
    assert!(forms[1].screen.is_none());
    assert!(forms[2].multiply.is_none());
    let mut runtime = PurismModelInstance::new(&encoded.bytes);
    for value in [0.0, 1.0, 1.5, 2.0, 1.5, 0.0] {
        runtime.set_parameter("ReviewBlend", value);
        runtime.update();
        let expected = runtime.get_drawable(&mesh.runtime_id).unwrap();
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(&doc, &HashMap::from([(id(400), value)]), &mut frame).is_ok());
        let actual = frame.drawables.iter().find(|d| d.id == mesh_id).unwrap();
        assert_eq!(actual.draw_order, expected.draw_order, "value={value}");
        for channel in 0..3 {
            near(
                actual.multiply_color[channel],
                expected.multiply_color[channel],
                1.0,
            );
            near(
                actual.screen_color[channel],
                expected.screen_color[channel],
                1.0,
            );
        }
    }
}

#[test]
fn animated_glue_roundtrips_and_matches_core() {
    use zipzi_core::types::{Glue, GlueVertexPair};
    let mut doc = create_m1_fixture_doc();
    let mesh_id = doc.mesh_order()[0].clone();
    let vertices = doc.get_mesh(&mesh_id).unwrap().vertex_ids.clone();
    assert!(doc
        .create_glue(Glue {
            id: id(410),
            runtime_id: "ReviewGlue".into(),
            name: "ReviewGlue".into(),
            mesh_a_id: (mesh_id.clone()).into(),
            mesh_b_id: (mesh_id).into(),
            pairs: vec![GlueVertexPair {
                vertex_a: Some(vertices[0]),
                vertex_b: Some(vertices[1]),
                weight_a: 0.2,
                weight_b: 0.8,
                ..Default::default()
            }],
            intensity: 1.0,
            binding: None,
        })
        .status
        .is_ok());
    let mut bytes = encode_moc3(&doc).unwrap().bytes;
    let report = inspect_moc3(&bytes).unwrap();
    let ordinary_binding = i32::from_le_bytes(
        bytes[report.section_offsets[34] as usize..][..4]
            .try_into()
            .unwrap(),
    );
    // Supply a valid three-key intensity window and point the Glue at the mesh's normal binding.
    let count_off = report.section_offsets[0] as usize + 22 * 4;
    bytes[count_off..count_off + 4].copy_from_slice(&3i32.to_le_bytes());
    import_cases::set_i32(&mut bytes, 91, 0, ordinary_binding);
    import_cases::set_i32(&mut bytes, 93, 0, 3);
    let data_off = report.section_offsets[100] as usize;
    assert!(report.section_offsets[101] as usize >= data_off + 12);
    for (i, v) in [0.0f32, 0.5, 1.0].iter().enumerate() {
        bytes[data_off + i * 4..data_off + (i + 1) * 4].copy_from_slice(&v.to_le_bytes());
    }
    let inspection = zipzi_moc3::inspect_moc3_safety(&bytes).unwrap();
    assert!(inspection.unsupported_features.is_empty());
    let imported = import_from_bare_moc3(&bytes, &fixture_textures())
        .unwrap()
        .document;
    let glue = imported.get_glue(&imported.glue_order()[0]).unwrap();
    let binding = glue.binding.as_ref().unwrap();
    assert_eq!(
        binding
            .keyforms
            .iter()
            .map(|k| k.intensity)
            .collect::<Vec<_>>(),
        vec![0.0, 0.5, 1.0]
    );
    let exported = encode_moc3(&imported).unwrap().bytes;
    let mut original_core = PurismModelInstance::new(&bytes);
    let mut exported_core = PurismModelInstance::new(&exported);
    let parameter = imported
        .get_parameter(&binding.axes[0].parameter_id)
        .unwrap();
    for value in [
        parameter.minimum,
        parameter.default_value,
        parameter.maximum,
        (parameter.maximum + parameter.default_value) * 0.5,
        parameter.minimum,
    ] {
        original_core.set_parameter(&parameter.runtime_id, value);
        exported_core.set_parameter(&parameter.runtime_id, value);
        original_core.update();
        exported_core.update();
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(
            &imported,
            &HashMap::from([(parameter.id.clone(), value)]),
            &mut frame
        )
        .is_ok());
        for actual in &frame.drawables {
            let expected = original_core.get_drawable(&actual.runtime_id).unwrap();
            let roundtrip = exported_core.get_drawable(&actual.runtime_id).unwrap();
            for ((a, e), r) in actual
                .positions
                .iter()
                .zip(&expected.positions)
                .zip(&roundtrip.positions)
            {
                near(
                    a.x / imported.source_canvas().unwrap().pixels_per_unit,
                    e.x,
                    imported.source_canvas().unwrap().pixels_per_unit,
                );
                near(
                    a.y / imported.source_canvas().unwrap().pixels_per_unit,
                    e.y,
                    imported.source_canvas().unwrap().pixels_per_unit,
                );
                near(r.x, e.x, imported.source_canvas().unwrap().pixels_per_unit);
                near(r.y, e.y, imported.source_canvas().unwrap().pixels_per_unit);
            }
        }
    }
    // A second ordinary axis exercises the dedicated encoder grid and ordering.
    let mut multi = imported.clone();
    assert!(multi
        .create_parameter(zipzi_core::types::Parameter {
            id: id(411),
            runtime_id: "GlueSecondAxis".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let mut glue = multi.get_glue(&multi.glue_order()[0]).unwrap().clone();
    let grid = glue.binding.as_mut().unwrap();
    let axis = zipzi_core::types::BindingAxis::new(id(411), vec![0., 1.]);
    for form in &mut grid.keyforms {
        form.samples.push(axis.samples[0].id.clone());
    }
    grid.keyforms.extend(
        [0.25, 0.75, 1.25]
            .into_iter()
            .enumerate()
            .map(|(i, intensity)| zipzi_core::types::GlueKeyform {
                samples: vec![
                    grid.axes[0].samples[i].id.clone(),
                    axis.samples[1].id.clone(),
                ],
                intensity,
            }),
    );
    grid.axes.push(axis);
    assert!(multi.replace_glue(glue).status.is_ok());
    let encoded = encode_moc3(&multi).unwrap();
    let mut runtime = PurismModelInstance::new(&encoded.bytes);
    for (a, b) in [
        (parameter.minimum, 0.0),
        (parameter.default_value, 0.5),
        (parameter.maximum, 1.0),
        (parameter.minimum, 0.0),
    ] {
        runtime.set_parameter(&parameter.runtime_id, a);
        runtime.set_parameter("GlueSecondAxis", b);
        runtime.update();
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(
            &multi,
            &HashMap::from([(parameter.id.clone(), a), (id(411), b)]),
            &mut frame
        )
        .is_ok());
        for actual in frame.drawables {
            let expected = runtime.get_drawable(&actual.runtime_id).unwrap();
            for (a, e) in actual.positions.iter().zip(&expected.positions) {
                near(
                    a.x / multi.source_canvas().unwrap().pixels_per_unit,
                    e.x,
                    multi.source_canvas().unwrap().pixels_per_unit,
                );
                near(
                    a.y / multi.source_canvas().unwrap().pixels_per_unit,
                    e.y,
                    multi.source_canvas().unwrap().pixels_per_unit,
                );
            }
        }
    }
    let mut glue = doc.get_glue(&id(410)).unwrap().clone();
    glue.runtime_id = "x".repeat(64);
    assert!(doc.replace_glue(glue).status.is_ok());
    assert_eq!(encode_moc3(&doc).unwrap_err().code, "UNREPRESENTABLE_ID");
}

#[test]
fn test_cyclic_parameter_moc3_roundtrip_and_evaluation() {
    let mut doc = create_m1_fixture_doc();
    let param_id = doc.parameter_order()[0].clone();
    let mut p = doc.get_parameter(&param_id).unwrap().clone();
    p.repeat = true;
    assert!(doc.replace_parameter(p).status.is_ok());

    let encoded = encode_moc3(&doc).expect("encode_moc3 failed");
    let offsets = &inspect_moc3(&encoded.bytes).unwrap().section_offsets;
    let rep_off = offsets[54] as usize;
    let rep_val = i32::from_le_bytes(encoded.bytes[rep_off..rep_off + 4].try_into().unwrap());
    assert_eq!(rep_val, 1, "Section 54 (param_src.repeat) must be 1");

    let imported = import_from_bare_moc3(&encoded.bytes, &fixture_textures())
        .expect("Import encoded cyclic model");
    let re_doc = &imported.document;
    assert!(
        re_doc
            .get_parameter(re_doc.parameter_order()[0].as_str())
            .unwrap()
            .repeat,
        "Re-imported document must have repeat: true"
    );

    // Evaluate across periods on both original and re-imported
    for &test_val in &[-3.5, -1.5, -1.0, 0.0, 0.5, 1.0, 2.5, 4.5] {
        let mut preview = HashMap::new();
        preview.insert(param_id.clone(), test_val);

        let mut frame_orig = zipzi_core::evaluation::DrawableFrame::default();
        let mut frame_re = zipzi_core::evaluation::DrawableFrame::default();

        let re_param_id = re_doc.parameter_order()[0].clone();
        let mut preview_re = HashMap::new();
        preview_re.insert(re_param_id, test_val);

        assert!(zipzi_core::evaluate_frame(&doc, &preview, &mut frame_orig).is_ok());
        assert!(zipzi_core::evaluate_frame(re_doc, &preview_re, &mut frame_re).is_ok());

        assert_eq!(frame_orig.parameters[0].value, frame_re.parameters[0].value);
        assert_eq!(
            frame_orig.drawables[0].positions,
            frame_re.drawables[0].positions
        );
    }
}

#[test]
fn test_blendshape_glue_moc3_roundtrip_and_evaluation() {
    let root = workspace_root();
    let moc3_path = root.join("fixtures/external_v50_bs_glue/model.moc3");
    let bytes = fs::read(&moc3_path).expect("Read v50 bs glue moc3");

    let inspection = inspect_moc3(&bytes).expect("Inspect v50 bs glue");
    assert_eq!(inspection.version, Moc3Version::Version50);
    assert_eq!(inspection.counts.bs_glues, 1);
    assert_eq!(inspection.counts.glues, 1);

    let res = import_from_bare_moc3(&bytes, &fixture_textures()).expect("Import v50 bs glue");
    let doc = &res.document;

    assert_eq!(doc.glue_order().len(), 1);
    let glue_id = &doc.glue_order()[0];
    let glue = doc.get_glue(glue_id).unwrap();
    assert_eq!(glue.intensity, 0.2);

    assert_eq!(doc.blend_binding_order().len(), 1);
    let bb_id = &doc.blend_binding_order()[0];
    let bb = doc.get_blend_binding(bb_id).unwrap();
    assert_eq!(bb.target_kind, BlendShapeTargetKind::Glue);
    assert_eq!(bb.target_id, *glue_id);
    match &bb.keyforms {
        DeltaKeyforms::Glue(forms) => {
            assert_eq!(forms.len(), 2);
            assert_eq!(forms[0].intensity, 0.0);
            assert_eq!(forms[1].intensity, 0.6);
        }
        _ => panic!("Expected DeltaKeyforms::Glue"),
    }

    let samples = vec![
        vec![0.0, 0.0],
        vec![0.0, 0.5],
        vec![0.5, 0.5],
        vec![1.0, 1.0],
        vec![-1.0, 1.0],
    ];
    assert_runtime_matches(doc, &bytes, &samples);
}

#[test]
fn static_offscreen_export_preserves_appearance() {
    use zipzi_core::types::{Offscreen, OffscreenKeyform, Part};
    let mut doc = create_m1_fixture_doc();
    assert!(doc
        .create_part(Part {
            id: id(900),
            runtime_id: "StaticPart".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_offscreen(Offscreen {
            id: id(901),
            runtime_id: "StaticOffscreen".into(),
            part_id: id(900),
            keyforms: vec![OffscreenKeyform {
                opacity: 0.3,
                multiply: Some([0.4, 0.5, 0.6]),
                screen: Some([0.1, 0.2, 0.3])
            }],
            ..Default::default()
        })
        .status
        .is_ok());
    let mut before = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut before).is_ok());
    let exported = encode_moc3(&doc).unwrap();
    let imported = import_from_bare_moc3(&exported.bytes, &fixture_textures()).unwrap();
    let mut after = DrawableFrame::default();
    assert!(evaluate_frame(&imported.document, &HashMap::new(), &mut after).is_ok());
    assert_eq!(before.offscreens[0].opacity, after.offscreens[0].opacity);
    assert_eq!(
        before.offscreens[0].multiply_color,
        after.offscreens[0].multiply_color
    );
    assert_eq!(
        before.offscreens[0].screen_color,
        after.offscreens[0].screen_color
    );
}

#[test]
fn joint_offscreen_edit_survives_project_and_moc3_roundtrip() {
    use zipzi_core::{Offscreen, OffscreenKeyform, Part, SceneBinding};
    let mut doc = create_m1_fixture_doc();
    let mut asset = doc.get_asset(&id(2)).unwrap().clone();
    asset.content.as_mut().unwrap().sha256 = "0".repeat(64);
    asset.content.as_mut().unwrap().source = "assets/textures/0.png".into();
    assert!(doc.replace_asset(asset).status.is_ok());
    assert!(doc
        .create_part(Part {
            id: id(910),
            runtime_id: "JointPart".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_parameter(Parameter {
            id: id(911),
            runtime_id: "JointParam".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut binding = SceneBinding {
        id: id(912),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(911),
            keys: vec![-1.0, 1.0],
        }],
        track: SceneTrack::Part {
            target_id: (id(910)).into(),
            keyforms: vec![
                PartKeyform {
                    keys: vec![-1.0],
                    ..Default::default()
                },
                PartKeyform {
                    keys: vec![1.0],
                    ..Default::default()
                },
            ],
        },
    };
    assert!(doc.create_scene_binding(binding.clone()).status.is_ok());
    let mut os = Offscreen {
        id: id(913),
        runtime_id: "JointOS".into(),
        part_id: id(910),
        part_keyform_indices: vec![0, -1],
        keyforms: vec![OffscreenKeyform {
            opacity: 0.2,
            multiply: Some([0.4, 0.5, 0.6]),
            screen: Some([0.1, 0.2, 0.3]),
        }],
        ..Default::default()
    };
    assert!(doc.create_offscreen(os.clone()).status.is_ok());
    binding.axes[0].keys.insert(1, 0.0);
    binding.track.part_keyforms_mut().unwrap().insert(
        1,
        PartKeyform {
            keys: vec![0.0],
            ..Default::default()
        },
    );
    os.keyforms.push(OffscreenKeyform {
        opacity: 0.6,
        multiply: Some([0.7, 0.8, 0.9]),
        screen: Some([0.2, 0.3, 0.4]),
    });
    os.part_keyform_indices = vec![0, 1, -1];
    assert!(doc
        .replace_part_binding_with_offscreen(binding, os.clone())
        .status
        .is_ok());
    // Reordered, reused and neutral slots must preserve interpolated appearance.
    for mapping in [
        vec![0, 1, -1],
        vec![-1, 1, 0],
        vec![1, 0, 1],
        vec![-1, -1, -1],
    ] {
        os.part_keyform_indices = mapping;
        assert!(doc.replace_offscreen(os.clone()).status.is_ok());
        let reopened =
            zipzi_project::decode_project(&zipzi_project::encode_project(&doc).unwrap()).unwrap();
        assert_eq!(reopened.get_offscreen(&id(913)), Some(&os));
        let bytes = encode_moc3(&reopened).unwrap().bytes;
        let imported = import_from_bare_moc3(&bytes, &fixture_textures())
            .unwrap()
            .document;
        let imported_param = imported
            .parameter_order()
            .iter()
            .find(|id| imported.get_parameter(id).unwrap().runtime_id == "JointParam")
            .unwrap();
        for value in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let mut before = DrawableFrame::default();
            let mut after = DrawableFrame::default();
            assert!(evaluate_frame(&doc, &HashMap::from([(id(911), value)]), &mut before).is_ok());
            assert!(evaluate_frame(
                &imported,
                &HashMap::from([(imported_param.clone(), value)]),
                &mut after
            )
            .is_ok());
            assert_eq!(before.offscreens[0].opacity, after.offscreens[0].opacity);
            assert_eq!(
                before.offscreens[0].multiply_color,
                after.offscreens[0].multiply_color
            );
            assert_eq!(
                before.offscreens[0].screen_color,
                after.offscreens[0].screen_color
            );
        }
    }
}
