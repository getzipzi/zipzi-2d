use editor_core::{Document, Editor, ExportCanvas, LoadRequest, TransferFormat};
use kasane_core::{Canvas, ExportSettings, Vec2};
use std::path::PathBuf;
fn local(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao")
        .join(name)
}

#[test]
fn local_v3a9_cubism_export_retains_expression_poses_and_playback_frames() -> anyhow::Result<()> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/v3a9_315702_nxss/v3a9_315702_nxss.can3");
    if !source.is_file() {
        eprintln!("SKIP: local v3a9 project unavailable");
        return Ok(());
    }
    // Build the optional fixture from its external source so a breaking native
    // schema change never requires a compatibility migration for local caches.
    let mut editor = Editor::from_document(Document::import_cubism(&source)?.0);
    let id = editor.active_id().unwrap();
    let revision = editor.document().unwrap().model_generation();
    let canvas = editor.export_canvas(id)?;
    let snapshot = editor.prepare_export(id, revision, canvas)?;
    let original = snapshot.document().clone();
    let temp = tempfile::tempdir()?;
    let parent = std::env::var_os("KASANE_V3A9_EXPORT_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_path_buf());
    std::fs::create_dir_all(&parent)?;
    let parent = std::fs::canonicalize(parent)?;
    let destination = parent.join("v3a9");
    let publication = snapshot
        .publish(TransferFormat::Cubism, &destination)
        .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
    assert!(publication.durable());
    assert!(!publication.notices.is_empty());
    for extension in ["cmo3", "can3"] {
        assert!(destination.join(format!("v3a9.{extension}")).is_file());
    }
    let report: kasane_cmo3::Cmo3Report =
        serde_json::from_slice(&std::fs::read(destination.join("export-report.json"))?)?;
    assert!(!report.has_loss());
    assert!(!report.diagnostics.iter().any(|d| matches!(
        d.code.as_str(),
        "CAN3_BAKED_TRACK" | "CAN3_KEY_COLLISION" | "CAN3_TIME_GRID_UNRESOLVED"
    )));
    // The external source has not received the old local draft's timing edits;
    // repair diagnostics depend on those edits, while expression semantics must
    // still be retained through this export.
    for code in ["CAN3_EXPRESSION_SEMANTICS"] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code)
        );
    }
    let model = kasane_cmo3::import_model(&std::fs::read(destination.join("v3a9.cmo3"))?)?;
    let mut returned = model.document;
    kasane_cmo3::import_animation(
        &std::fs::read(destination.join("v3a9.can3"))?,
        &mut returned,
    )?;
    assert_eq!(returned.mesh_order().len(), original.mesh_order().len());
    assert_eq!(returned.motion_order().len(), 27);
    assert_eq!(returned.expression_order().len(), 8);
    assert_eq!(
        external_motion_groups(returned.motion_groups()),
        external_motion_groups(original.motion_groups())
    );
    let mut animation_reference = Vec::new();
    for id in original.expression_order() {
        let before = original.get_expression(id).unwrap();
        let after = returned.get_expression(id).unwrap();
        assert_eq!(after.name, before.name);
        assert_eq!(after.entries.len(), before.entries.len());
        let mut reference_tracks = Vec::new();
        for entry in &before.entries {
            use kasane_core::document::{ExpressionBlend, ExpressionTarget};
            let ExpressionTarget::Resolved { parameter_id } = &entry.target else {
                panic!("unresolved fixture expression")
            };
            let default = original.get_parameter(parameter_id).unwrap().default_value;
            let expected = match entry.blend.unwrap_or(ExpressionBlend::Add) {
                ExpressionBlend::Add => default + entry.value,
                ExpressionBlend::Multiply => default * entry.value,
                ExpressionBlend::Overwrite => entry.value,
            };
            let parameter = original.get_parameter(parameter_id).unwrap();
            let expected = expected.clamp(parameter.minimum, parameter.maximum);
            let actual = after
                .entries
                .iter()
                .find(|candidate| candidate.target == entry.target)
                .unwrap();
            assert_eq!(
                kasane_core::document::ExpressionEntryFields::from(actual),
                kasane_core::document::ExpressionEntryFields::from(entry)
            );
            let companion: serde_json::Value = serde_json::from_slice(&std::fs::read(
                destination.join(format!("expression-{id}.exp3.json")),
            )?)?;
            let runtime_id = &parameter.runtime_id;
            let exported = companion["Parameters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|candidate| candidate["Id"] == *runtime_id)
                .unwrap();
            assert_eq!(exported["Blend"], "Multiply");
            assert!((exported["Value"].as_f64().unwrap() as f32 - entry.value).abs() < 0.000001);
            reference_tracks.push(serde_json::json!({
                "attribute": format!("live2dParam_{}", original.get_parameter(parameter_id).unwrap().runtime_id),
                "values": [expected],
            }));
        }
        animation_reference
            .push(serde_json::json!({"name":before.name, "tracks":reference_tracks}));
        for baseline in ["min", "default", "max"] {
            let mut expected = kasane_sdk::ExpressionPreview::new(&original);
            let mut actual = kasane_sdk::ExpressionPreview::new(&returned);
            for preview in [&mut expected, &mut actual] {
                if baseline != "default" {
                    for parameter_id in original.parameter_order() {
                        let parameter = original.get_parameter(parameter_id).unwrap();
                        preview.set_base_parameter(
                            parameter_id,
                            if baseline == "min" {
                                parameter.minimum
                            } else {
                                parameter.maximum
                            },
                        )?;
                    }
                }
                preview.schedule_expression(id, 0.)?;
                let next = original
                    .expression_order()
                    .iter()
                    .find(|candidate| *candidate != id)
                    .unwrap();
                preview.schedule_expression(next, 0.5)?;
            }
            for _ in 0..60 {
                assert_eq!(
                    expected.advance(1. / 60.)?.parameters,
                    actual.advance(1. / 60.)?.parameters,
                    "{}: {baseline} baseline and cross-fade",
                    before.name
                );
            }
        }
    }
    let mut samples = 0;
    let mut original_anchors = 0;
    let mut exported_anchors = 0;
    let mut original_beziers = 0;
    let mut exported_beziers = 0;
    let mut dense_checks = 0;
    let mut maximum_error = 0f32;
    for id in original.motion_order() {
        let before = original.get_motion(id).unwrap();
        let after = returned.get_motion(id).unwrap();
        assert_eq!(after.fps, 60.);
        assert!((after.duration - before.duration).abs() <= 0.000501);
        assert_eq!(after.tracks.len(), before.tracks.len());
        let mut reference_tracks = Vec::new();
        for track in &before.tracks {
            let actual = after
                .tracks
                .iter()
                .find(|candidate| candidate.target == track.target)
                .unwrap();
            let mut normalized = track.clone();
            normalized.initial.time = (f64::from(track.initial.time) * 60.).round() as f32 / 60.;
            let mut start = track.initial.time;
            let mut normalized_start = normalized.initial.time;
            let segments = std::sync::Arc::make_mut(&mut normalized.segments);
            for segment in segments {
                let old_end = segment.end().time;
                let new_end = (f64::from(old_end) * 60.).round() as f32 / 60.;
                if let kasane_core::document::MotionSegment::Bezier {
                    control1, control2, ..
                } = segment
                {
                    if before.restricted_beziers {
                        control1.time = normalized_start + (new_end - normalized_start) / 3.;
                        control2.time = normalized_start + (new_end - normalized_start) * 2. / 3.;
                    } else {
                        control1.time = normalized_start
                            + (new_end - normalized_start) * (control1.time - start)
                                / (old_end - start);
                        control2.time = normalized_start
                            + (new_end - normalized_start) * (control2.time - start)
                                / (old_end - start);
                    }
                }
                segment.end_mut().time = new_end;
                start = old_end;
                normalized_start = new_end;
            }
            let source_curve = kasane_sdk::CompiledCurve::from(&normalized);
            let result_curve = kasane_sdk::CompiledCurve::from(actual);
            let mut reference_values = Vec::new();
            let mut extra_checks = Vec::new();
            for frame in 0..=(before.duration * 60.).round() as usize {
                let time = frame as f32 / 60.;
                let expected =
                    kasane_sdk::sample_motion_curve(&source_curve, time, before.restricted_beziers);
                let actual =
                    kasane_sdk::sample_motion_curve(&result_curve, time, after.restricted_beziers);
                assert!(
                    (actual - expected).abs() < 0.0001,
                    "{} {} frame {frame}: {actual} != {expected}",
                    before.name,
                    track.id
                );
                samples += 1;
                maximum_error = maximum_error.max((actual - expected).abs());
                reference_values.push(expected);
            }
            original_anchors += track.keys().count();
            exported_anchors += actual.keys().count();
            for key in normalized.keys() {
                assert!(
                    actual
                        .keys()
                        .any(|candidate| (candidate.time - key.time).abs() < 0.000001),
                    "Original anchor missing: {} {}",
                    before.name,
                    key.time
                );
            }
            original_beziers += track
                .segments
                .iter()
                .filter(|s| matches!(s, kasane_core::document::MotionSegment::Bezier { .. }))
                .count();
            exported_beziers += actual
                .segments
                .iter()
                .filter(|s| matches!(s, kasane_core::document::MotionSegment::Bezier { .. }))
                .count();
            let mut start = normalized.initial.time;
            for (index, segment) in normalized.segments.iter().enumerate() {
                if matches!(segment, kasane_core::document::MotionSegment::Bezier { .. }) {
                    let mut end = segment.end().time;
                    if normalized.segments.get(index + 1).is_some_and(|s| {
                        matches!(
                            s,
                            kasane_core::document::MotionSegment::InverseStepped { .. }
                        )
                    }) {
                        end -= 1. / 60.;
                    }
                    for fraction in [0.125, 0.25, 0.5, 0.75, 0.875] {
                        let time = start + (end - start) * fraction;
                        let expected = kasane_sdk::sample_motion_curve(
                            &source_curve,
                            time,
                            before.restricted_beziers,
                        );
                        let value = kasane_sdk::sample_motion_curve(
                            &result_curve,
                            time,
                            after.restricted_beziers,
                        );
                        assert!(
                            (value - expected).abs() < 0.0001,
                            "Bezier changed: {} {} at {time}: {value} != {expected}",
                            before.name,
                            track.id
                        );
                        maximum_error = maximum_error.max((value - expected).abs());
                        extra_checks.push(
                            serde_json::json!({"frame":f64::from(time)*60.,"value":expected}),
                        );
                        dense_checks += 1;
                    }
                }
                start = segment.end().time;
            }
            use kasane_core::document::MotionTrackTarget;
            let attribute = match &track.target {
                MotionTrackTarget::Parameter { parameter_id } => format!(
                    "live2dParam_{}",
                    original.get_parameter(parameter_id).unwrap().runtime_id
                ),
                MotionTrackTarget::PartOpacity { part_id } => format!(
                    "live2DPartsOpacity_{}",
                    original.get_part(part_id).unwrap().runtime_id
                ),
                MotionTrackTarget::Model { runtime_id } if runtime_id == "Opacity" => {
                    "opacity".into()
                }
                _ => panic!("unsupported fixture channel"),
            };
            reference_tracks
                .push(serde_json::json!({"attribute":attribute,"values":reference_values,"checks":extra_checks}));
        }
        animation_reference
            .push(serde_json::json!({"name":before.name,"fps":60,"tracks":reference_tracks}));
    }
    assert_eq!(original_anchors, 17736);
    assert_eq!(original_beziers, 1868);
    assert_eq!(exported_beziers, original_beziers);
    if std::env::var_os("KASANE_V3A9_EXPORT_OUTPUT").is_some() {
        std::fs::write(
            parent.join("animation-reference.json"),
            serde_json::to_vec(&animation_reference)?,
        )?;
        std::fs::write(
            parent.join("curve-verification.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "fps":60,"original_anchors":original_anchors,"exported_anchors":exported_anchors,
                "original_beziers":original_beziers,"exported_beziers":exported_beziers,
                "frame_samples":samples,"bezier_subframe_samples":dense_checks,"maximum_error":maximum_error,
                "comparison":"Source timestamps restored from millisecond rounding onto the inferred 60 FPS grid; inverse-step final-frame boundary adaptations are reported separately"
            }))?,
        )?;
    }
    eprintln!(
        "v3a9: 60 FPS, {original_anchors} original anchors retained ({exported_anchors} exported), {exported_beziers} Beziers, {samples} frame samples, {dense_checks} subframe checks, maximum error {maximum_error}; {}",
        destination.display()
    );
    Ok(())
}

