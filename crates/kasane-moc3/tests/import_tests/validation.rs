use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::helpers::*;
use kasane_moc3::{
    encode_moc3, import_from_bare_moc3, import_from_model3_json, inspect_moc3, Moc3Version,
};

#[test]
fn import_preserves_keys_outside_declared_parameter_range() {
    let mut bytes = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let inspection = inspect_moc3(&bytes).unwrap();
    let maximum_offset = inspection.section_offsets[51] as usize;
    bytes[maximum_offset..maximum_offset + 4].copy_from_slice(&0.5f32.to_le_bytes());

    let imported = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap();
    let param_id = &imported.report.id_mapping.parameter_by_index[0];
    let parameter = imported.document.get_parameter(param_id).unwrap();
    assert_eq!(parameter.maximum, 1.0);
    assert!(imported.document.binding_order().iter().any(|id| {
        imported
            .document
            .get_binding(id)
            .unwrap()
            .axes
            .iter()
            .any(|axis| axis.parameter_id == *param_id && axis.keys.contains(&1.0))
    }));
}

#[test]
fn import_document_accepts_self_mask_from_source() {
    let bytes = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let mut document = import_from_bare_moc3(&bytes, &HashMap::new())
        .unwrap()
        .document;
    let mesh_id = document.mesh_order()[0].clone();
    let mut mesh = document.get_mesh(&mesh_id).unwrap().clone();
    mesh.masks.push(mesh_id.clone());
    assert!(document.replace_mesh(mesh).status.is_ok());
    assert_eq!(document.get_mesh(&mesh_id).unwrap().masks, [mesh_id]);
}

#[test]
fn import_document_accepts_mutual_masks_from_source() {
    let bytes = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let mut document = import_from_bare_moc3(&bytes, &HashMap::new())
        .unwrap()
        .document;
    let first = document.mesh_order()[0].clone();
    let second = id(987);
    let mut second_mesh = document.get_mesh(&first).unwrap().clone();
    second_mesh.id = second.clone();
    second_mesh.runtime_id = "MutualMaskSource".into();
    second_mesh.name = "MutualMaskSource".into();
    second_mesh.masks.clear();
    assert!(document.create_mesh(second_mesh).status.is_ok());
    let mut first_mesh = document.get_mesh(&first).unwrap().clone();
    first_mesh.masks = vec![second.clone()];
    assert!(document.replace_mesh(first_mesh).status.is_ok());
    let mut second_mesh = document.get_mesh(&second).unwrap().clone();
    second_mesh.masks = vec![first];
    assert!(document.replace_mesh(second_mesh).status.is_ok());
}

#[test]
fn import_normalizes_empty_drawing_group_sentinel() {
    let mut bytes = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    import_cases::set_i32(&mut bytes, 82, 0, 2);
    import_cases::set_i32(&mut bytes, 82, 1, 0);
    import_cases::set_i32(&mut bytes, 85, 1, i32::MAX);
    import_cases::set_i32(&mut bytes, 84, 1, -i32::MAX);

    let imported = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap();
    let groups = imported.document.draw_order_groups().unwrap();
    assert!(groups
        .iter()
        .any(|group| { group.items.is_empty() && group.min_order == 0 && group.max_order == 0 }));
}

