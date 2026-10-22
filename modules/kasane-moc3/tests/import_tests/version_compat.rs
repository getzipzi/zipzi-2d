use std::collections::HashMap;
use std::fs;

use super::helpers::*;
use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{DeltaKeyforms, ParameterKind};
use kasane_moc3::{
    encode_moc3, encode_moc3_with_version, import_from_bare_moc3, import_from_model3_file,
    inspect_moc3, Moc3ExportVersion, Moc3Version,
};

#[test]
fn test_import_external_v33_3d8e869a() {
    let root = workspace_root();
    let moc_path = root.join("modules/purism-core/testdata/moc3/3d8e869a678a1dac.moc3");
    let bytes = fs::read(&moc_path).expect("Failed to read 3d8e869a moc3");

    // 1. Inspect
    let inspection = inspect_moc3(&bytes).expect("Inspection failed");
    assert_eq!(inspection.version, Moc3Version::Version33);
    assert_eq!(inspection.counts.parts, 10);
    assert_eq!(inspection.counts.deformers, 17);
    assert_eq!(inspection.counts.warps, 15);
    assert_eq!(inspection.counts.rotations, 2);
    assert_eq!(inspection.counts.art_meshes, 29);
    assert_eq!(inspection.counts.parameters, 32);

    // 2. Import
    let mut tex_map = HashMap::new();
    tex_map.insert(
        0,
        root.join("tests/fixtures/gpu/gpu-package/textures/0.png"),
    );
    let res = import_from_bare_moc3(&bytes, &tex_map).expect("Import failed");
    let doc = &res.document;

    assert_eq!(doc.sorted_parts().len(), 10);
    assert_eq!(doc.sorted_transforms().len(), 17);
    assert_eq!(doc.mesh_order().len(), 29);
    assert_eq!(doc.parameter_order().len(), 32);

    let defaults = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap().default_value)
        .collect::<Vec<_>>();
    let mut toggle = defaults.clone();
    let index = doc
        .parameter_order()
        .iter()
        .position(|id| doc.get_parameter(id).unwrap().runtime_id == "MouseToggle")
        .unwrap();
    toggle[index] = 1.0;
    assert_runtime_matches(doc, &bytes, &[defaults, toggle]);

    // 3. Compare with PurismCore running the original model
    let mut purism_orig = PurismModelInstance::new(&bytes);
    purism_orig.update();

    let mut doc_frame = DrawableFrame::default();
    evaluate_frame(doc, &HashMap::new(), &mut doc_frame);

    let ppu = doc.source_canvas().unwrap().pixels_per_unit;

    // 4. Re-export imported Document to v50 MOC3 and verify in PurismCore
    let re_export = encode_moc3(doc).expect("Re-export of v33 model to v50 failed");

    let mut purism_re = PurismModelInstance::new(&re_export.bytes);
    purism_re.update();

    for d in &doc_frame.drawables {
        let p_draw_orig = purism_orig.get_drawable(&d.runtime_id).unwrap();
        let p_draw_re = purism_re.get_drawable(&d.runtime_id);
        assert!(
            p_draw_re.is_some(),
            "Drawable {} missing in re-exported model",
            d.runtime_id
        );
        let pd_re = p_draw_re.unwrap();
        near(pd_re.opacity, p_draw_orig.opacity, ppu);

        for (po, pr) in p_draw_orig.positions.iter().zip(pd_re.positions.iter()) {
            near(pr.x, po.x, ppu);
            near(pr.y, po.y, ppu);
        }
    }
}