#[test]
fn local_v3a9_native_saved_can3_retains_original_expression_operations() -> anyhow::Result<()> {
    let Some(saved) = std::env::var_os("KASANE_V3A9_NATIVE_SAVED_CAN3") else {
        eprintln!("SKIP: set KASANE_V3A9_NATIVE_SAVED_CAN3 after the native Cubism save probe");
        return Ok(());
    };
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/v3a9_315702_nxss.kasane");
    let original = Document::open(&source)?;
    let (returned, _) = Document::import_cubism(&PathBuf::from(saved))?;
    let expressions = original.expression_resources();
    assert_eq!(expressions.len(), 8);
    assert_eq!(returned.expression_resources().len(), expressions.len());
    let mut entries = 0;
    for expression in expressions {
        let before = original.expression_definition(&expression.id).unwrap();
        let after = returned.expression_definition(&expression.id).unwrap();
        assert_eq!(before.entries, after.entries);
        assert_eq!(before.fade_in, after.fade_in);
        assert_eq!(before.fade_out, after.fade_out);
        entries += before.entries.len();
    }
    eprintln!("Native Cubism save/reimport: restored {entries} original expression operations");
    Ok(())
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
            // Third-party files carry runtime operations, not native entry IDs.
            assert_eq!(after.entries.len(), before.entries.len());
            for (a, b) in after.entries.iter().zip(&before.entries) {
                assert_eq!(a.target, b.target);
                assert_eq!(a.value, b.value);
                assert_eq!(a.blend, b.blend);
                assert_eq!(a.extensions, b.extensions);
            }
            assert_eq!(
                after
                    .entries
                    .iter()
                    .map(|e| &e.id)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                after.entries.len()
            );
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
    assert_eq!(
        external_motion_groups(returned.document().motion_groups()),
        external_motion_groups(&groups)
    );
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

fn external_motion_groups(
    groups: &[kasane_core::document::MotionGroup],
) -> Vec<kasane_core::document::MotionGroup> {
    let mut groups = groups.to_vec();
    for group in &mut groups {
        group.id.clear();
        for entry in &mut group.entries {
            entry.id.clear();
        }
    }
    groups
}
