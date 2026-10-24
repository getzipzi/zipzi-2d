use std::collections::HashMap;
use std::fs;

use super::helpers::*;
use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::types::{Appearance, BlendMode, Mesh, Parameter, TransformKind, Vec2};
use zipzi_moc3::{encode_moc3, import_from_bare_moc3, inspect_moc3, Moc3Version};

#[test]
fn test_import_m1_roundtrip() {
    let doc_orig = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc_orig).expect("Export failed");
    assert!(!encoded.bytes.is_empty());

    // Import back via bare moc3
    let mut tex_map = HashMap::new();
    // Use an existing test texture
    tex_map.insert(
        0,
        workspace_root().join("fixtures/gpu/gpu-package/textures/0.png"),
    );
    let import_res = import_from_bare_moc3(&encoded.bytes, &tex_map).expect("Import failed");
    let doc_imported = &import_res.document;

    // Import initializes model origin/units from the exported finite rectangle.
    let inspection = inspect_moc3(&encoded.bytes).unwrap();
    let canvas = doc_imported.source_canvas().unwrap();
    assert_eq!(canvas.width, inspection.canvas.width);
    assert_eq!(canvas.height, inspection.canvas.height);
    assert_eq!(
        canvas.origin,
        Vec2::new(
            inspection.canvas.origin_x,
            canvas.height - inspection.canvas.origin_y
        )
    );
    assert_eq!(
        doc_imported.export_settings().pixels_per_unit,
        canvas.pixels_per_unit
    );
    assert_eq!(
        canvas.pixels_per_unit,
        doc_orig.source_canvas().unwrap().pixels_per_unit
    );

    // Verify parameters count and values
    assert_eq!(doc_imported.parameter_order().len(), 1);
    let p_imp_id = &doc_imported.parameter_order()[0];
    let p_imp = doc_imported.get_parameter(p_imp_id).unwrap();
    assert_eq!(p_imp.runtime_id, "ParamAngle");
    assert_eq!(p_imp.minimum, -30.0);
    assert_eq!(p_imp.maximum, 30.0);
    assert_eq!(p_imp.default_value, 0.0);

    // Compare evaluation across sample values (-30, -15, 0, 15, 30)
    for &val in &[-30.0, -15.0, 0.0, 15.0, 30.0] {
        let mut orig_frame = DrawableFrame::default();
        let mut imp_frame = DrawableFrame::default();

        let mut p_orig_map = HashMap::new();
        p_orig_map.insert(id(3), val);
        evaluate_frame(&doc_orig, &p_orig_map, &mut orig_frame);

        let mut p_imp_map = HashMap::new();
        p_imp_map.insert(p_imp_id.clone(), val);
        evaluate_frame(doc_imported, &p_imp_map, &mut imp_frame);

        assert_eq!(orig_frame.drawables.len(), imp_frame.drawables.len());
        let d_orig = &orig_frame.drawables[0];
        let d_imp = &imp_frame.drawables[0];

        assert_eq!(d_imp.runtime_id, "ArtMeshQuad");
        near(d_imp.opacity, d_orig.opacity, 100.0);
        near(d_imp.draw_order as f32, d_orig.draw_order as f32, 100.0);

        for (p_i, p_o) in d_imp.positions.iter().zip(d_orig.positions.iter()) {
            near(p_i.x, p_o.x, 100.0);
            near(p_i.y, p_o.y, 100.0);
        }
    }

    // Re-export imported document to MOC3 and verify in PurismCore
    let re_encoded = encode_moc3(doc_imported).expect("Re-export failed");
    let mut purism = PurismModelInstance::new(&re_encoded.bytes);
    purism.set_parameter("ParamAngle", 15.0);
    purism.update();
    let d_purism = purism
        .get_drawable("ArtMeshQuad")
        .expect("Drawable not found");
    assert!(d_purism.positions.len() == 4);
    assert!(d_purism.opacity > 0.0);
}