#[test]
fn test_import_external_v50() {
    let root = workspace_root();
    let model3_path = root.join("tests/fixtures/external_v50/model.model3.json");

    let res = import_from_model3_file(&model3_path).expect("Import external v50 failed");
    let doc = &res.document;

    // Verify unimported attachments collected
    assert!(
        res.report
            .unimported_attachments
            .iter()
            .any(|s| s.contains("Physics")),
        "Physics attachment should be reported"
    );
    assert!(
        res.report
            .unimported_attachments
            .iter()
            .any(|s| s.contains("Motions")),
        "Motions attachment should be reported"
    );

    // Verify topology
    assert_eq!(doc.parameter_order().len(), 2);
    assert_eq!(doc.sorted_transforms().len(), 2); // 1 Warp, 1 Rotation
    assert_eq!(doc.mesh_order().len(), 1);

    // Verify nested deformer hierarchy: RotChild has parent WarpRoot
    let rot_id = &doc.sorted_transforms()[1];
    let rot = doc.get_transform(rot_id).unwrap();
    assert_eq!(rot.runtime_id, "RotChild");
    let warp_id = &doc.sorted_transforms()[0];
    assert_eq!(rot.parent(), warp_id);

    // Verify explicit v50 multiply and screen colors are preserved
    let mesh = doc.get_mesh(&doc.mesh_order()[0]).unwrap();
    let b = doc.binding_for_mesh(&mesh.id).unwrap();
    assert_eq!(b.keyforms.len(), 6);
    // Keyform 0 color from create_v50_external_fixture: mul_r = 0.5, scr_r = 0.0
    near(b.keyforms[0].appearance.multiply[0], 0.5, 100.0);
    near(b.keyforms[0].appearance.screen[0], 0.0, 100.0);
    // Keyform 5 color: mul_r = 0.5 + 5 * 0.08 = 0.9, scr_r = 0.05 * 5 = 0.25
    near(b.keyforms[5].appearance.multiply[0], 0.9, 100.0);
    near(b.keyforms[5].appearance.screen[0], 0.25, 100.0);

    // Re-export and verify in PurismCore
    let re_export = encode_moc3(doc).expect("Re-export failed");
    let mut purism = PurismModelInstance::new(&re_export.bytes);
    purism.set_parameter("ParamX", 0.5);
    purism.set_parameter("ParamY", -0.5);
    purism.update();
    let d = purism.get_drawable("MeshQuad").expect("Drawable not found");
    assert_eq!(d.positions.len(), 4);
}

#[test]
fn test_import_external_v42() {
    let root = workspace_root();
    let model3_path = root.join("tests/fixtures/external_v42/model.model3.json");
    let moc3_path = root.join("tests/fixtures/external_v42/model.moc3");
    let bytes = fs::read(&moc3_path).expect("Failed to read external v42 moc3");

    // 1. Inspect
    let inspection = inspect_moc3(&bytes).expect("Inspection of v42 must succeed in S2");
    assert_eq!(inspection.version, Moc3Version::Version42);
    assert_eq!(inspection.counts.parts, 1);
    assert_eq!(inspection.counts.warps, 1);
    assert_eq!(inspection.counts.rotations, 1);
    assert_eq!(inspection.counts.art_meshes, 1);
    assert_eq!(inspection.counts.parameters, 3);
    assert_eq!(inspection.counts.keyform_mul_colors, 7);
    assert_eq!(inspection.counts.keyform_scr_colors, 7);
    assert_eq!(inspection.counts.blend_key_tables, 1);
    assert_eq!(inspection.counts.blend_bindings, 2);
    assert_eq!(inspection.counts.bs_warps, 1);
    assert_eq!(inspection.counts.bs_art_meshes, 1);
    assert_eq!(inspection.counts.bs_constraints, 1);

    // 2. Import
    let res = import_from_model3_file(&model3_path).expect("Import external v42 failed");
    let doc = &res.document;

    // Verify elements
    assert_eq!(doc.parameter_order().len(), 3);
    assert_eq!(doc.sorted_transforms().len(), 2);
    assert_eq!(doc.mesh_order().len(), 1);
    assert_eq!(doc.blend_key_table_order().len(), 1);
    assert_eq!(doc.blend_constraint_order().len(), 1);
    assert_eq!(doc.blend_binding_order().len(), 2);

    // Verify ParamBS is BlendShape
    let p_bs_id = doc
        .parameter_order()
        .iter()
        .find(|id| doc.get_parameter(id).unwrap().runtime_id == "ParamBS")
        .expect("ParamBS exists");
    let p_bs = doc.get_parameter(p_bs_id).unwrap();
    assert_eq!(p_bs.kind, ParameterKind::BlendShape);

    // Verify shared constraint
    let bsc_id = &doc.blend_constraint_order()[0];
    let bsc = doc.get_blend_constraint(bsc_id).unwrap();
    assert_eq!(bsc.parameter_id, *p_bs_id);
    assert_eq!(bsc.keys, vec![-1.0, 0.0, 1.0]);
    assert_eq!(bsc.weights, vec![1.0, 0.0, 1.0]);

    // Verify base_key_idx on key table is intermediate (1)
    let bkt_id = &doc.blend_key_table_order()[0];
    let bkt = doc.get_blend_key_table(bkt_id).unwrap();
    assert_eq!(bkt.base_key_idx, 1);
    assert_eq!(bkt.keys, vec![-1.0, 0.0, 1.0]);

    // Verify normal colors imported into document
    let warp_id = &doc.sorted_transforms()[0];
    let warp = doc.get_transform(warp_id).unwrap();
    near(warp.appearance.multiply[0], 0.9, 100.0);
    near(warp.appearance.screen[0], 0.05, 100.0);

    let mesh_id = &doc.mesh_order()[0];
    let binding = doc.binding_for_mesh(mesh_id).unwrap();
    assert_eq!(binding.keyforms.len(), 6);
    near(binding.keyforms[0].appearance.multiply[0], 0.5, 100.0);
    near(binding.keyforms[5].appearance.multiply[0], 0.9, 100.0);

    // 3. Runtime parity against PurismCore (and 5.0 re-export) across parameter space
    let samples: Vec<Vec<f32>> = vec![
        vec![0.0, 0.0, 0.0],
        vec![0.5, -0.5, 1.0],
        vec![-0.5, 0.5, -1.0],
        vec![0.8, 0.3, 0.5],
    ];
    assert_runtime_matches(doc, &bytes, &samples);
}

