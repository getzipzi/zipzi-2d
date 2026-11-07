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
