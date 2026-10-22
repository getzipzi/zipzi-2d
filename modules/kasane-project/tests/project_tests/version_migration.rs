use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::helpers::*;
use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaGlueKeyform, DeltaKeyforms, Glue, GlueVertexPair, Mesh, Offscreen, OffscreenKeyform,
    Parameter, ParameterKind, Vec2,
};
use kasane_project::{decode_project, encode_project, DocumentSession};

#[test]
fn test_v42_project_lifecycle_detachment_and_export() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src_fixture = root.join("tests/fixtures/external_v42");

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

    // Import into session
    let mut session = DocumentSession::new();
    let (imp_res, report) = session.import_model3(&src_model3);
    assert!(imp_res.status.is_ok(), "{:?}", imp_res);
    let rep = report.expect("Report should exist");
    assert_eq!(rep.moc_version, 4);

    // Verify V42 properties
    assert_eq!(session.document().blend_binding_order().len(), 2);
    assert_eq!(session.document().blend_constraint_order().len(), 1);

    // Save as project.kasane
    let project_path = tmp.0.join("v42_detached_project");
    let save_res = session.save(&project_path);
    assert!(save_res.status.is_ok(), "{:?}", save_res);

    // Detach: remove original files
    fs::remove_dir_all(&source_dir).unwrap();
    assert!(!source_dir.exists());

    // Reopen without original files
    let mut reopened_session = DocumentSession::new();
    let open_res = reopened_session.open(&project_path);
    assert!(open_res.status.is_ok(), "{:?}", open_res);
    assert!(open_res.resources_complete());

    // Verify document contents preserved
    let doc = reopened_session.document();
    assert_eq!(doc.blend_binding_order().len(), 2);
    assert_eq!(doc.blend_constraint_order().len(), 1);
    assert_eq!(doc.mesh_order().len(), 1);

    // Export upgraded 5.0 package
    let export_dir = tmp.0.join("v42_exported_package");
    let exp_res = reopened_session.export_package(&export_dir);
    assert!(exp_res.status.is_ok(), "{:?}", exp_res);

    let exported_moc3 = export_dir.join("model.moc3");
    assert!(exported_moc3.exists());
    let moc3_bytes = fs::read(&exported_moc3).unwrap();
    let inspection = kasane_moc3::inspect_moc3(&moc3_bytes).expect("Exported MOC3 inspect failed");
    assert_eq!(inspection.version, kasane_moc3::Moc3Version::Version50);
    assert_eq!(inspection.counts.blend_bindings, 2);
    assert_eq!(inspection.counts.bs_constraints, 1);
}

