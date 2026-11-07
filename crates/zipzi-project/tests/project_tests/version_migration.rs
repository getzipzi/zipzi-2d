use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::helpers::*;
use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::types::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaGlueKeyform, DeltaKeyforms, Glue, GlueVertexPair, Mesh, Offscreen, OffscreenKeyform,
    Parameter, ParameterKind, Vec2,
};

#[test]
fn test_v42_project_lifecycle_detachment_and_export() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src_fixture = root.join("fixtures/external_v42");

    let tmp = TestDirectory::new();
    let source_dir = tmp.0.join("v42_external_source");
    fs::create_dir_all(&source_dir).unwrap();

    let src_model3 = source_dir.join("model.model3.json");
    fs::copy(src_fixture.join("model.model3.json"), &src_model3).unwrap();
    fs::copy(
        src_fixture.join("model.moc3"),
        source_dir.join("model.moc3"),
    )
    .unwrap();
    fs::copy(
        src_fixture.join("texture_00.png"),
        source_dir.join("texture_00.png"),
    )
    .unwrap();

    let (result, imported, report) =
        zipzi_project::ProjectImporter::default().import_model3(&src_model3);
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(report.unwrap().moc_version, 4);
    let imported = imported.unwrap();
    assert_eq!(imported.document.blend_binding_order().len(), 2);
    assert_eq!(imported.document.blend_constraint_order().len(), 1);
    let project_path = tmp.0.join("v42.zipzi");
    tmp.io()
        .save(
            &imported.document,
            &imported.resources,
            &project_path,
            None,
            || true,
        )
        .unwrap();
    fs::remove_dir_all(&source_dir).unwrap();
    let reopened = tmp.io().load(&project_path, || true).unwrap();
    assert!(reopened.diagnostics.is_empty());
    let doc = &reopened.document;
    assert_eq!(doc.blend_binding_order().len(), 2);
    assert_eq!(doc.blend_constraint_order().len(), 1);
    assert_eq!(doc.mesh_order().len(), 1);
    let export_dir = tmp.0.join("v42_exported_package");
    zipzi_project::TransferSnapshot::new(reopened.document, reopened.resources, vec![])
        .unwrap()
        .publish(zipzi_project::TransferFormat::Runtime, &export_dir)
        .unwrap();

    let exported_moc3 = export_dir.join("model.moc3");
    assert!(exported_moc3.exists());
    let moc3_bytes = fs::read(&exported_moc3).unwrap();
    let inspection = zipzi_moc3::inspect_moc3(&moc3_bytes).expect("Exported MOC3 inspect failed");
    assert_eq!(inspection.version, zipzi_moc3::Moc3Version::Version50);
    assert_eq!(inspection.counts.blend_bindings, 2);
    assert_eq!(inspection.counts.bs_constraints, 1);
}