#[test]
fn test_post_import_editing() {
    let source = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let mut doc = import_from_bare_moc3(&source, &fixture_textures())
        .unwrap()
        .document;
    let mesh_id = doc.mesh_order()[0].clone();
    let before = exported_drawable(&doc);

    let mut binding = doc.binding_for_mesh(&mesh_id).unwrap().clone();
    binding.keyforms[1].positions[0].x += 0.17;
    assert!(doc.replace_binding(binding).status.is_ok());
    let mesh_edited = exported_drawable(&doc);
    assert_ne!(
        mesh_edited.positions, before.positions,
        "Mesh edit must affect runtime geometry"
    );

    let warp_id = doc.sorted_transforms()[0].clone();
    let mut warp = doc.get_transform(&warp_id).unwrap().clone();
    warp.warp_mut().unwrap().points[0].x += 30.0;
    assert!(doc.replace_transform(warp).status.is_ok());
    let warp_edited = exported_drawable(&doc);
    assert_ne!(
        warp_edited.positions, mesh_edited.positions,
        "Warp edit must affect runtime geometry"
    );

    let mut binding = doc.binding_for_mesh(&mesh_id).unwrap().clone();
    for keyform in &mut binding.keyforms {
        keyform.appearance.opacity = 0.42;
        keyform.draw_order = Some(42.0);
    }
    assert!(doc.replace_binding(binding).status.is_ok());
    let mut mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    mesh.blend_mode = BlendMode::Additive;
    assert!(doc.replace_mesh(mesh).status.is_ok());
    let draw_edited = exported_drawable(&doc);
    near(draw_edited.opacity, 0.42, 1.0);
    assert_eq!(draw_edited.draw_order, 42);
    let artifact = encode_moc3(&doc).unwrap();
    let reimported = import_from_bare_moc3(&artifact.bytes, &fixture_textures()).unwrap();
    assert_eq!(
        reimported
            .document
            .get_mesh(&reimported.document.mesh_order()[0])
            .unwrap()
            .blend_mode,
        BlendMode::Additive
    );
    assert_runtime_matches(
        &doc,
        &artifact.bytes,
        &[vec![0.0, 0.0], vec![-1.0, 1.0], vec![0.5, -0.5]],
    );
}

#[test]
fn test_structural_editing() {
    let doc_orig = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc_orig).expect("Export failed");
    let mut tex_map = HashMap::new();
    tex_map.insert(
        0,
        workspace_root().join("fixtures/gpu/gpu-package/textures/0.png"),
    );
    let mut res = import_from_bare_moc3(&encoded.bytes, &tex_map).expect("Import failed");
    let doc = &mut res.document;

    // Add new parameter
    let new_p = Parameter {
        id: id(100),
        runtime_id: "ParamNew".to_string(),
        name: "NewParam".to_string(),
        minimum: 0.0,
        maximum: 10.0,
        default_value: 5.0,
        decimal_places: 2,
        ..Default::default()
    };
    assert!(doc.create_parameter(new_p).status.is_ok());

    // Add new mesh
    let new_m = Mesh {
        id: id(101),
        name: "NewMesh".to_string(),
        runtime_id: "ArtMeshNew".to_string(),
        texture_asset_id: (doc.asset_order()[0].clone()).into(),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(100.0, 100.0),
            Vec2::new(200.0, 100.0),
            Vec2::new(150.0, 200.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    };
    assert!(doc.create_mesh(new_m).status.is_ok());

    // Re-export and verify both meshes exist in PurismCore
    let re_export = encode_moc3(doc).expect("Re-export with new mesh failed");
    let mut purism = PurismModelInstance::new(&re_export.bytes);
    purism.update();

    assert!(purism.get_drawable("ArtMeshQuad").is_some());
    assert!(purism.get_drawable("ArtMeshNew").is_some());
}

#[test]
fn import_regressions_match_document_and_runtime() {
    let source = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let samples: Vec<Vec<f32>> = [-1.0, -0.5, 0.0, 0.5, 1.0]
        .into_iter()
        .flat_map(|x| [-1.0, 0.0, 1.0].into_iter().map(move |y| vec![x, y]))
        .collect();
    for name in ["binding_zero", "color_window", "canvas_y"] {
        let bytes = import_cases::variant(&source, name);
        let imported = import_from_bare_moc3(&bytes, &fixture_textures()).unwrap();
        assert_eq!(imported.document.binding_order().len(), 1, "{name}");
        assert_eq!(
            imported
                .document
                .get_binding(&imported.document.binding_order()[0])
                .unwrap()
                .keyforms
                .len(),
            6
        );
        assert_runtime_matches(&imported.document, &bytes, &samples);
    }
}