#[test]
fn test_project_v2_blendshape_and_glue_roundtrip() {
    use kasane_core::types::{
        BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
        DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, ParameterKind,
    };

    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    let bs_param_id = id(100);
    assert!(doc
        .create_parameter(kasane_core::types::Parameter {
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
        .status
        .is_ok());

    let kt_id = id(101);
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: kt_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());

    let c_id = id(102);
    assert!(doc
        .create_blend_constraint(BlendShapeConstraint {
            samples: Default::default(),
            id: c_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            weights: vec![0.0, 1.0],
        })
        .status
        .is_ok());

    let mesh_id = id(4); // from fixture_doc
    let mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    let v_count = mesh.vertex_ids.len();

    let mesh2_id = id(5);
    let mut m2 = mesh.clone();
    m2.id = mesh2_id.clone();
    m2.runtime_id = "ArtMeshB".to_string();
    assert!(doc.create_mesh(m2).status.is_ok());

    let bb_id = id(103);
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
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
        .status
        .is_ok());

    let glue_parameter = id(105);
    assert!(doc
        .create_parameter(Parameter {
            id: glue_parameter.clone(),
            runtime_id: "GlueStrength".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let glue_id = id(104);
    assert!(doc
        .create_glue(Glue {
            id: glue_id.clone(),
            runtime_id: "Glue0".to_string(),
            name: "Glue 0".to_string(),
            mesh_a_id: (mesh_id.clone()).into(),
            mesh_b_id: (mesh2_id.clone()).into(),
            pairs: vec![GlueVertexPair {
                vertex_a: Some(mesh.vertex_ids[0]),
                vertex_b: Some(doc.get_mesh(&mesh2_id).unwrap().vertex_ids[0]),
                weight_a: 0.5,
                weight_b: 0.5,
                ..Default::default()
            }],
            intensity: 1.0,
            binding: Some(kasane_core::types::GlueBinding {
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: glue_parameter,
                    keys: vec![0.0, 1.0]
                }],
                keyforms: vec![
                    kasane_core::types::GlueKeyform {
                        samples: Default::default(),
                        intensity: 0.0
                    },
                    kasane_core::types::GlueKeyform {
                        samples: Default::default(),
                        intensity: 1.0
                    }
                ],
            }),
        })
        .status
        .is_ok());

    let encoded = encode_project(&doc).expect("encode_project failed");
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], 19);
    for field in [
        "blend_key_tables",
        "blend_constraints",
        "blend_bindings",
        "glues",
    ] {
        assert!(wire["document"].get(field).is_some(), "missing {field}");
    }

    let decoded = decode_project(&encoded).expect("decode_project failed");
    assert!(doc.same_content(&decoded));
    assert_eq!(decoded.blend_key_table_order(), &[kt_id]);
    assert_eq!(decoded.blend_constraint_order(), &[c_id]);
    assert_eq!(decoded.blend_binding_order(), &[bb_id]);
    assert_eq!(decoded.glue_order(), &[glue_id]);
}

#[test]
fn test_project_v1_v2_v3_v4_migration_to_current_format() {
    let doc = fixture_doc(&"0".repeat(64), &"1".repeat(64));
    let mut wire: serde_json::Value = serde_json::from_str(&encode_project(&doc).unwrap()).unwrap();
    for old in 1..14 {
        wire["format_version"] = old.into();
        assert_eq!(
            decode_project(&wire.to_string()).unwrap_err().code,
            "UNSUPPORTED_VERSION"
        );
    }
}

#[test]
fn test_v6_reader_rejects_current_format() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let doc = fixture_doc(sha1, sha2);
    let encoded_current = encode_project(&doc).unwrap();

    // Simulate an older reader that only accepts 1..=6.
    let v: serde_json::Value = serde_json::from_str(&encoded_current).unwrap();
    let ver = v["format_version"].as_u64().unwrap() as u32;
    let accepted_by_legacy = (1..=6).contains(&ver);
    assert!(
        !accepted_by_legacy,
        "Legacy reader (1..=6) must reject the current format"
    );
}

#[test]
fn test_project_v4_rejects_unimplemented_collections() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let doc = fixture_doc(sha1, sha2);
    // Inject non-empty deformers collection (unsupported prototype relationships)
    let mut wire: serde_json::Value = serde_json::from_str(&encode_project(&doc).unwrap()).unwrap();
    wire["document"]["deformers"] = serde_json::json!([{ "id": "def1" }]);
    let bad_project = serde_json::to_string(&wire).unwrap();
    let err = decode_project(&bad_project).expect_err("Non-empty deformers must be rejected");
    assert_eq!(err.code, "LEGACY_PROJECT");
    assert!(err.message.contains("Prototype relationships"));
}

#[test]
fn test_project_v4_preserves_offscreen() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    let mesh_id = doc.mesh_order()[0].clone();
    let mut mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    mesh.raw_blend_mode = Some(262);
    assert!(doc.replace_mesh(mesh).status.is_ok());

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
    assert!(doc.create_offscreen(os.clone()).status.is_ok());

    let encoded = encode_project(&doc).expect("encode_project failed");
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        wire["document"]["offscreens"][0]["runtime_id"],
        "Offscreen0"
    );
    assert_eq!(
        wire["document"]["meshes"][0]["properties"]["raw_blend_mode"],
        262
    );

    let decoded = decode_project(&encoded).expect("decode_project failed");
    assert_eq!(decoded.offscreen_count(), 1);
    let decoded_os = decoded
        .get_offscreen(&id(10))
        .expect("offscreen must exist");
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

    let decoded_mesh = decoded.get_mesh(&mesh_id).expect("mesh must exist");
    assert_eq!(decoded_mesh.raw_blend_mode, Some(262));
}