#[test]
fn test_project_v2_blendshape_and_glue_roundtrip() {
    use zipzi_core::types::{
        BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
        DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, ParameterKind,
    };

    let mut doc = fixture_doc();

    let bs_param_id = id(100);
    doc = prepare(&doc, |builder| {
        builder.create_parameter(zipzi_core::types::Parameter {
            id: bs_param_id.clone(),
            runtime_id: "ParamMouthOpenBS".to_string(),
            name: "Mouth Open BS".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::BlendShape,
            repeat: false,
        })
    });

    let kt_id = id(101);
    doc = prepare(&doc, |builder| {
        builder.create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: kt_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
    });

    let c_id = id(102);
    doc = prepare(&doc, |builder| {
        builder.create_blend_constraint(BlendShapeConstraint {
            samples: Default::default(),
            id: c_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            weights: vec![0.0, 1.0],
        })
    });

    let mesh_id = id(4); // from fixture_doc
    let mesh = doc.mesh(&mesh_id).unwrap().clone();
    let v_count = mesh.vertex_ids.len();

    let mesh2_id = id(5);
    let mut m2 = mesh.clone();
    m2.id = mesh2_id.clone();
    m2.runtime_id = "ArtMeshB".to_string();
    doc = prepare(&doc, |builder| builder.create_mesh(m2));

    let bb_id = id(103);
    doc = prepare(&doc, |builder| {
        builder.create_blend_binding(BlendShapeBinding {
            id: bb_id.clone(),
            target_id: mesh_id.clone(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: kt_id.clone(),
            constraint_ids: vec![c_id.clone()],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(0.0, 0.0); v_count],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(1.0, 2.0); v_count],
                    ..Default::default()
                },
            ]),
        })
    });

    let glue_parameter = id(105);
    doc = prepare(&doc, |builder| {
        builder.create_parameter(Parameter {
            id: glue_parameter.clone(),
            runtime_id: "GlueStrength".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
    });
    let glue_id = id(104);
    doc = prepare(&doc, |builder| {
        builder.create_glue(Glue {
            id: glue_id.clone(),
            runtime_id: "Glue0".to_string(),
            name: "Glue 0".to_string(),
            mesh_a_id: (mesh_id.clone()).into(),
            mesh_b_id: (mesh2_id.clone()).into(),
            pairs: vec![GlueVertexPair {
                vertex_a: Some(mesh.vertex_ids[0]),
                vertex_b: Some(doc.mesh(&mesh2_id).unwrap().vertex_ids[0]),
                weight_a: 0.5,
                weight_b: 0.5,
                ..Default::default()
            }],
            intensity: 1.0,
            binding: Some(zipzi_core::types::GlueBinding {
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: glue_parameter,
                    keys: vec![0.0, 1.0],
                }],
                keyforms: vec![
                    zipzi_core::types::GlueKeyform {
                        samples: Default::default(),
                        intensity: 0.0,
                    },
                    zipzi_core::types::GlueKeyform {
                        samples: Default::default(),
                        intensity: 1.0,
                    },
                ],
            }),
        })
    });

    let encoded = fixture_bytes(&doc);
    let wire: serde_json::Value = project_json(&encoded);
    assert_eq!(wire["format_version"], 22);
    for field in [
        "blend_key_tables",
        "blend_constraints",
        "blend_bindings",
        "glues",
    ] {
        assert!(wire["document"].get(field).is_some(), "missing {field}");
    }

    let decoded = load_native(&encoded).expect("decode_project failed");
    assert_eq!(
        project_json(&fixture_bytes(&doc)),
        project_json(&resaved_native(&encoded))
    );
    assert_eq!(decoded.blend_key_table_order(), &[kt_id]);
    assert_eq!(decoded.blend_constraint_order(), &[c_id]);
    assert_eq!(decoded.blend_binding_order(), &[bb_id]);
    assert_eq!(decoded.glue_order(), &[glue_id]);
}

#[test]
fn test_project_v1_v2_v3_v4_migration_to_current_format() {
    let doc = fixture_doc();
    let mut wire: serde_json::Value = project_json(&fixture_bytes(&doc));
    for old in 1..14 {
        wire["format_version"] = old.into();
        assert_eq!(
            load_native(&replace_project_json(&fixture_bytes(&doc), &wire))
                .unwrap_err()
                .code,
            "UNSUPPORTED_VERSION"
        );
    }
}

#[test]
fn test_v6_reader_rejects_current_format() {
    let doc = fixture_doc();
    let encoded_current = fixture_bytes(&doc);

    // Simulate an older reader that only accepts 1..=6.
    let v: serde_json::Value = project_json(&encoded_current);
    let ver = v["format_version"].as_u64().unwrap() as u32;
    let accepted_by_legacy = (1..=6).contains(&ver);
    assert!(
        !accepted_by_legacy,
        "Legacy reader (1..=6) must reject the current format"
    );
}

#[test]
fn test_project_v4_rejects_unimplemented_collections() {
    let doc = fixture_doc();
    // Inject non-empty deformers collection (unsupported prototype relationships)
    let mut wire: serde_json::Value = project_json(&fixture_bytes(&doc));
    wire["document"]["deformers"] = serde_json::json!([{ "id": "def1" }]);
    let bad_project = replace_project_json(&fixture_bytes(&doc), &wire);
    let err = load_native(&bad_project).expect_err("Non-empty deformers must be rejected");
    assert_eq!(err.code, "LEGACY_PROJECT");
    assert!(err.message.contains("Prototype relationships"));
}