#[test]
fn rotation_edit_rebinding_and_deletion_change_export() {
    let bytes = fs::read(workspace_root().join("fixtures/external_v50/model.moc3")).unwrap();
    let mut doc = import_from_bare_moc3(&bytes, &fixture_textures())
        .unwrap()
        .document;
    let rot_id = doc
        .sorted_transforms()
        .into_iter()
        .find(|id| doc.get_transform(id).unwrap().kind() == TransformKind::Rotation)
        .unwrap();
    let mut rot = doc.get_transform(&rot_id).unwrap().clone();
    rot.rotation_mut().unwrap().pose.angle = 23.0;
    rot.rotation_mut().unwrap().pose.scale = 0.7;
    assert!(doc.replace_transform(rot).status.is_ok());
    let changed = encode_moc3(&doc).unwrap();
    let mut before = PurismModelInstance::new(&bytes);
    let mut after = PurismModelInstance::new(&changed.bytes);
    before.update();
    after.update();
    assert_ne!(
        before.get_drawable("MeshQuad").unwrap().positions,
        after.get_drawable("MeshQuad").unwrap().positions
    );
    assert_runtime_matches(&doc, &changed.bytes, &[vec![0.0, 0.0], vec![0.5, 1.0]]);

    let old_param = doc.parameter_order()[0].clone();
    let mut param = doc.get_parameter(&old_param).unwrap().clone();
    param.id = id(200);
    param.runtime_id = "ReboundParameter".into();
    assert!(doc.create_parameter(param).status.is_ok());
    let mesh_id = doc.mesh_order()[0].clone();
    let mut binding = doc.binding_for_mesh(&mesh_id).unwrap().clone();
    binding.axes[0].parameter_id = id(200);
    assert!(doc.replace_binding(binding).status.is_ok());
    assert!(doc.erase_object(&old_param).status.is_ok());
    let rebound = encode_moc3(&doc).unwrap();
    let reimported = import_from_bare_moc3(&rebound.bytes, &fixture_textures())
        .unwrap()
        .document;
    assert_eq!(reimported.parameter_order().len(), 2);
    assert!(reimported.parameter_order().iter().any(|p| reimported
        .get_parameter(p)
        .unwrap()
        .runtime_id
        == "ReboundParameter"));
    let mut runtime = PurismModelInstance::new(&rebound.bytes);
    runtime.set_parameter("ReboundParameter", -1.0);
    runtime.update();
    let left = runtime.get_drawable("MeshQuad").unwrap().positions;
    runtime.set_parameter("ReboundParameter", 1.0);
    runtime.update();
    assert_ne!(left, runtime.get_drawable("MeshQuad").unwrap().positions);

    let mut survivor = doc.get_mesh(&mesh_id).unwrap().clone();
    survivor.id = id(201);
    survivor.runtime_id = "SurvivingMesh".into();
    assert!(doc.create_mesh(survivor).status.is_ok());
    let binding_id = doc.binding_for_mesh(&mesh_id).unwrap().id.clone();
    assert!(doc.erase_object(&binding_id).status.is_ok());
    assert!(doc.erase_object(&mesh_id).status.is_ok());
    let empty = encode_moc3(&doc).unwrap();
    assert_eq!(inspect_moc3(&empty.bytes).unwrap().counts.art_meshes, 1);
    assert!(PurismModelInstance::new(&empty.bytes)
        .get_drawable("MeshQuad")
        .is_none());
}