#[test]
fn test_unsupported_features_rejected() {
    // 1. mao_pro.moc3 has Glues (7) and BlendShapes (34 targets/tables), which are valid in M3B
    let mao_path = local_mao_moc3();
    if mao_path.exists() {
        let bytes = fs::read(&mao_path).expect("Failed to read mao_pro.moc3");
        let report = inspect_moc3(&bytes).expect("mao_pro must pass structural inspection in M3B");
        assert_eq!(report.version, Moc3Version::Version50);
        assert_eq!(report.counts.glues, 7);
        assert_eq!(report.counts.parameters, 128);
        assert_eq!(report.counts.art_meshes, 260);
        assert_eq!(report.counts.bs_glues, 0);
        assert_eq!(report.counts.offscreens, 0);
        assert!(report.unsupported_features.is_empty());
    } else {
        eprintln!("Skipping local Mao structural checks: {mao_path:?} not found");
    }

    // 2. Unknown version rejection
    let bad_ver_bytes = create_m1_fixture_doc();
    let mut bytes = encode_moc3(&bad_ver_bytes).unwrap().bytes;
    bytes[4] = 99; // unknown version
    let err = inspect_moc3(&bytes).expect_err("Version 99 must be rejected");
    assert_eq!(err.code, "UNSUPPORTED_VERSION");

    // 3. Cyclic parameter acceptance in S3
    let doc = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc).unwrap();
    let offsets = &inspect_moc3(&encoded.bytes).unwrap().section_offsets;
    let counts_off = offsets[0] as usize;
    let rep_off = offsets[54] as usize;

    let mut cyclic_bytes = encoded.bytes.clone();
    cyclic_bytes[rep_off] = 1; // set repeat = 1
    let insp = inspect_moc3(&cyclic_bytes).expect("Cyclic parameter must now be accepted in S3");
    assert!(insp.unsupported_features.is_empty());
    let imported = import_from_bare_moc3(&cyclic_bytes, &std::collections::HashMap::new())
        .expect("Decode cyclic model");
    let decoded = &imported.document;
    assert!(
        decoded
            .get_parameter(decoded.parameter_order()[0].as_str())
            .unwrap()
            .repeat
    );

    // 4. Unknown parameter type is rejected as unsupported feature
    let type_off = offsets[114] as usize;
    let mut bad_type_bytes = encoded.bytes.clone();
    bad_type_bytes[type_off..type_off + 4].copy_from_slice(&2i32.to_le_bytes());
    let err = inspect_moc3(&bad_type_bytes).expect_err("Unknown parameter type must be rejected");
    assert_eq!(err.code, "UNSUPPORTED_FEATURE");
    assert!(err.message.contains("unknown type 2"));

    // Fabricated extension counts have no matching data tables. Safety must
    // reject corruption even when the same file advertises unsupported features.
    for (field, count) in [(34, 1), (35, 1)] {
        let mut corrupt = encoded.bytes.clone();
        corrupt[counts_off + field * 4..counts_off + field * 4 + 4]
            .copy_from_slice(&i32::to_le_bytes(count));
        import_cases::set_i32(&mut corrupt, 40, 0, i32::MAX);
        assert_eq!(
            kasane_moc3::inspect_moc3_safety(&corrupt).unwrap_err().code,
            "FILE_CORRUPT"
        );
    }
}

#[test]
fn test_malformed_and_corrupt_files() {
    // 1. Truncated header
    let short = vec![0u8; 32];
    assert_eq!(inspect_moc3(&short).unwrap_err().code, "BUFFER_TOO_SMALL");

    // 2. Invalid magic
    let mut bad_magic = vec![0u8; 100];
    bad_magic[0..4].copy_from_slice(b"NOPE");
    bad_magic[4] = 5;
    assert_eq!(inspect_moc3(&bad_magic).unwrap_err().code, "INVALID_MAGIC");

    // 3. Bad counts offset oob
    let mut bad_offsets = vec![0u8; 1000];
    bad_offsets[0..4].copy_from_slice(b"MOC3");
    bad_offsets[4] = 5;
    bad_offsets[64..68].copy_from_slice(&999999u32.to_le_bytes()); // count_info offset out of bounds
    assert!(inspect_moc3(&bad_offsets).is_err());
}

#[test]
fn test_resource_mapping() {
    let doc = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc).unwrap();

    // Bare moc3 with empty texture map: should succeed with unmapped texture warning
    let empty_map = HashMap::new();
    let res = import_from_bare_moc3(&encoded.bytes, &empty_map).unwrap();
    assert!(!res.textures_complete);
    assert!(res
        .diagnostics
        .iter()
        .any(|d| d.code == "UNMAPPED_TEXTURE_SLOT"));

    // model3.json without textures: reports diagnostic
    let json_str = r#"{
        "Version": 3,
        "FileReferences": {
            "Moc": "model.moc3"
        }
    }"#;
    let res = import_from_model3_json(json_str, Path::new("."));
    assert!(res.is_err() || !res.unwrap().textures_complete);
}

#[test]
fn cyclic_parent_references_are_errors() {
    let source = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    for (section, parent, kind) in [(9, 0, "Part"), (16, 0, "Deformer"), (16, 1, "Deformer")] {
        let mut bytes = source.clone();
        import_cases::set_i32(&mut bytes, section, 0, parent);
        let error = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap_err();
        assert_eq!(error.code, "RELATIONSHIP_CYCLE");
        assert!(error.message.contains(kind));
    }
}

#[test]
fn invalid_color_windows_are_rejected() {
    let mut bytes = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    import_cases::set_i32(&mut bytes, 107, 0, 1); // six keyforms no longer fit
    let error = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "INVALID_COLOR_REFERENCE" | "FILE_CORRUPT"
    ));
}