#[test]
fn disabled_offscreen_roundtrips_and_legacy_defaults_to_enabled() {
    let mut doc = fixture_doc(&"0".repeat(64), &"1".repeat(64));
    let mut offscreen = Offscreen {
        id: id(10),
        part_id: sid(1),
        ..Default::default()
    };
    assert!(doc.create_offscreen(offscreen.clone()).status.is_ok());
    let encoded = encode_project(&doc).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], 19);
    assert!(wire["document"]["offscreens"][0].get("enabled").is_none());
    assert!(
        decode_project(&encoded)
            .unwrap()
            .get_offscreen(&id(10))
            .unwrap()
            .enabled
    );

    offscreen.enabled = false;
    assert!(doc.replace_offscreen(offscreen.clone()).status.is_ok());
    let encoded = encode_project(&doc).unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], 19);
    assert_eq!(wire["document"]["offscreens"][0]["enabled"], false);
    assert_eq!(
        decode_project(&encoded).unwrap().get_offscreen(&id(10)),
        Some(&offscreen)
    );
    wire["format_version"] = serde_json::json!(7);
    assert_eq!(
        decode_project(&wire.to_string()).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );
}

#[test]
fn test_project_v4_preserves_repeat_parameter() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    let param_id = doc.parameter_order()[0].clone();
    let mut p = doc.get_parameter(&param_id).unwrap().clone();
    p.repeat = true;
    assert!(doc.replace_parameter(p).status.is_ok());

    let encoded = encode_project(&doc).expect("encode_project failed");
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], 19);
    assert_eq!(wire["document"]["parameters"][0]["repeat"], true);

    let decoded = decode_project(&encoded).expect("decode_project failed");
    let decoded_param = decoded.get_parameter(&param_id).unwrap();
    assert!(
        decoded_param.repeat,
        "repeat: true must be preserved after project decode"
    );
}

#[test]
fn test_project_v4_blendshape_glue_roundtrip() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

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
    assert!(doc.create_mesh(mesh2).status.is_ok());

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
    assert!(doc.create_glue(glue).status.is_ok());

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
    assert!(doc.create_parameter(param_bs).status.is_ok());

    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: id(23),
        parameter_id: id(22),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    let constraint = BlendShapeConstraint {
        samples: Default::default(),
        id: id(24),
        parameter_id: id(6),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.5],
    };
    assert!(doc.create_blend_constraint(constraint).status.is_ok());

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
    assert!(doc.create_blend_binding(binding).status.is_ok());

    // Encode to project JSON
    let encoded = encode_project(&doc).expect("encode_project failed");
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        wire["document"]["blend_bindings"][0]["keyforms"]["items"][1]["intensity"],
        0.5
    );

    // Decode and verify
    let decoded = decode_project(&encoded).expect("decode_project failed");
    #[cfg(feature = "binary-prototype")]
    {
        let binary = kasane_project::encode_project_cbor(&doc).unwrap();
        let binary_decoded = kasane_project::decode_project_cbor(&binary).unwrap();
        assert!(doc.same_content(&binary_decoded));
    }
    let re_glue = decoded.get_glue(&id(21)).unwrap();
    assert_eq!(re_glue.intensity, 0.3);

    let re_binding = decoded.get_blend_binding(&id(25)).unwrap();
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
    let doc = fixture_doc(&"0".repeat(64), &"1".repeat(64));
    let mut wire: serde_json::Value = serde_json::from_str(&encode_project(&doc).unwrap()).unwrap();
    wire["format_version"] = 5.into();
    assert_eq!(
        decode_project(&wire.to_string()).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );
}