#[test]
fn test_import_mao_full() {
    let mao_path = local_mao_moc3();
    if !mao_path.exists() {
        eprintln!(
            "Skipping test_import_mao_full: mao_pro.moc3 not found at {:?}",
            mao_path
        );
        return;
    }

    let bytes = std::fs::read(&mao_path).expect("failed to read mao_pro.moc3");
    let decoded = import_from_bare_moc3(&bytes, &model_textures(&mao_path))
        .expect("failed to import mao_pro.moc3");
    let doc = &decoded.document;

    // Verify element counts
    assert_eq!(doc.part_order().len(), 31, "Parts count");
    let warps_count = doc
        .transform_order()
        .iter()
        .filter(|t| doc.get_transform(t).unwrap().kind() == zipzi_core::types::TransformKind::Warp)
        .count();
    let rotations_count = doc
        .transform_order()
        .iter()
        .filter(|t| {
            doc.get_transform(t).unwrap().kind() == zipzi_core::types::TransformKind::Rotation
        })
        .count();
    assert_eq!(warps_count, 116, "Warps count");
    assert_eq!(rotations_count, 59, "Rotations count");
    assert_eq!(doc.mesh_order().len(), 260, "ArtMeshes count");
    assert_eq!(doc.parameter_order().len(), 128, "Parameters count");

    let inspection = zipzi_moc3::inspect_moc3(&bytes).expect("inspect failed");
    println!("Counts: bs_warps={}, bs_rotations={}, bs_parts={}, bs_art_meshes={}, bs_constraints={}, blend_bindings={}, blend_key_tables={}",
        inspection.counts.bs_warps, inspection.counts.bs_rotations, inspection.counts.bs_parts, inspection.counts.bs_art_meshes,
        inspection.counts.bs_constraints, inspection.counts.blend_bindings, inspection.counts.blend_key_tables);
    assert_eq!(
        doc.blend_constraint_order().len(),
        7,
        "BlendShapeConstraint count"
    );
    assert_eq!(
        doc.blend_binding_order().len(),
        124,
        "BlendShapeBinding count"
    );
    assert_eq!(doc.glue_order().len(), 7, "Glue count");

    let total_glue_pairs: usize = doc
        .glue_order()
        .iter()
        .map(|g| doc.get_glue(g).unwrap().pairs.len())
        .sum();
    assert_eq!(total_glue_pairs, 161, "Total Glue pairs count");

    let mut part_bb = 0;
    let mut warp_bb = 0;
    let mut rot_bb = 0;
    let mut mesh_bb = 0;
    let mut glue_bb = 0;
    for b_id in doc.blend_binding_order() {
        let b = doc.get_blend_binding(b_id).unwrap();
        match b.keyforms {
            zipzi_core::types::DeltaKeyforms::Part(_) => part_bb += 1,
            zipzi_core::types::DeltaKeyforms::Warp(_) => warp_bb += 1,
            zipzi_core::types::DeltaKeyforms::Rotation(_) => rot_bb += 1,
            zipzi_core::types::DeltaKeyforms::Mesh(_) => mesh_bb += 1,
            zipzi_core::types::DeltaKeyforms::Glue(_) => glue_bb += 1,
            zipzi_core::types::DeltaKeyforms::Offscreen(_) => {}
        }
    }
    println!(
        "BlendBindings distribution: Part={}, Warp={}, Rotation={}, Mesh={}, Glue={}",
        part_bb, warp_bb, rot_bb, mesh_bb, glue_bb
    );

    // Evaluation against PurismModelInstance
    let mut runtime = PurismModelInstance::new(&bytes);
    runtime.update();

    let mut frame = zipzi_core::evaluation::DrawableFrame::default();
    let preview = HashMap::new();
    assert!(zipzi_core::evaluation::evaluate_frame(doc, &preview, &mut frame).is_ok());

    let mut max_pos_error = 0.0f32;
    let mut checked_meshes = 0;

    for mesh_id in doc.mesh_order() {
        let mesh = doc.get_mesh(mesh_id).unwrap();
        if let Some(purism_drawable) = runtime.get_drawable(&mesh.runtime_id) {
            let doc_drawable = frame
                .drawables
                .iter()
                .find(|d| d.runtime_id == mesh.runtime_id)
                .unwrap();
            assert_eq!(
                doc_drawable.positions.len(),
                purism_drawable.positions.len()
            );
            let mut mesh_err = 0.0f32;
            for (p_doc, p_purism) in doc_drawable
                .positions
                .iter()
                .zip(&purism_drawable.positions)
            {
                let err = ((p_doc.x / doc.source_canvas().unwrap().pixels_per_unit - p_purism.x)
                    .powi(2)
                    + (p_doc.y / doc.source_canvas().unwrap().pixels_per_unit - p_purism.y)
                        .powi(2))
                .sqrt();
                if err > mesh_err {
                    mesh_err = err;
                }
            }

            if mesh_err > 0.05 {
                // error mesh
            }
            if mesh_err > max_pos_error {
                max_pos_error = mesh_err;
            }
            checked_meshes += 1;
        }
    }

    let mut mismatch_count = 0;
    for d in &frame.drawables {
        let p = runtime.get_drawable(&d.runtime_id).unwrap();
        let is_ok = d.positions.iter().zip(&p.positions).all(|(a, b)| {
            ((a.x / doc.source_canvas().unwrap().pixels_per_unit - b.x).powi(2)
                + (a.y / doc.source_canvas().unwrap().pixels_per_unit - b.y).powi(2))
            .sqrt()
                <= 0.05
        });
        if !is_ok {
            mismatch_count += 1;
        }
    }
    let ok_count = checked_meshes - mismatch_count;
    println!(
        "Matching meshes (<= 0.05px): {} / {}",
        ok_count, checked_meshes
    );
    println!("Mismatch count: {}", mismatch_count);

    assert_eq!(checked_meshes, 260, "All meshes checked");
    println!(
        "Max position error against PurismCore on Mao default pose: {} px",
        max_pos_error
    );
    assert!(
        max_pos_error < 0.05,
        "Dual-core numerical parity margin exceeded: max_pos_error={}",
        max_pos_error
    );
}