#[test]
fn safety_inspection_does_not_skip_corruption_for_unsupported_features() {
    let mut bytes = encode_moc3(&create_m1_fixture_doc()).unwrap().bytes;
    import_cases::set_i32(&mut bytes, 54, 0, 1);
    // Corrupt mesh parent reference while a cyclic parameter is present.
    import_cases::set_i32(&mut bytes, 40, 0, i32::MAX);
    assert_eq!(
        kasane_moc3::inspect_moc3_safety(&bytes).unwrap_err().code,
        "FILE_CORRUPT"
    );
}

#[test]
fn repeated_blend_target_groups_are_not_silently_merged() {
    use kasane_core::types::*;
    let mut doc = create_m1_fixture_doc();
    assert!(doc
        .create_parameter(Parameter {
            id: id(420),
            runtime_id: "SharedBlend".into(),
            minimum: 0.0,
            maximum: 1.0,
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
            id: id(421),
            parameter_id: id(420),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    let original = doc.get_mesh(&doc.mesh_order()[0]).unwrap().clone();
    let mut other = original.clone();
    other.id = id(422);
    other.runtime_id = "SecondBlendTarget".into();
    assert!(doc.create_mesh(other.clone()).status.is_ok());
    for (n, mesh) in [original, other].iter().enumerate() {
        assert!(doc
            .create_blend_binding(BlendShapeBinding {
                id: id(423 + n as i32),
                target_id: mesh.id.clone(),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(421),
                constraint_ids: vec![],
                keyforms: DeltaKeyforms::Mesh(vec![
                    DeltaMeshKeyform {
                        positions: vec![Vec2::default(); mesh.vertex_ids.len()],
                        ..Default::default()
                    };
                    2
                ]),
            })
            .status
            .is_ok());
    }
    let mut bytes = encode_moc3(&doc).unwrap().bytes;
    let inspection = inspect_moc3(&bytes).unwrap();
    let offset = inspection.section_offsets[128] as usize;
    let first = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    import_cases::set_i32(&mut bytes, 128, 1, first);
    assert!(inspect_moc3(&bytes).is_ok());
    let error = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap_err();
    assert_eq!(error.code, "UNSUPPORTED_FEATURE");
    assert!(error.message.contains("multiple BlendShape target groups"));
}

#[test]
fn test_decoder_strict_index_validation() {
    let doc = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc).unwrap();
    let inspection = inspect_moc3(&encoded.bytes).unwrap();
    let idx_len_off = inspection.section_offsets[46] as usize; // art_mesh_src.idx_len
    let idx_off_val = inspection.section_offsets[45] as usize; // art_mesh_src.idx_off
    let indices_base = inspection.section_offsets[79] as usize; // idx_src.idx

    // 1. Non-multiple of 3 index length
    let mut bad_len = encoded.bytes.clone();
    bad_len[idx_len_off..idx_len_off + 4].copy_from_slice(&i32::to_le_bytes(4)); // 4 is not divisible by 3
    let err = kasane_moc3::decoder::decode_moc3(&bad_len, &inspection, &[]).unwrap_err();
    assert_eq!(err.code, "INVALID_LENGTH");

    // 2. Out of bounds vertex index (vi >= vc)
    let mut bad_idx = encoded.bytes.clone();
    let first_idx_offset = indices_base
        + (i32::from_le_bytes(bad_idx[idx_off_val..idx_off_val + 4].try_into().unwrap()) as usize)
            * 2;
    bad_idx[first_idx_offset..first_idx_offset + 2].copy_from_slice(&u16::to_le_bytes(9999));
    let err2 = kasane_moc3::decoder::decode_moc3(&bad_idx, &inspection, &[]).unwrap_err();
    assert_eq!(err2.code, "INVALID_INDEX");

    // 3. Repeated vertex within triangle
    let mut dup_idx = encoded.bytes.clone();
    let first_v = dup_idx[first_idx_offset..first_idx_offset + 2].to_vec();
    dup_idx[first_idx_offset + 2..first_idx_offset + 4].copy_from_slice(&first_v); // i1 = i0
    let err3 = kasane_moc3::decoder::decode_moc3(&dup_idx, &inspection, &[]).unwrap_err();
    assert_eq!(err3.code, "REPEATED_VERTEX");
}

#[test]
fn test_s1_layout_safety_and_version_gating() {
    let doc = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc).unwrap();

    // 1. Version 4 MOC3: inspect_moc3_safety succeeds and inspect_moc3 succeeds (enabled in S2)
    let mut v4_bytes = encoded.bytes.clone();
    v4_bytes[4] = 4;
    let safe_v4 = kasane_moc3::inspect_moc3_safety(&v4_bytes).expect("safety should pass");
    assert_eq!(safe_v4.version, Moc3Version::Version42);
    assert!(!safe_v4
        .unsupported_features
        .iter()
        .any(|u| u.category == "version_4_moc42"));
    let ins_v4 = kasane_moc3::inspect_moc3(&v4_bytes).expect("v4 inspection should succeed in S2");
    assert_eq!(ins_v4.version, Moc3Version::Version42);

    // 2. Version 6 MOC3 with 480 offsets header
    let root = workspace_root();
    let ren_path = root.join("third_party/CubismSdkForNative-5-r.5/Samples/Resources/Ren/Ren.moc3");
    if ren_path.exists() {
        let ren_bytes = fs::read(&ren_path).expect("read Ren");
        let safe_ren =
            kasane_moc3::inspect_moc3_safety(&ren_bytes).expect("safety should pass on Ren");
        assert_eq!(safe_ren.version, Moc3Version::Version53);
        assert_eq!(safe_ren.counts.offscreens, 24);
        assert!(
            safe_ren.unsupported_features.is_empty(),
            "Ren unsupported_features must be empty in S5: {:?}",
            safe_ren.unsupported_features
        );
        let ins_ren =
            kasane_moc3::inspect_moc3(&ren_bytes).expect("Ren inspection should succeed in S5");
        assert_eq!(ins_ren.version, Moc3Version::Version53);
        let decoded =
            kasane_moc3::decode_moc3(&ren_bytes, &ins_ren, &[]).expect("decode Ren in S5");
        assert_eq!(decoded.document.offscreen_count(), 24);
        assert_eq!(decoded.document.part_order().len(), 51);
        assert_eq!(decoded.document.mesh_order().len(), 198);
    }

    // 3. Truncated 480-offset table for version 6
    let mut short_v6 = vec![0u8; 1000]; // less than 1984 bytes
    short_v6[0..4].copy_from_slice(b"MOC3");
    short_v6[4] = 6;
    assert_eq!(
        kasane_moc3::inspect_moc3_safety(&short_v6)
            .unwrap_err()
            .code,
        "BUFFER_TOO_SMALL"
    );

    // 4. Misaligned section offset
    let mut misaligned = encoded.bytes.clone();
    let old_off = u32::from_le_bytes(misaligned[64..68].try_into().unwrap());
    misaligned[64..68].copy_from_slice(&(old_off + 1).to_le_bytes()); // add 1 to make it odd
    assert_eq!(
        kasane_moc3::inspect_moc3_safety(&misaligned)
            .unwrap_err()
            .code,
        "FILE_CORRUPT"
    );

    // 5. Unknown version (version 7)
    let mut v7_bytes = encoded.bytes.clone();
    v7_bytes[4] = 7;
    assert_eq!(
        kasane_moc3::inspect_moc3_safety(&v7_bytes)
            .unwrap_err()
            .code,
        "UNSUPPORTED_VERSION"
    );
}

#[test]
fn decoder_rejects_blendshape_references_without_core_guard() {
    // Keep valid section layout and counts, change only semantic references.
    // Calling the decoder directly exercises its own checks, independently of
    // the optional native consistency checker used by inspect_moc3.
    let original =
        fs::read(workspace_root().join("fixtures/external_v50_bs_glue/model.moc3")).unwrap();
    let inspection = inspect_moc3(&original).unwrap();
    for section in [149, 150, 151, 120, 121, 122, 123, 124] {
        let mut bytes = original.clone();
        let offset = inspection.section_offsets[section] as usize;
        bytes[offset..offset + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        let error = kasane_moc3::decode_moc3(&bytes, &inspection, &[]).unwrap_err();
        assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS", "section {section}");
    }
}

#[test]
fn unavailable_image_sources_are_preserved_without_fabricated_dimensions() {
    let artifact = encode_moc3(&create_m1_fixture_doc()).unwrap();
    for (path, code) in [
        (
            workspace_root().join("missing-image-c5.png"),
            "MISSING_TEXTURE",
        ),
        (workspace_root().join("Cargo.toml"), "CORRUPT_TEXTURE"),
    ] {
        let result =
            import_from_bare_moc3(&artifact.bytes, &HashMap::from([(0, path.clone())])).unwrap();
        assert!(!result.textures_complete);
        assert!(result.diagnostics.iter().any(|d| d.code == code));
        let asset = result
            .document
            .get_asset(&result.document.asset_order()[0])
            .unwrap();
        assert!(asset.content.is_none());
        assert_eq!(asset.unresolved_source.as_deref(), path.to_str());
        assert!(encode_moc3(&result.document).is_err());
    }
}