#[test]
fn test_v42_without_blendshapes() {
    let root = workspace_root();
    let moc3_path = root.join("tests/fixtures/external_v42/model.moc3");
    let mut bytes = fs::read(&moc3_path).expect("Read v42 moc3");

    // Zero out blendshape counts in count_info:
    // counts[25..=31] are blend_key_tables, blend_bindings, bs_warps, bs_art_meshes, bs_constraint_idx, bs_constraints, bs_constraint_vals
    let counts_off = u32::from_le_bytes(bytes[64..68].try_into().unwrap()) as usize;
    for f in 25..=31 {
        bytes[counts_off + f * 4..counts_off + f * 4 + 4].copy_from_slice(&0i32.to_le_bytes());
    }
    // Also clear param_src.blend_key_table_len (section 116) and param_src.type (section 114)
    let sec_114_off =
        u32::from_le_bytes(bytes[64 + 114 * 4..64 + 114 * 4 + 4].try_into().unwrap()) as usize;
    let sec_116_off =
        u32::from_le_bytes(bytes[64 + 116 * 4..64 + 116 * 4 + 4].try_into().unwrap()) as usize;
    for p in 0..3 {
        bytes[sec_114_off + p * 4..sec_114_off + p * 4 + 4].copy_from_slice(&0i32.to_le_bytes());
        bytes[sec_116_off + p * 4..sec_116_off + p * 4 + 4].copy_from_slice(&0i32.to_le_bytes());
    }

    let inspection = inspect_moc3(&bytes).expect("Inspection should succeed for v42 without BS");
    assert_eq!(inspection.counts.blend_bindings, 0);
    assert_eq!(inspection.counts.blend_key_tables, 0);

    let res = import_from_bare_moc3(
        &bytes,
        &HashMap::from([(0, root.join("tests/fixtures/external_v42/texture_00.png"))]),
    )
    .expect("Import without BS should succeed");
    assert_eq!(res.document.blend_binding_order().len(), 0);
    assert_eq!(res.document.blend_key_table_order().len(), 0);

    let re_export = encode_moc3(&res.document).expect("Re-export to v50 should succeed");
    assert!(!re_export.bytes.is_empty());
}

#[test]
fn test_v42_corrupt_color_index_rejected() {
    let root = workspace_root();
    let moc3_path = root.join("tests/fixtures/external_v42/model.moc3");
    let mut bytes = fs::read(&moc3_path).expect("Read v42 moc3");

    // Section 107 is art_mesh_src.key_color_off
    // Corrupt it to point to 999 (outside keyform_mul_colors which is 7)
    let sec_107_off =
        u32::from_le_bytes(bytes[64 + 107 * 4..64 + 107 * 4 + 4].try_into().unwrap()) as usize;
    bytes[sec_107_off..sec_107_off + 4].copy_from_slice(&999i32.to_le_bytes());

    let err = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap_err();
    assert!(
        matches!(
            err.code.as_str(),
            "INVALID_COLOR_REFERENCE" | "FILE_CORRUPT"
        ),
        "Expected color reference error, got: {:?}",
        err
    );
}

