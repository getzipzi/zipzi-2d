use editor_core::{Document, Editor, ExportCanvas, LoadRequest, TransferFormat};
use kasane_core::{Canvas, ExportSettings, Vec2};
use std::path::PathBuf;
fn local(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao")
        .join(name)
}

/// Native archives retain their author's absolute links. Relocate only a temporary
/// copy so acceptance never depends on or reads the Downloads original.
pub(super) fn portable_mao_fixture() -> anyhow::Result<tempfile::TempDir> {
    let temp = tempfile::tempdir()?;
    std::fs::copy(
        local("mao_pro_t06.cmo3"),
        temp.path().join("mao_pro_t06.cmo3"),
    )?;
    let animation = kasane_cmo3::test_support::animation_with_sibling_models(&std::fs::read(
        local("mao_pro_t06.can3"),
    )?)?;
    std::fs::write(temp.path().join("mao_pro_t06.can3"), animation)?;
    Ok(temp)
}

#[test]
fn moc3_entry_matches_reference_and_rejects_missing_or_ambiguous_entry() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let moc = dir.path().join("actual.moc3");
    std::fs::write(&moc, b"not decoded by entry resolution")?;
    assert!(kasane_project::resolve_model3_entry(&moc).is_err());
    let contents = r#"{"FileReferences":{"Moc":"actual.moc3"}}"#;
    let entry = dir.path().join("different-name.model3.json");
    std::fs::write(&entry, contents)?;
    assert_eq!(kasane_project::resolve_model3_entry(&moc).unwrap(), entry);
    std::fs::write(dir.path().join("second.model3.json"), contents)?;
    assert!(kasane_project::resolve_model3_entry(&moc).is_err());
    Ok(())
}
#[test]
fn export_settings_are_project_data_and_continue_keeps_them_without_output() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let id = editor.active_id().unwrap();
    let doc = editor.document().unwrap();
    let revision = doc.model_generation();
    let source = doc.source_canvas();
    let canvas = ExportCanvas {
        manual: true,
        width: 800.,
        height: 600.,
        origin_x: 300.,
        origin_y: 250.,
        settings: ExportSettings {
            pixels_per_unit: 50.,
            ..Default::default()
        },
    };
    let snapshot = editor.prepare_export(id, revision, canvas)?;
    assert_eq!(
        snapshot.document().export_settings().bounds,
        Some([-300., -350., 500., 250.])
    );
    assert_eq!(
        editor.document().unwrap().export_settings(),
        canvas.validated()?
    );
    assert_eq!(editor.document().unwrap().source_canvas(), source);
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("saved.kasane");
    let mut completion = editor.prepare_save(id, &path, false)?.run();
    editor.finish_save(&mut completion)?;
    let reopened = Document::open(&path)?;
    assert_eq!(reopened.export_settings(), canvas.validated()?);
    // The next import gets its own document, never overwrites the source.
    editor.new_document()?;
    assert!(editor.prepare_export(id, revision, canvas).is_err());
    Ok(())
}
#[test]
fn invalid_export_canvas_is_rejected_before_settings_change() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let id = editor.active_id().unwrap();
    let revision = editor.document().unwrap().model_generation();
    let before = editor.document().unwrap().export_settings();
    let mut canvas = editor.export_canvas(id)?;
    canvas.manual = true;
    canvas.width = f32::NAN;
    assert!(editor.prepare_export(id, revision, canvas).is_err());
    assert_eq!(editor.document().unwrap().export_settings(), before);
    Ok(())
}
#[test]
fn model_only_cubism_group_has_no_empty_can3_and_rejects_unmanaged_output() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let id = editor.active_id().unwrap();
    let revision = editor.document().unwrap().model_generation();
    let canvas = editor.export_canvas(id)?;
    let snapshot = editor.prepare_export(id, revision, canvas)?;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("model");
    snapshot
        .publish(TransferFormat::Cubism, &path)
        .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
    assert!(path.join("model.cmo3").is_file());
    assert!(!path.join("model.can3").exists());
    std::fs::write(path.join("unrelated.txt"), b"keep")?;
    let revision = editor.document().unwrap().model_generation();
    assert!(
        editor
            .prepare_export(id, revision, canvas)?
            .publish(TransferFormat::Cubism, &path)
            .is_err()
    );
    assert_eq!(std::fs::read(path.join("unrelated.txt"))?, b"keep");
    Ok(())
}
#[test]
fn real_cubism_editor_mao_imports_from_both_entries_and_saves() -> anyhow::Result<()> {
    let path = local("mao_pro_t06.cmo3");
    if !path.exists() {
        eprintln!("SKIP: local Mao Cubism project unavailable");
        return Ok(());
    }
    let fixture = portable_mao_fixture()?;
    for extension in ["cmo3", "can3"] {
        let path = fixture.path().join(format!("mao_pro_t06.{extension}"));
        let (mut doc, notices) = Document::import_cubism(&path)?;
        assert!(notices.is_empty());
        assert_eq!(doc.motion_resources().len(), 7);
        let expressions = doc.expression_resources();
        assert_eq!(expressions.len(), 8);
        assert!(
            expressions
                .iter()
                .all(|expression| expression.name.starts_with("exp_"))
        );
        let normal = expressions
            .iter()
            .find(|expression| expression.name == "exp_01")
            .unwrap();
        let normal = doc.expression_definition(&normal.id).unwrap();
        assert_eq!(normal.source_scene().unwrap().duration, 8. / 30.);
        assert!(
            normal
                .entries
                .iter()
                .all(|entry| entry.blend == Some(kasane_core::document::ExpressionBlend::Add))
        );
        assert_eq!(doc.parameters().len(), 128);
        let frame = doc.snapshot()?;
        assert_eq!(frame.runtime_frame.drawables.len(), 261);
        assert!(!frame.textures.is_empty());
        assert_eq!(
            doc.source_canvas(),
            Some(Canvas::new(5800., 8400., Vec2::new(0., 0.), 1.))
        );
        assert_eq!(doc.export_settings().bounds, Some([0., -8400., 5800., 0.]));
        let mut editor = Editor::from_document(doc);
        let id = editor.active_id().unwrap();
        let temp = tempfile::tempdir()?;
        let saved = temp.path().join("mao.kasane");
        let output = std::env::var_os("KASANE_TRANSFER_ACCEPTANCE_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().to_path_buf());
        std::fs::create_dir_all(&output)?;
        let revision = editor.document().unwrap().model_generation();
        let canvas = editor.export_canvas(id)?;
        editor
            .prepare_export(id, revision, canvas)?
            .publish(TransferFormat::Runtime, &output.join("runtime"))
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        let revision = editor.document().unwrap().model_generation();
        editor
            .prepare_export(id, revision, canvas)?
            .publish(TransferFormat::Cubism, &output.join("engineering"))
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        let mut completion = editor.prepare_save(id, &saved, false)?.run();
        editor.finish_save(&mut completion)?;
        let mut reopened = Document::open(&saved)?;
        assert_eq!(reopened.motion_resources().len(), 7);
        for expression in &expressions {
            assert_eq!(
                reopened.expression_definition(&expression.id),
                editor
                    .document()
                    .unwrap()
                    .expression_definition(&expression.id)
            );
        }
        let runtime: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("runtime/model.model3.json"))?)?;
        let exported_expressions = runtime["FileReferences"]["Expressions"].as_array().unwrap();
        assert_eq!(exported_expressions.len(), 8);
        assert_eq!(
            runtime["FileReferences"]["Motions"]
                .as_object()
                .unwrap()
                .values()
                .map(|clips| clips.as_array().unwrap().len())
                .sum::<usize>(),
            7
        );
        for registration in exported_expressions {
            let contents: serde_json::Value = serde_json::from_slice(&std::fs::read(
                output
                    .join("runtime")
                    .join(registration["File"].as_str().unwrap()),
            )?)?;
            assert!(contents.get("KasaneSourceScene").is_none());
            assert!(
                contents["Parameters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|entry| entry["Blend"] == "Add")
            );
        }
        let (roundtrip, _) = Document::import_cubism(&output.join("engineering/engineering.can3"))?;
        assert_eq!(roundtrip.motion_resources().len(), 7);
        assert_eq!(roundtrip.expression_resources().len(), 8);
        for expression in &expressions {
            let before = editor
                .document()
                .unwrap()
                .expression_definition(&expression.id)
                .unwrap();
            let after = roundtrip.expression_definition(&expression.id).unwrap();
            assert_eq!(after.name, before.name);
            assert_eq!(after.entries, before.entries);
            assert_eq!(
                after.source_scene().unwrap().duration,
                before.source_scene().unwrap().duration
            );
        }
        assert_eq!(reopened.snapshot()?.runtime_frame.drawables.len(), 261);
        assert_eq!(
            reopened.export_settings(),
            editor.document().unwrap().export_settings()
        );
    }
    Ok(())
}