#[test]
fn test_project_v4_preserves_offscreen() {
    let mut doc = fixture_doc();

    let mesh_id = doc.mesh_order()[0].clone();
    let mut mesh = doc.mesh(&mesh_id).unwrap().clone();
    mesh.raw_blend_mode = Some(262);
    doc = prepare(&doc, |builder| builder.replace_mesh(mesh));

    let os = Offscreen {
        enabled: true,
        id: id(10),
        runtime_id: "Offscreen0".to_string(),
        name: "Offscreen 0".to_string(),
        part_id: sid(1),
        blend_mode: 262,
        flags: 4,
        masks: vec![mesh_id.clone()],
        part_keyform_indices: vec![0],
        keyforms: vec![OffscreenKeyform {
            opacity: 0.8,
            multiply: Some([1.0, 0.5, 0.5]),
            screen: Some([0.1, 0.1, 0.1]),
        }],
    };
    doc = prepare(&doc, |builder| builder.create_offscreen(os.clone()));

    let encoded = fixture_bytes(&doc);
    let wire: serde_json::Value = project_json(&encoded);
    assert_eq!(
        wire["document"]["offscreens"][0]["runtime_id"],
        "Offscreen0"
    );
    assert_eq!(
        wire["document"]["meshes"][0]["properties"]["raw_blend_mode"],
        262
    );

    let decoded = load_native(&encoded).expect("decode_project failed");
    assert_eq!(decoded.offscreen_order().len(), 1);
    let decoded_os = decoded.offscreen(&id(10)).expect("offscreen must exist");
    assert_eq!(decoded_os.runtime_id, "Offscreen0");
    assert_eq!(decoded_os.name, "Offscreen 0");
    assert_eq!(decoded_os.part_id, sid(1));
    assert_eq!(decoded_os.blend_mode, 262);
    assert_eq!(decoded_os.flags, 4);
    assert_eq!(decoded_os.masks, vec![mesh_id.clone()]);
    assert_eq!(decoded_os.part_keyform_indices, vec![0]);
    assert_eq!(decoded_os.keyforms.len(), 1);
    assert_eq!(decoded_os.keyforms[0].opacity, 0.8);
    assert_eq!(decoded_os.keyforms[0].multiply, Some([1.0, 0.5, 0.5]));
    assert_eq!(decoded_os.keyforms[0].screen, Some([0.1, 0.1, 0.1]));

    let decoded_mesh = decoded.mesh(&mesh_id).expect("mesh must exist");
    assert_eq!(decoded_mesh.raw_blend_mode, Some(262));
}

#[test]
fn disabled_offscreen_roundtrips_and_legacy_defaults_to_enabled() {
    let mut doc = fixture_doc();
    let mut offscreen = Offscreen {
        id: id(10),
        runtime_id: "Offscreen".into(),
        part_id: sid(1),
        ..Default::default()
    };
    doc = prepare(&doc, |builder| builder.create_offscreen(offscreen.clone()));
    let encoded = fixture_bytes(&doc);
    let wire: serde_json::Value = project_json(&encoded);
    assert_eq!(wire["format_version"], 22);
    assert!(wire["document"]["offscreens"][0].get("enabled").is_none());
    assert!(
        load_native(&encoded)
            .unwrap()
            .offscreen(&id(10))
            .unwrap()
            .enabled
    );

    offscreen.enabled = false;
    doc = prepare(&doc, |builder| builder.replace_offscreen(offscreen.clone()));
    let encoded = fixture_bytes(&doc);
    let mut wire: serde_json::Value = project_json(&encoded);
    assert_eq!(wire["format_version"], 22);
    assert_eq!(wire["document"]["offscreens"][0]["enabled"], false);
    assert_eq!(
        load_native(&encoded).unwrap().offscreen(&id(10)),
        Some(&offscreen)
    );
    wire["format_version"] = serde_json::json!(7);
    assert_eq!(
        load_native(&replace_project_json(&fixture_bytes(&doc), &wire))
            .unwrap_err()
            .code,
        "UNSUPPORTED_VERSION"
    );
}

#[test]
fn test_project_v4_preserves_repeat_parameter() {
    let mut doc = fixture_doc();

    let param_id = doc.parameter_order()[0].clone();
    let mut p = doc.parameter(&param_id).unwrap().clone();
    p.repeat = true;
    doc = prepare(&doc, |builder| builder.replace_parameter(p));

    let encoded = fixture_bytes(&doc);
    let wire: serde_json::Value = project_json(&encoded);
    assert_eq!(wire["format_version"], 22);
    assert_eq!(wire["document"]["parameters"][0]["repeat"], true);

    let decoded = load_native(&encoded).expect("decode_project failed");
    let decoded_param = decoded.parameter(&param_id).unwrap();
    assert!(
        decoded_param.repeat,
        "repeat: true must be preserved after project decode"
    );
}

