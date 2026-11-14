use std::fs;
use zipzi_core::{state::*, *};
use zipzi_project::*;
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn model() -> (DocumentState, ResourceView) {
    let resources = ResourceStore::default();
    let image = resources.prepare_rgba(8, 8, vec![255; 256].into()).unwrap();
    let mut b = DocumentState::new(
        id(1),
        Canvas::new(64., 64., Vec2::new(32., 32.), 10.).into(),
    )
    .unwrap()
    .builder();
    b.create_asset(AssetRecord {
        id: id(2),
        name: "art".into(),
        content: AssetContent::Image {
            resource: image.key(),
            width: 8,
            height: 8,
        },
    })
    .unwrap();
    b.create_mesh(Mesh {
        id: id(3),
        runtime_id: "ArtMesh".into(),
        name: "Art".into(),
        texture_asset_id: id(2).into(),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![Vec2::new(0., 0.), Vec2::new(8., 0.), Vec2::new(0., 8.)],
        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    })
    .unwrap();
    (
        b.freeze().unwrap().state,
        ResourceView::default().with_images([image]),
    )
}
fn frame(s: &DocumentState) -> DrawableFrame {
    let mut f = DrawableFrame::default();
    assert!(evaluate_frame(s, &Default::default(), &mut f).is_ok());
    f
}
fn captured(input: &ImportedProject) {
    input.resources.for_state(&input.document).unwrap();
    assert!(!input.resources.is_empty());
    assert_eq!(frame(&input.document).drawables.len(), 1);
    for id in input.document.asset_order() {
        let a = input.document.asset(id).unwrap();
        if matches!(a.content, AssetContent::Image { .. }) {
            let data = input.resources.read_asset(a).unwrap();
            assert!(data.rgba.iter().any(|v| *v != 0));
        }
    }
}
#[test]
fn runtime_package_import_captures_textures_and_preserves_geometry_after_sources_disappear() {
    let (state, resources) = model();
    let initial = state.content_id();
    let root = tempfile::tempdir().unwrap();
    let plan = build_export_plan(&state, &resources).unwrap();
    for (path, file) in plan.files() {
        let target = root.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, file.bytes()).unwrap();
    }
    let entry = plan
        .files()
        .keys()
        .find(|p| p.ends_with(".model3.json"))
        .unwrap();
    let (result, imported, _) = ProjectImporter::default().import_model3(&root.path().join(entry));
    assert!(result.status.is_ok(), "{result:?}");
    let imported = imported.unwrap();
    captured(&imported);
    let before = frame(&state);
    let after = frame(&imported.document);
    assert_eq!(
        before.drawables[0].positions.len(),
        after.drawables[0].positions.len()
    );
    for (a, b) in before.drawables[0]
        .positions
        .iter()
        .zip(&after.drawables[0].positions)
    {
        assert!(
            (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4,
            "{a:?} {b:?}"
        );
    }
    assert_eq!(state.content_id(), initial);
    assert!(imported
        .protected_paths
        .contains(&root.path().join(entry).canonicalize().unwrap()));
    drop(root);
    captured(&imported);
}
#[test]
fn cubism_import_returns_owned_resource_values_and_roundtrips_model_positions() {
    let (state, resources) = model();
    let artifact = encode_document_cmo3(
        &state,
        &resources,
        &Cmo3Options {
            loss_policy: LossPolicy::Allow,
            ..Default::default()
        },
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("model.cmo3");
    fs::write(&path, artifact.bytes).unwrap();
    let imported = ProjectImporter::default()
        .import_cubism_authoring(&path)
        .unwrap();
    captured(&imported);
    drop(root);
    captured(&imported);
    let source = frame(&state);
    let result = frame(&imported.document);
    for (a, b) in source.drawables[0]
        .positions
        .iter()
        .zip(&result.drawables[0].positions)
    {
        assert!(
            (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
            "{a:?} {b:?}"
        );
    }
}
#[test]
fn psd_capture_and_native_save_keep_every_pixel_resource() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zipzi-psd/tests/fixtures/layered.psd");
    let (imported, _) = ProjectImporter::default().import_psd(&source).unwrap();
    assert!(!imported.resources.is_empty());
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path(), ResourceStore::default());
    let path = root.path().join("drawing.zipzi");
    io.save(&imported.document, &imported.resources, &path, None, || {
        true
    })
    .unwrap();
    let loaded = io.load(&path, || true).unwrap();
    for id in imported.document.asset_order() {
        let a = imported
            .resources
            .read_asset(imported.document.asset(id).unwrap())
            .unwrap();
        let b = loaded
            .resources
            .read_asset(loaded.document.asset(id).unwrap())
            .unwrap();
        assert_eq!(a.rgba, b.rgba);
    }
}