#[test]
fn test_mao_roundtrip_export_and_detached_reopening() {
    let path = local_mao_moc3();
    if !path.exists() {
        eprintln!(
            "Skipping test_mao_roundtrip_export_and_detached_reopening: mao_pro.moc3 not found at {path:?}"
        );
        return;
    }

    let orig_bytes = std::fs::read(&path).expect("failed to read mao_pro.moc3");
    let orig_res =
        import_from_bare_moc3(&orig_bytes, &model_textures(&path)).expect("initial import failed");
    let mut orig_doc = orig_res.document;
    // The JSON codec expects package-relative asset descriptors. Keep the real
    // imported dimensions/hash, rebasing only the source paths for this fixture.
    for id in orig_doc.asset_order().to_vec() {
        let mut asset = orig_doc.get_asset(&id).unwrap().clone();
        let content = asset.content.as_mut().unwrap();
        content.source = format!(
            "assets/{}",
            std::path::Path::new(&content.source)
                .strip_prefix(path.parent().unwrap())
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert!(orig_doc.replace_asset(asset).status.is_ok());
    }

    // 1. Serialize the complete native authoring document.
    let project_json = zipzi_project::encode_project(&orig_doc).expect("encode_project failed");

    // 2. Load into a fresh, completely detached Document (no orig_bytes reference)
    let detached_doc = zipzi_project::decode_project(&project_json).expect("decode_project failed");

    assert_eq!(detached_doc.mesh_order().len(), 260);
    assert_eq!(detached_doc.parameter_order().len(), 128);
    assert_eq!(detached_doc.part_order().len(), 31);
    assert_eq!(detached_doc.glue_order().len(), 7);
    assert_eq!(detached_doc.blend_binding_order().len(), 124);
    assert_eq!(detached_doc.blend_key_table_order().len(), 33);
    assert_eq!(detached_doc.blend_constraint_order().len(), 7);

    // 3. Export to 5.0 MOC3 binary
    let exported = encode_moc3(&detached_doc).expect("encode_moc3 failed");
    let exp_bytes = exported.bytes;

    // 4. Verify csmHasMocConsistency on exported bytes
    unsafe {
        let mut moc_buf = exp_bytes.clone();
        let consistent = crate::common::purism::csmHasMocConsistency(
            moc_buf.as_mut_ptr() as *mut std::ffi::c_void,
            moc_buf.len() as u32,
        );
        assert_eq!(
            consistent, 1,
            "csmHasMocConsistency failed on exported 5.0 MOC3"
        );
    }

    // 5. Initialize Purism runtime on exported bytes
    let mut exp_runtime = PurismModelInstance::new(&exp_bytes);
    exp_runtime.update();

    let mut orig_runtime = PurismModelInstance::new(&orig_bytes);
    orig_runtime.update();

    // 6. Verify numerical parity between original Mao and re-exported Mao in PurismCore
    let mut max_err = 0.0f32;
    for mesh_id in detached_doc.mesh_order() {
        let mesh = detached_doc.get_mesh(mesh_id).unwrap();
        let orig_d = orig_runtime.get_drawable(&mesh.runtime_id).unwrap();
        let exp_d = exp_runtime.get_drawable(&mesh.runtime_id).unwrap();
        assert_eq!(orig_d.positions.len(), exp_d.positions.len());
        for (po, pe) in orig_d.positions.iter().zip(&exp_d.positions) {
            let err = ((po.x - pe.x).powi(2) + (po.y - pe.y).powi(2)).sqrt();
            if err > max_err {
                max_err = err;
            }
        }
    }
    println!(
        "Max position error between original Mao and exported Mao in PurismCore: {} px",
        max_err
    );
    assert!(
        max_err < 0.05,
        "Dual-core numerical parity margin exceeded on exported MOC3: max_err={}",
        max_err
    );

    // 7. Re-import the exported MOC3 binary into a 3rd Document and verify lossless roundtrip
    let reimported_res = import_from_bare_moc3(&exp_bytes, &model_textures(&path))
        .expect("re-import of exported MOC3 failed");
    let reimported_doc = &reimported_res.document;
    assert_eq!(reimported_doc.mesh_order().len(), 260);
    assert_eq!(reimported_doc.parameter_order().len(), 128);
    assert_eq!(reimported_doc.part_order().len(), 31);
    assert_eq!(reimported_doc.glue_order().len(), 7);
    let total_reimported_glue_pairs: usize = reimported_doc
        .glue_order()
        .iter()
        .map(|g| reimported_doc.get_glue(g).unwrap().pairs.len())
        .sum();
    assert_eq!(total_reimported_glue_pairs, 161);
    assert_eq!(reimported_doc.blend_binding_order().len(), 124);
    assert_eq!(reimported_doc.blend_key_table_order().len(), 33);
    assert_eq!(reimported_doc.blend_constraint_order().len(), 7);
}

#[test]
fn test_import_hiyori_zero_triangle_meshes_and_glue() {
    let root = workspace_root();
    let moc_path =
        root.join("third_party/CubismSdkForNative-5-r.5/Samples/Resources/Hiyori/Hiyori.moc3");
    if !moc_path.exists() {
        eprintln!(
            "Skipping test_import_hiyori_zero_triangle_meshes_and_glue: Hiyori.moc3 not found"
        );
        return;
    }
    let bytes = fs::read(&moc_path).expect("failed to read Hiyori.moc3");
    let inspection = inspect_moc3(&bytes).expect("Hiyori inspection failed");
    assert_eq!(inspection.version, Moc3Version::Version40);
    assert_eq!(inspection.version_number, 3);
    assert_eq!(inspection.counts.art_meshes, 134);
    assert_eq!(inspection.counts.glues, 26);

    let decoded = import_from_bare_moc3(&bytes, &model_textures(&moc_path))
        .expect("failed to import Hiyori.moc3");
    let doc = &decoded.document;

    // Verify the 4 zero-triangle meshes exist in Document
    let zero_triangle_rids = ["ArtMesh116", "ArtMesh123", "ArtMesh130", "ArtMesh137"];
    let mut zero_mesh_ids = Vec::new();
    for rid in &zero_triangle_rids {
        let m_id = decoded
            .report
            .id_mapping
            .meshes
            .get(*rid)
            .expect("mesh mapping found");
        let mesh = doc.get_mesh(m_id).expect("mesh exists in document");
        assert!(
            mesh.triangles.is_empty(),
            "Mesh {} must have zero triangles",
            rid
        );
        assert!(
            !mesh.vertex_ids.is_empty(),
            "Mesh {} must retain its vertices",
            rid
        );
        zero_mesh_ids.push(m_id.clone());
    }

    // Verify Glues 7, 13, 19, 25 connect these zero-triangle meshes
    let glues = doc.glue_order();
    assert_eq!(glues.len(), 26);
    let mut zero_mesh_glues = 0;
    for g_id in glues {
        let g = doc.get_glue(g_id).unwrap();
        if zero_mesh_ids
            .iter()
            .any(|id| g.mesh_a_id == *id || g.mesh_b_id == *id)
        {
            zero_mesh_glues += 1;
            assert!(
                !g.pairs.is_empty(),
                "Glue with zero-triangle endpoint must have vertex pairs"
            );
        }
    }
    assert_eq!(
        zero_mesh_glues, 4,
        "All 4 zero-triangle meshes must be bound to glues"
    );

    // Evaluate against PurismCore
    let mut runtime = PurismModelInstance::new(&bytes);
    runtime.update();

    let mut frame = DrawableFrame::default();
    let preview = HashMap::new();
    assert!(evaluate_frame(doc, &preview, &mut frame).is_ok());

    // Compare positions for all non-empty meshes
    let mut max_pos_error = 0.0f32;
    for d in &frame.drawables {
        if let Some(core_d) = runtime.get_drawable(&d.runtime_id) {
            assert_eq!(d.positions.len(), core_d.positions.len());
            for p in 0..d.positions.len() {
                let dx = (d.positions[p].x / doc.source_canvas().unwrap().pixels_per_unit
                    - core_d.positions[p].x)
                    .abs();
                let dy = (d.positions[p].y / doc.source_canvas().unwrap().pixels_per_unit
                    - core_d.positions[p].y)
                    .abs();
                max_pos_error = max_pos_error.max(dx).max(dy);
            }
        }
    }
    assert!(
        max_pos_error < 1e-3,
        "Hiyori max position error {max_pos_error} too high"
    );

    // Export to MOC3 v5 and verify roundtrip
    let encoded = encode_moc3(doc).expect("failed to encode Hiyori");
    let reimport = import_from_bare_moc3(&encoded.bytes, &model_textures(&moc_path))
        .expect("failed to reimport Hiyori");
    assert_eq!(reimport.document.mesh_order().len(), 134);
    assert_eq!(reimport.document.glue_order().len(), 26);

    // Verify zero-triangle meshes are still zero-triangle in reimported doc
    for rid in &zero_triangle_rids {
        let m_id = reimport
            .report
            .id_mapping
            .meshes
            .get(*rid)
            .expect("mesh mapping in reimport");
        let mesh = reimport.document.get_mesh(m_id).expect("mesh in reimport");
        assert!(mesh.triangles.is_empty());
    }
}

#[test]
fn test_import_rice_and_mark_v3() {
    let root = workspace_root();
    for (name, rel_path, expected_meshes, expected_glues) in [
        (
            "Rice",
            "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Rice/Rice.moc3",
            178,
            33,
        ),
        (
            "Mark",
            "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Mark/Mark.moc3",
            30,
            0,
        ),
    ] {
        let moc_path = root.join(rel_path);
        if !moc_path.exists() {
            eprintln!("Skipping {name}: not found at {:?}", moc_path);
            continue;
        }
        let bytes = fs::read(&moc_path).expect("failed to read moc3");
        let inspection = inspect_moc3(&bytes).expect("inspect failed");
        assert_eq!(inspection.version, Moc3Version::Version40);
        assert_eq!(inspection.version_number, 3);
        assert_eq!(inspection.counts.art_meshes as usize, expected_meshes);
        assert_eq!(inspection.counts.glues as usize, expected_glues);

        let decoded =
            import_from_bare_moc3(&bytes, &model_textures(&moc_path)).expect("import failed");
        let doc = &decoded.document;
        assert_eq!(doc.mesh_order().len(), expected_meshes);
        assert_eq!(doc.glue_order().len(), expected_glues);

        // Evaluate against PurismCore
        let mut runtime = PurismModelInstance::new(&bytes);
        runtime.update();

        let mut frame = DrawableFrame::default();
        let preview = HashMap::new();
        assert!(evaluate_frame(doc, &preview, &mut frame).is_ok());

        let mut max_pos_error = 0.0f32;
        for d in &frame.drawables {
            if let Some(core_d) = runtime.get_drawable(&d.runtime_id) {
                assert_eq!(d.positions.len(), core_d.positions.len());
                for p in 0..d.positions.len() {
                    let dx = (d.positions[p].x / doc.source_canvas().unwrap().pixels_per_unit
                        - core_d.positions[p].x)
                        .abs();
                    let dy = (d.positions[p].y / doc.source_canvas().unwrap().pixels_per_unit
                        - core_d.positions[p].y)
                        .abs();
                    max_pos_error = max_pos_error.max(dx).max(dy);
                }
            }
        }
        assert!(
            max_pos_error < 1e-3,
            "{name} max position error {max_pos_error} too high"
        );

        // Encode and roundtrip
        let encoded = encode_moc3(doc).expect("encode failed");
        let reimport = import_from_bare_moc3(&encoded.bytes, &model_textures(&moc_path))
            .expect("reimport failed");
        assert_eq!(reimport.document.mesh_order().len(), expected_meshes);
        assert_eq!(reimport.document.glue_order().len(), expected_glues);
    }
}

#[test]
fn test_zero_triangle_mesh_editing_and_lifecycle() {
    let mut doc = create_m1_fixture_doc();
    let mesh_id = id(501);
    let vid1 = 1;
    let vid2 = 2;
    let vid3 = 3;
    let base_mesh = Mesh {
        id: mesh_id.clone(),
        runtime_id: "ZeroTriMesh".into(),
        name: "ZeroTriMesh".into(),
        part_id: String::new(),
        deformer_id: id(4),
        texture_asset_id: doc
            .get_mesh(&doc.mesh_order()[0])
            .unwrap()
            .texture_asset_id
            .clone(),
        vertex_ids: vec![vid1, vid2, vid3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ],
        triangles: vec![], // Zero triangles!
        appearance: Appearance::default(),
        draw_order: Some(1.0),
        blend_mode: BlendMode::Normal,
        enabled: true,
        double_sided: false,
        inverted_mask: false,
        masks: vec![],
        raw_blend_mode: None,
    };
    assert!(doc.create_mesh(base_mesh.clone()).status.is_ok());

    // Evaluate frame with zero-triangle mesh present
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    let z_drawable = frame
        .drawables
        .iter()
        .find(|d| d.id == mesh_id)
        .expect("drawable exists");
    assert!(z_drawable.indices.is_empty());
    assert_eq!(z_drawable.positions.len(), 3);

    // Edit 1: Add a triangle (zero-triangle -> renderable)
    let mut with_triangle = base_mesh.clone();
    with_triangle.triangles = vec![[vid1, vid2, vid3]];
    assert!(doc.replace_mesh(with_triangle).status.is_ok());

    let mut frame2 = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame2).is_ok());
    let r_drawable = frame2
        .drawables
        .iter()
        .find(|d| d.id == mesh_id)
        .expect("drawable exists");
    assert_eq!(r_drawable.indices.len(), 3);

    // Edit 2: Remove triangle again (renderable -> zero-triangle)
    let mut without_triangle = base_mesh.clone();
    without_triangle.triangles = vec![];
    assert!(doc.replace_mesh(without_triangle).status.is_ok());

    let mut frame3 = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame3).is_ok());
    let z2_drawable = frame3
        .drawables
        .iter()
        .find(|d| d.id == mesh_id)
        .expect("drawable exists");
    assert!(z2_drawable.indices.is_empty());

    // Encode to MOC3 and verify zero triangles survive serialization
    let encoded = encode_moc3(&doc).expect("encode failed");
    let reimport =
        import_from_bare_moc3(&encoded.bytes, &fixture_textures()).expect("reimport failed");
    let re_mesh = reimport
        .document
        .get_mesh(&reimport.report.id_mapping.meshes["ZeroTriMesh"])
        .unwrap();
    assert!(re_mesh.triangles.is_empty());
    assert_eq!(re_mesh.vertex_ids.len(), 3);
}