#[test]
fn test_v42_does_not_access_v50_fields() {
    let root = workspace_root();
    let moc3_path = root.join("tests/fixtures/external_v42/model.moc3");
    let bytes = fs::read(&moc3_path).expect("Read v42 moc3");

    let inspection = inspect_moc3(&bytes).expect("Inspect v42");
    assert_eq!(inspection.version, Moc3Version::Version42);

    let res = import_from_bare_moc3(&bytes, &HashMap::new()).expect("Import v42");
    for bb_id in res.document.blend_binding_order() {
        let bb = res.document.get_blend_binding(bb_id).unwrap();
        match &bb.keyforms {
            DeltaKeyforms::Warp(forms) => {
                for f in forms {
                    assert!(f.multiply.is_none());
                    assert!(f.screen.is_none());
                }
            }
            DeltaKeyforms::Mesh(forms) => {
                for f in forms {
                    assert!(f.multiply.is_none());
                    assert!(f.screen.is_none());
                }
            }
            _ => {}
        }
    }
}

#[test]
fn test_v6_export_and_preflight() {
    let root = workspace_root();
    let ren_path = root.join("third_party/CubismSdkForNative-5-r.5/Samples/Resources/Ren/Ren.moc3");
    if !ren_path.exists() {
        return;
    }
    let ren_bytes = fs::read(&ren_path).expect("read Ren");
    let ins_ren = kasane_moc3::inspect_moc3(&ren_bytes).expect("Ren inspection");
    assert_eq!(ins_ren.version, Moc3Version::Version53);
    let decoded = import_from_bare_moc3(&ren_bytes,&HashMap::from([(0,root.join("third_party/CubismSdkForNative-5-r.5/Samples/Resources/Ren/Ren.2048/texture_00.png"))])).expect("decode Ren with its actual texture");
    let doc = &decoded.document;

    // 1. Re-export Ren with Auto -> should succeed and export v6
    let exported = kasane_moc3::encode_moc3(doc).expect("encode Ren auto");
    assert_eq!(exported.bytes[4], 6); // Version 6
    let safe_re =
        kasane_moc3::inspect_moc3_safety(&exported.bytes).expect("safety on re-exported Ren");
    assert_eq!(safe_re.version, Moc3Version::Version53);
    assert_eq!(safe_re.counts.offscreens, 24);
    assert_eq!(safe_re.counts.parts, 51);
    assert_eq!(safe_re.counts.art_meshes, 198);

    // 2. Export with Moc3ExportVersion::V50 -> should be rejected with INCOMPATIBLE_EXPORT_VERSION
    let err_v50 = encode_moc3_with_version(doc, Moc3ExportVersion::V50)
        .expect_err("V50 export must fail on document with offscreens");
    assert_eq!(err_v50.code, "INCOMPATIBLE_EXPORT_VERSION");

    // 3. Export with Moc3ExportVersion::V53 -> should succeed
    let exp_v53 =
        encode_moc3_with_version(doc, Moc3ExportVersion::V53).expect("V53 export must succeed");
    assert_eq!(exp_v53.bytes[4], 6);

    // 4. Verify re-exported bytes can be loaded in PurismModelInstance
    let purism = PurismModelInstance::new(&exported.bytes);
    assert_eq!(purism.part_count(), 51);
    assert_eq!(purism.drawable_count(), 198);

    // Render order must include Offscreens in group descendant totals. Missing
    // them creates colliding orders and loses meshes in the official renderer.
    let defaults: Vec<f32> = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap().default_value)
        .collect();
    assert_runtime_matches(doc, &exported.bytes, &[defaults]);

    // 5. Test auto on a document without 5.3 features exports v5
    let v5_source = fs::read(root.join("tests/fixtures/external_v50/model.moc3")).unwrap();
    let v5_doc = import_from_bare_moc3(
        &v5_source,
        &HashMap::from([(0, root.join("tests/fixtures/external_v50/texture_00.png"))]),
    )
    .unwrap()
    .document;
    let v5_exported = kasane_moc3::encode_moc3(&v5_doc).expect("encode v5 doc auto");
    assert_eq!(v5_exported.bytes[4], 5); // Version 5
}