#[test]
fn animation_sidecars_keep_ids_references_and_payloads_through_native_save() {
    let mut b = DocumentState::new(
        id(100),
        Canvas::new(100., 100., Vec2::default(), 10.).into(),
    )
    .unwrap()
    .builder();
    for (n, runtime) in [(101, "ParamX"), (102, "ParamY")] {
        b.create_parameter(Parameter {
            id: id(n),
            runtime_id: runtime.into(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
        .unwrap();
    }
    for (n, runtime) in [(103, "Part0"), (104, "Part1")] {
        b.create_part(Part {
            id: id(n),
            runtime_id: runtime.into(),
            ..Default::default()
        })
        .unwrap();
    }
    let state = b.freeze().unwrap().state;
    let expression = r#"{"Type":"Live2D Expression","FadeInTime":0.25,"Parameters":[{"Id":"ParamX","Value":0.75,"Blend":"Overwrite"}]}"#;
    let state = import_expression3(&state, &id(105), "Smile", expression)
        .unwrap()
        .candidate;
    let pose = include_str!("../../../fixtures/animation_cpu/minimal.pose3.json");
    let imported = import_pose3(&state, &id(106), pose).unwrap();
    assert!(imported.diagnostics.is_empty());
    let motion = include_str!("../../../fixtures/animation_cpu/typed.motion3.json");
    let imported = import_motion3(&imported.candidate, &id(107), "Idle", motion).unwrap();
    assert!(imported.diagnostics.is_empty());
    let physics = include_str!("../../../fixtures/animation_cpu/thirty.physics3.json");
    let imported = import_physics3(&imported.candidate, &id(108), physics).unwrap();
    assert!(imported.diagnostics.is_empty());
    let state = imported.candidate;
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path(), ResourceStore::default());
    let path = root.path().join("sidecars.zipzi");
    io.save(&state, &ResourceView::default(), &path, None, || true)
        .unwrap();
    let loaded = io.load(&path, || true).unwrap().document;
    let exports = |doc: &DocumentState| {
        [
            export_expression3(doc, &id(105)).unwrap(),
            export_pose3(doc).unwrap().unwrap(),
            export_motion3(doc, &id(107)).unwrap(),
            export_physics3(doc).unwrap().unwrap(),
        ]
        .map(|s| serde_json::from_str::<serde_json::Value>(&s).unwrap())
    };
    assert_eq!(exports(&state), exports(&loaded));
    assert_eq!(
        exports(&loaded)[0],
        serde_json::from_str::<serde_json::Value>(expression).unwrap()
    );
    assert_eq!(
        exports(&loaded)[1],
        serde_json::from_str::<serde_json::Value>(pose).unwrap()
    );
    let keys = |doc: &DocumentState| {
        doc.motion(&id(107))
            .unwrap()
            .tracks
            .iter()
            .flat_map(|t| t.keys().map(|k| k.id.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&state), keys(&loaded));
    let reimported = import_motion3(&loaded, &id(107), "Idle", motion)
        .unwrap()
        .candidate;
    assert_ne!(keys(&loaded), keys(&reimported));
    let unresolved = import_pose3(&loaded, &id(106), r#"{"Groups":[[{"Id":"Missing"}]]}"#).unwrap();
    assert_eq!(
        export_pose3(&unresolved.candidate).unwrap_err().code,
        "UNRESOLVED_PART"
    );
    assert_eq!(exports(&loaded), exports(&state));
}

#[test]
fn psd_export_uses_frozen_pixels_and_export_bounds_and_roundtrips() {
    let (state, resources) = model();
    let mut builder = state.builder();
    builder
        .set_setup(zipzi_core::ModelSetup {
            export: ExportSettings {
                bounds: Some([-2., -3., 14., 13.]),
                ..state.setup().export
            },
            ..state.setup()
        })
        .unwrap();
    let state = builder.freeze().unwrap().state;
    let content = state.content_id();
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("drawing.psd");
    fs::write(&output, b"previous export").unwrap();
    TransferSnapshot::new(state.clone(), resources, vec![])
        .unwrap()
        .publish(TransferFormat::Psd, &output)
        .unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(&bytes[..4], b"8BPS");
    assert_eq!(u32::from_be_bytes(bytes[14..18].try_into().unwrap()), 16);
    assert_eq!(u32::from_be_bytes(bytes[18..22].try_into().unwrap()), 16);
    let imported = zipzi_psd::import_psd(&bytes).unwrap();
    assert_eq!(imported.report.raster_layers, 1);
    assert_eq!(imported.assets.len(), 1);
    assert!(zipzi_core::image::decode_png(&imported.assets[0].bytes)
        .unwrap()
        .rgba
        .chunks_exact(4)
        .any(|p| p == [255, 255, 255, 255]));
    assert_eq!(state.content_id(), content);
}

#[test]
fn psd_export_failure_preserves_destination_and_protects_sources() {
    let (state, resources) = model();
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("drawing.psd");
    fs::write(&output, b"keep me").unwrap();
    let protected = fs::canonicalize(&output).unwrap();
    let error = TransferSnapshot::new(state.clone(), resources.clone(), vec![protected])
        .unwrap()
        .publish(TransferFormat::Psd, &output)
        .unwrap_err();
    assert_eq!(error.code, "INVALID_DESTINATION");
    let mut builder = state.builder();
    builder
        .set_setup(zipzi_core::ModelSetup {
            export: ExportSettings {
                bounds: Some([0., 0., 30001., 1.]),
                ..state.setup().export
            },
            ..state.setup()
        })
        .unwrap();
    let oversized = builder.freeze().unwrap().state;
    let error = TransferSnapshot::new(oversized, resources, vec![])
        .unwrap()
        .publish(TransferFormat::Psd, &output)
        .unwrap_err();
    assert_eq!(error.code, "PSD_EXPORT");
    assert_eq!(fs::read(&output).unwrap(), b"keep me");
}

#[test]
fn psd_export_rejects_unresolved_textures_without_replacing_file() {
    let (state, _) = model();
    let mut builder = state.builder();
    let mut asset = state.asset(&id(2)).unwrap().clone();
    asset.content = AssetContent::Unresolved {
        source: "missing.png".into(),
    };
    builder.replace_asset(asset).unwrap();
    let state = builder.freeze().unwrap().state;
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("drawing.psd");
    fs::write(&output, b"keep me").unwrap();
    assert!(
        TransferSnapshot::new(state, ResourceView::default(), vec![])
            .unwrap()
            .publish(TransferFormat::Psd, &output)
            .is_err()
    );
    assert_eq!(fs::read(output).unwrap(), b"keep me");
}

#[test]
fn psd_export_pixel_mask_survives_reimport_with_hidden_source() {
    let (state, resources) = model();
    let mut builder = state.builder();
    let mut source = state.mesh(&id(3)).unwrap().clone();
    let mut target = source.clone();
    source.enabled = false;
    source.base_positions = vec![Vec2::new(0., 0.), Vec2::new(4., 0.), Vec2::new(0., 4.)];
    builder.replace_mesh(source).unwrap();
    target.id = id(4);
    target.runtime_id = "Masked".into();
    target.name = "Masked".into();
    target.masks = vec![id(3)];
    builder.create_mesh(target).unwrap();
    let state = builder.freeze().unwrap().state;
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("masked.psd");
    let publication = TransferSnapshot::new(state, resources, vec![])
        .unwrap()
        .publish(TransferFormat::Psd, &output)
        .unwrap();
    assert!(!publication
        .notices
        .iter()
        .any(|note| note.contains("masks")));
    let imported = zipzi_psd::import_psd(&fs::read(output).unwrap()).unwrap();
    assert_eq!(imported.report.raster_layers, 2);
    let target = imported
        .document
        .mesh_order()
        .iter()
        .filter_map(|id| imported.document.mesh(id))
        .find(|mesh| mesh.name == "Masked")
        .unwrap();
    let asset = imported
        .assets
        .iter()
        .find(|asset| asset.id == target.texture_asset_id.as_str())
        .unwrap();
    let pixels = zipzi_core::image::decode_png(&asset.bytes).unwrap();
    let opaque = pixels.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
    assert!(
        opaque > 0 && opaque < 36,
        "clipping should survive import: {opaque}"
    );
}

#[test]
fn psd_roundtrip_preserves_native_resolution_pixels_without_resampling_drift() {
    let (state, _) = model();
    let pixels: Vec<u8> = (0..64)
        .flat_map(|i| {
            [
                (i * 3) as u8,
                (255 - i * 2) as u8,
                (i * 4) as u8,
                if i % 3 == 0 { 128 } else { 255 },
            ]
        })
        .collect();
    let image = ResourceStore::default()
        .prepare_rgba(8, 8, pixels.clone().into())
        .unwrap();
    let resources = ResourceView::default().with_images([image.clone()]);
    let mut builder = state.builder();
    let mut asset = state.asset(&id(2)).unwrap().clone();
    asset.content = AssetContent::Image {
        resource: image.key(),
        width: 8,
        height: 8,
    };
    builder.replace_asset(asset).unwrap();
    let mut mesh = state.mesh(&id(3)).unwrap().clone();
    mesh.vertex_ids = vec![1, 2, 3, 4];
    mesh.base_positions = vec![
        Vec2::new(0., 0.),
        Vec2::new(8., 0.),
        Vec2::new(8., -8.),
        Vec2::new(0., -8.),
    ];
    mesh.uvs = vec![
        Vec2::new(0., 1.),
        Vec2::new(1., 1.),
        Vec2::new(1., 0.),
        Vec2::new(0., 0.),
    ];
    mesh.triangles = vec![[1, 2, 3], [1, 3, 4]];
    builder.replace_mesh(mesh).unwrap();
    builder
        .set_setup(zipzi_core::ModelSetup {
            export: ExportSettings {
                bounds: Some([0., -8., 8., 0.]),
                ..state.setup().export
            },
            ..state.setup()
        })
        .unwrap();
    let mut state = builder.freeze().unwrap().state;
    let mut resources = resources;
    let root = tempfile::tempdir().unwrap();
    for iteration in 0..3 {
        let path = root.path().join(format!("roundtrip-{iteration}.psd"));
        TransferSnapshot::new(state, resources, vec![])
            .unwrap()
            .publish(TransferFormat::Psd, &path)
            .unwrap();
        let (imported, _) = ProjectImporter::default().import_psd(&path).unwrap();
        let asset = imported
            .document
            .asset(imported.document.asset_order().first().unwrap())
            .unwrap();
        let actual = imported.resources.read_texture(asset).unwrap();
        assert_eq!((actual.width, actual.height), (8, 8));
        assert_eq!(actual.rgba.as_ref(), pixels, "roundtrip {iteration}");
        state = imported.document;
        resources = imported.resources;
    }
}

#[test]
fn psd_export_keeps_mesh_display_names_and_nested_parts_on_reimport() {
    let (state, resources) = model();
    let mut builder = state.builder();
    for (key, parent, name) in [
        (5, None, "头部"),
        (6, Some(5), "眼睛"),
        (7, Some(6), "空分组"),
    ] {
        builder
            .create_part(Part {
                id: id(key),
                runtime_id: format!("Part{key}"),
                parent_id: parent.map(id).unwrap_or_default(),
                name: name.into(),
                ..Default::default()
            })
            .unwrap();
    }
    let mut mesh = state.mesh(&id(3)).unwrap().clone();
    mesh.name = "左眼 / 高光".into();
    mesh.runtime_id = "ArtMesh_Different_Runtime_ID".into();
    mesh.part_id = id(6);
    builder.replace_mesh(mesh.clone()).unwrap();
    mesh.id = id(4);
    mesh.runtime_id = "Another_Runtime_ID".into();
    builder.create_mesh(mesh).unwrap();
    let mut state = builder.freeze().unwrap().state;
    let mut resources = resources;
    let root = tempfile::tempdir().unwrap();
    for round in 0..2 {
        let path = root.path().join(format!("named-{round}.psd"));
        let publication = TransferSnapshot::new(state, resources, vec![])
            .unwrap()
            .publish(TransferFormat::Psd, &path)
            .unwrap();
        assert!(publication.notices.is_empty());
        let (imported, report) = ProjectImporter::default().import_psd(&path).unwrap();
        assert_eq!(report.groups, 3);
        assert_eq!(report.raster_layers, 2);
        for mesh_id in imported.document.mesh_order() {
            let mesh = imported.document.mesh(mesh_id).unwrap();
            assert_eq!(mesh.name, "左眼 / 高光");
            let eyes = imported.document.part(&mesh.part_id).unwrap();
            assert_eq!(eyes.name, "眼睛");
            let head = imported.document.part(&eyes.parent_id).unwrap();
            assert_eq!(head.name, "头部");
        }
        let empty = imported
            .document
            .part_order()
            .iter()
            .filter_map(|id| imported.document.part(id))
            .find(|p| p.name == "空分组")
            .unwrap();
        assert_eq!(
            imported.document.part(&empty.parent_id).unwrap().name,
            "眼睛"
        );
        state = imported.document;
        resources = imported.resources;
    }
}