#[test]
fn alternate_file_axis_normalizes_nested_rotation_and_warp_spaces() {
    use zipzi_core::{
        Canvas, Document, ImageAsset, PreciseVec2, RotationPose, RotationTransform, Transform,
        TransformData, WarpTransform,
    };
    for root_warp in [false, true] {
        let mut doc = Document::new();
        assert!(doc
            .initialize(
                id(800),
                Canvas::new(800., 600., Vec2::new(211., -77.), 37.5)
            )
            .is_ok());
        assert!(doc
            .add_asset(ImageAsset {
                unresolved_source: None,
                id: id(801),
                name: "texture".into(),
                content: Some(zipzi_core::ImageContent {
                    source: "textures/0.png".into(),
                    sha256: "0".repeat(64),
                    width: 8,
                    height: 8,
                }),
            })
            .status
            .is_ok());
        let mut parent = String::new();
        for depth in 0..4 {
            let transform_id = id(810 + depth);
            let warp = (depth % 2 == 0) == root_warp;
            let data = if warp {
                TransformData::Warp(WarpTransform {
                    rows: 1,
                    columns: 1,
                    quad: true,
                    bezier: None,
                    points: vec![
                        Vec2::new(-30., -20.),
                        Vec2::new(80., -10.),
                        Vec2::new(-15., 90.),
                        Vec2::new(100., 110.),
                    ],
                })
            } else {
                let in_warp = doc
                    .get_transform(&parent)
                    .is_some_and(|t| t.warp().is_some());
                TransformData::Rotation(RotationTransform {
                    base_angle: 17.,
                    pose: RotationPose {
                        origin: if in_warp {
                            PreciseVec2::new(0.4, 0.55)
                        } else {
                            PreciseVec2::new(11., 23.)
                        },
                        angle: -29.,
                        scale: 0.8,
                        reflect_x: depth == 0,
                        ..Default::default()
                    },
                })
            };
            assert!(doc
                .create_transform(Transform {
                    id: transform_id.clone(),
                    runtime_id: format!("Transform{depth}"),
                    parent_id: zipzi_core::TransformId::optional(parent),
                    data,
                    ..Default::default()
                })
                .status
                .is_ok());
            let positions = if warp {
                vec![
                    Vec2::new(0.2, 0.3),
                    Vec2::new(0.7, 0.3),
                    Vec2::new(0.2, 0.8),
                ]
            } else {
                vec![Vec2::new(-4., -3.), Vec2::new(8., -3.), Vec2::new(-4., 9.)]
            };
            assert!(doc
                .create_mesh(Mesh {
                    id: id(820 + depth),
                    runtime_id: format!("Mesh{depth}"),
                    texture_asset_id: (id(801)).into(),
                    deformer_id: transform_id.clone(),
                    vertex_ids: vec![1, 2, 3],
                    base_positions: positions,
                    uvs: vec![
                        Vec2::new(0.1, 0.2),
                        Vec2::new(0.9, 0.2),
                        Vec2::new(0.1, 0.8)
                    ],
                    triangles: vec![[1, 2, 3]],
                    ..Default::default()
                })
                .status
                .is_ok());
            parent = transform_id;
        }
        let bytes = encode_moc3(&doc).unwrap().bytes;
        for bytes in [bytes.clone(), import_cases::variant(&bytes, "canvas_y")] {
            let imported = import_from_bare_moc3(&bytes, &fixture_textures()).unwrap();
            assert_runtime_matches(&imported.document, &bytes, &[vec![]]);
        }
    }
}