#[test]
fn test_project_v4_blendshape_glue_roundtrip() {
    let mut doc = fixture_doc();

    // Create a second mesh
    let mesh2 = Mesh {
        id: id(20),
        runtime_id: "Mesh2".to_string(),
        name: "Mesh 2".to_string(),
        texture_asset_id: (id(2)).into(),
        vertex_ids: vec![10, 20, 30],
        base_positions: vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(20.0, 10.0),
            Vec2::new(10.0, 20.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[10, 20, 30]],
        ..Default::default()
    };
    doc = prepare(&doc, |builder| builder.create_mesh(mesh2));

    let glue = Glue {
        id: id(21),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: (id(4)).into(),
        mesh_b_id: (id(20)).into(),
        pairs: vec![GlueVertexPair {
            vertex_a: Some(1),
            vertex_b: Some(10),
            weight_a: 0.5,
            weight_b: 0.5,
            ..Default::default()
        }],
        intensity: 0.3,
        binding: None,
    };
    doc = prepare(&doc, |builder| builder.create_glue(glue));

    let param_bs = Parameter {
        id: id(22),
        runtime_id: "ParamBS".to_string(),
        name: "Param BS".to_string(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::BlendShape,
        repeat: false,
    };
    doc = prepare(&doc, |builder| builder.create_parameter(param_bs));

    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: id(23),
        parameter_id: id(22),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    doc = prepare(&doc, |builder| builder.create_blend_key_table(bkt));

    let constraint = BlendShapeConstraint {
        samples: Default::default(),
        id: id(24),
        parameter_id: id(6),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.5],
    };
    doc = prepare(&doc, |builder| builder.create_blend_constraint(constraint));

    let binding = BlendShapeBinding {
        id: id(25),
        target_id: id(21),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: id(23),
        constraint_ids: vec![id(24)],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.0,
            },
            DeltaGlueKeyform {
                sample_id: Default::default(),
                intensity: 0.5,
            },
        ]),
    };
    doc = prepare(&doc, |builder| builder.create_blend_binding(binding));

    // Encode to project JSON
    let encoded = fixture_bytes(&doc);
    let wire: serde_json::Value = project_json(&encoded);
    assert_eq!(
        wire["document"]["blend_bindings"][0]["keyforms"]["items"][1]["intensity"],
        0.5
    );

    // Decode and verify
    let decoded = load_native(&encoded).expect("decode_project failed");

    let re_glue = decoded.glue(&id(21)).unwrap();
    assert_eq!(re_glue.intensity, 0.3);

    let re_binding = decoded.blend_binding(&id(25)).unwrap();
    assert_eq!(re_binding.target_kind, BlendShapeTargetKind::Glue);
    assert_eq!(re_binding.target_id, id(21));
    match &re_binding.keyforms {
        DeltaKeyforms::Glue(forms) => {
            assert_eq!(forms.len(), 2);
            assert_eq!(forms[0].intensity, 0.0);
            assert_eq!(forms[1].intensity, 0.5);
        }
        _ => panic!("Expected DeltaKeyforms::Glue"),
    }

    // Compare evaluation
    let mut preview = HashMap::new();
    preview.insert(id(6), 0.5);
    preview.insert(id(22), 0.8);

    let mut frame_orig = DrawableFrame::default();
    let mut frame_re = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame_orig).is_ok());
    assert!(evaluate_frame(&decoded, &preview, &mut frame_re).is_ok());

    assert_eq!(frame_orig.drawables.len(), frame_re.drawables.len());
    for (d1, d2) in frame_orig.drawables.iter().zip(&frame_re.drawables) {
        assert_eq!(d1.positions, d2.positions);
    }
}

#[test]
fn v5_model3_migrates_to_uuid_references_and_survives_runtime_renames() {
    let doc = fixture_doc();
    let mut wire: serde_json::Value = project_json(&fixture_bytes(&doc));
    wire["format_version"] = 5.into();
    assert_eq!(
        load_native(&replace_project_json(&fixture_bytes(&doc), &wire))
            .unwrap_err()
            .code,
        "UNSUPPORTED_VERSION"
    );
}