#[test]
fn mao_runtime_returns_to_cubism_with_part_names_and_motion_groups() -> anyhow::Result<()> {
    let source = local("mao_pro_t06.cmo3");
    if !source.is_file() {
        eprintln!("SKIP: local Mao Cubism project unavailable");
        return Ok(());
    }
    let session = || {
        kasane_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(1., 1., Vec2::default(), 1.),
        )
    };
    let part_names = |doc: &kasane_core::Document| -> std::collections::BTreeMap<String, String> {
        doc.part_order()
            .iter()
            .map(|id| {
                let part = doc.get_part(id).unwrap();
                (part.runtime_id.clone(), part.name.clone())
            })
            .collect()
    };
    let fixture = portable_mao_fixture()?;
    let mut original = session()?;
    original.import_cubism(&fixture.path().join("mao_pro_t06.cmo3"), None)?;
    let expected = part_names(original.document());
    assert_eq!(expected.len(), 32);
    let temp = tempfile::tempdir()?;
    let runtime_path = temp.path().join("runtime");
    original
        .transfer_snapshot()?
        .publish(TransferFormat::Runtime, &runtime_path)
        .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
    let cdi: serde_json::Value =
        serde_json::from_slice(&std::fs::read(runtime_path.join("model.cdi3.json"))?)?;
    let parts = cdi["Parts"].as_array().unwrap();
    let actual: std::collections::BTreeMap<_, _> = parts
        .iter()
        .map(|part| {
            (
                part["Id"].as_str().unwrap().to_owned(),
                part["Name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
    let mut runtime = session()?;
    runtime.import_model3(&runtime_path.join("model.model3.json"), None)?;
    assert_eq!(part_names(runtime.document()), expected);
    let groups = runtime.document().motion_groups().to_vec();
    assert!(!groups.is_empty());
    let returned_path = temp.path().join("returned");
    runtime
        .transfer_snapshot()?
        .publish(TransferFormat::Cubism, &returned_path)
        .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
    let mut returned = session()?;
    returned.import_cubism(&returned_path.join("returned.can3"), None)?;
    assert_eq!(part_names(returned.document()), expected);
    assert_eq!(returned.document().motion_groups(), groups);
    assert_eq!(returned.document().motion_order().len(), 7);
    assert_eq!(returned.document().expression_order().len(), 8);
    let final_runtime = temp.path().join("final-runtime");
    returned
        .transfer_snapshot()?
        .publish(TransferFormat::Runtime, &final_runtime)
        .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
    let mut final_document = session()?;
    final_document.import_model3(&final_runtime.join("model.model3.json"), None)?;
    assert_eq!(part_names(final_document.document()), expected);
    let group_names: Vec<_> = final_document
        .document()
        .motion_groups()
        .iter()
        .map(|g| (&g.name, g.entries.len()))
        .collect();
    assert_eq!(
        group_names,
        groups
            .iter()
            .map(|g| (&g.name, g.entries.len()))
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn can3_without_its_associated_model_does_not_replace_the_active_document() -> anyhow::Result<()> {
    let source = local("mao_pro_t06.can3");
    if !source.is_file() {
        eprintln!("SKIP: local CAN3 unavailable");
        return Ok(());
    }
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("animation.can3");
    let animation =
        kasane_cmo3::test_support::animation_with_sibling_models(&std::fs::read(source)?)?;
    std::fs::write(&path, animation)?;
    let mut editor = Editor::new()?;
    let original = editor.active_id();
    let task = editor.prepare_load(LoadRequest::Cubism(path))?.unwrap();
    assert!(editor.finish_load(task.run()).is_err());
    assert_eq!(editor.active_id(), original);
    assert_eq!(editor.document_count(), 1);
    Ok(())
}
