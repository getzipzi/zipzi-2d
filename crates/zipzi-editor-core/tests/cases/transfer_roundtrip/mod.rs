//! Opt-in diagnostic audit. A completed audit is not a claim of losslessness;
//! every changed field and blocked conversion is retained in report.json.
mod compare;
use anyhow::{Context, Result, anyhow, ensure};
use compare::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use zipzi_core::{Canvas, Document, Vec2, document::MotionTrackTarget};
use zipzi_project::{TransferFormat, content_sha256};
use zipzi_sdk::{AuthoringSession, CompiledCurve, ExpressionPreview, sample_motion_curve};

fn session() -> Result<AuthoringSession> {
    Ok(AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(1., 1., Vec2::default(), 1.),
    )?)
}

fn load(path: &Path, format: &str) -> Result<AuthoringSession> {
    load_with_diagnostics(path, format).map(|(session, _)| session)
}
fn load_with_diagnostics(path: &Path, format: &str) -> Result<(AuthoringSession, Vec<Value>)> {
    let mut result = session()?;
    let mut diagnostics = Vec::new();
    match format {
        "K" => {
            result.open_project(path, None)?;
        }
        "C" => {
            result.import_cubism(path, None)?;
        }
        "R" => {
            let path = zipzi_project::resolve_model3_entry(path)
                .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
            let receipt = result.import_model3(&path, None)?;
            diagnostics = receipt
                .project
                .diagnostics
                .iter()
                .map(|d| json!({"asset":d.asset_id,"code":d.code,"message":d.message}))
                .collect();
        }
        _ => unreachable!(),
    }
    ensure!(
        result.document().validate_structure().is_empty(),
        "Invalid imported structure"
    );
    ensure!(
        result.diagnose_resources().is_empty(),
        "Missing imported resources"
    );
    Ok((result, diagnostics))
}

fn transfer(
    current: &mut AuthoringSession,
    format: &str,
    destination: &Path,
) -> Result<AuthoringSession> {
    match format {
        "K" => {
            let saved = current.save_project(destination, None).context("save")?;
            load(&saved.manifest, "K").context("reimport")
        }
        "C" | "R" => {
            let snapshot = current.transfer_snapshot().context("capture")?;
            snapshot
                .publish(
                    if format == "C" {
                        TransferFormat::Cubism
                    } else {
                        TransferFormat::Runtime
                    },
                    destination,
                )
                .map_err(|s| anyhow!("{}: {}", s.code, s.message))
                .context("publish")?;
            if format == "C" {
                let name = destination.file_name().unwrap().to_str().unwrap();
                load(&destination.join(format!("{name}.can3")), "C").context("reimport")
            } else {
                load(&destination.join("model.model3.json"), "R").context("reimport")
            }
        }
        _ => unreachable!(),
    }
}

fn target(document: &Document, target: &MotionTrackTarget) -> String {
    match target {
        MotionTrackTarget::Parameter { parameter_id } => format!(
            "parameter/{}",
            document.get_parameter(parameter_id).unwrap().runtime_id
        ),
        MotionTrackTarget::PartOpacity { part_id } => {
            format!("part/{}", document.get_part(part_id).unwrap().runtime_id)
        }
        MotionTrackTarget::Model { runtime_id } => format!("model/{runtime_id}"),
        _ => format!("unresolved/{target:?}"),
    }
}

fn behavior(document: &Document) -> Result<Value> {
    let mut motions = BTreeMap::new();
    for id in document.motion_order() {
        let clip = document.get_motion(id).unwrap();
        let mut tracks = BTreeMap::new();
        for track in &clip.tracks {
            let curve = CompiledCurve::from(track);
            let mut times: Vec<_> = (0..=128).map(|i| clip.duration * i as f32 / 128.).collect();
            times.extend([1. / clip.fps, (clip.duration - 1. / clip.fps).max(0.)]);
            tracks.insert(
                target(document, &track.target),
                json!(
                    times
                        .into_iter()
                        .map(|t| sample_motion_curve(&curve, t, clip.restricted_beziers))
                        .collect::<Vec<_>>()
                ),
            );
        }
        motions.insert(
            clip.name.clone(),
            json!({"duration":clip.duration,"fps":clip.fps,"loop":clip.looping,"tracks":tracks}),
        );
    }
    let mut expressions = BTreeMap::new();
    for id in document.expression_order() {
        let expression = document.get_expression(id).unwrap();
        let mut cases = BTreeMap::new();
        for baseline in ["min", "default", "max"] {
            let mut preview = ExpressionPreview::new(document);
            if baseline != "default" {
                for id in document.parameter_order() {
                    let parameter = document.get_parameter(id).unwrap();
                    preview.set_base_parameter(
                        id,
                        if baseline == "min" {
                            parameter.minimum
                        } else {
                            parameter.maximum
                        },
                    )?;
                }
            }
            preview.schedule_expression(id, 0.)?;
            preview.advance(0.)?;
            preview.advance(expression.fade_in.unwrap_or(1.) + 0.01)?;
            let values: BTreeMap<_, _> = preview
                .snapshot()
                .parameters
                .iter()
                .map(|(id, value)| {
                    (
                        document.get_parameter(id).unwrap().runtime_id.clone(),
                        *value,
                    )
                })
                .collect();
            cases.insert(baseline, values);
        }
        expressions.insert(expression.name.clone(), cases);
    }
    Ok(json!({"motion_samples":motions,"expression_samples":expressions}))
}

struct Capture {
    document: Value,
    behavior: Value,
    geometry: Geometry,
}
fn capture(session: &AuthoringSession, cases: &[Pose]) -> Result<Capture> {
    Ok(Capture {
        document: canonical_document(session)?,
        behavior: behavior(session.document())?,
        geometry: geometry(session.document(), cases)?,
    })
}
fn changes(before: &Capture, after: &Capture) -> Value {
    json!({
        "document": difference(&before.document, &after.document, 1e-6),
        "behavior": difference(&before.behavior, &after.behavior, 1e-5),
        "geometry": geometry_difference(&before.geometry,&after.geometry),
    })
}
fn changed(value: &Value) -> bool {
    value["document"]["changed"].as_u64().unwrap() > 0
        || value["behavior"]["changed"].as_u64().unwrap() > 0
        || value["geometry"]["vertices_over_0_1px"].as_u64().unwrap() > 0
        || !value["geometry"]["missing"].as_array().unwrap().is_empty()
        || value["geometry"]["appearance_topology"]["changed"]
            .as_u64()
            .unwrap()
            > 0
}

fn fixture_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    fn walk(root: &Path, dir: &Path, hashes: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(root, &path, hashes)?;
            } else {
                hashes.insert(
                    path.strip_prefix(root)?.to_string_lossy().into_owned(),
                    content_sha256(&fs::read(path)?),
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result)?;
    Ok(result)
}

fn diagnostic_counts(error: &str) -> BTreeMap<&str, usize> {
    let mut codes = BTreeMap::new();
    for line in error.lines() {
        if let Some(code) = line.split(':').map(str::trim).find(|s| {
            s.contains('_')
                && *s != "CUBISM_TRANSFER"
                && s.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        }) {
            *codes.entry(code).or_default() += 1;
        }
    }
    codes
}

fn markdown_report(report: &Value) -> String {
    let mut text = format!(
        "# Mao 格式转换测试结果\n\n每条路径循环 {} 轮。K=Zipzi，C=CMO3/CAN3，R=model3/MOC3。\n\n`stable` 表示该步在文档/行为容差及 0.1px 顶点阈值内一致；实际顶点误差仍单独列出。`changed` 需要检查具体字段，不能直接等同于视觉损失。\n\n| 起点 | 路径 | 已完成步骤 | 首次变化/中断 | 对原始基线最大顶点偏移 |\n| --- | --- | --- | --- | --- |\n",
        report["rounds"]
    );
    for route in report["routes"].as_array().unwrap() {
        let steps = route["steps"].as_array().unwrap();
        let completed = steps.iter().filter(|s| s["status"] != "blocked").count();
        let first = steps
            .iter()
            .find(|s| s["status"] != "stable")
            .map(|s| {
                format!(
                    "{}: {}",
                    s["step"].as_str().unwrap(),
                    s["status"].as_str().unwrap()
                )
            })
            .unwrap_or_else(|| "无".into());
        let max = steps
            .iter()
            .filter_map(|s| s["baseline"]["geometry"]["max_vertex_error_px"].as_f64())
            .fold(0_f64, f64::max);
        text.push_str(&format!(
            "| {} | {} | {} | {} | {:.8}px |\n",
            route["seed"].as_str().unwrap(),
            route["route"].as_str().unwrap(),
            completed,
            first,
            max
        ));
    }
    text.push_str("\n## 输入原有诊断\n\n");
    for input in report["inputs"].as_array().unwrap() {
        text.push_str(&format!(
            "- {}：{} 项；详见 report.json 的 inputs。\n",
            input["seed"].as_str().unwrap(),
            input["diagnostics"].as_array().unwrap().len()
        ));
    }
    text.push_str("\n## 首次差异与错误\n\n");
    for route in report["routes"].as_array().unwrap() {
        for step in route["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["status"] != "stable")
            .take(2)
        {
            text.push_str(&format!("- {}：", step["step"].as_str().unwrap()));
            if let Some(error) = step["error"].as_str() {
                let codes = diagnostic_counts(error);
                if codes.is_empty() {
                    text.extend(error.chars().take(240));
                } else {
                    text.push_str(&format!("诊断计数 {codes:?}；完整错误见 report.json。"));
                }
            } else {
                text.push_str(&format!(
                    "文档字段 {}；行为差异 {} 处；首批具体值见 report.json。",
                    step["previous"]["document"]["fields"], step["previous"]["behavior"]["changed"]
                ));
            }
            text.push('\n');
        }
    }
    text.push_str(&format!(
        "\n源素材未变化：{}。整体无损：{}。\n",
        report["source_unchanged"], report["lossless"]
    ));
    text
}

#[test]
#[ignore = "Licensed Mao multi-format audit; run explicitly with --ignored --nocapture"]
fn mao_repeated_format_transfers_report_document_loss() -> Result<()> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao")
        .canonicalize()?;
    let original_files = fixture_hashes(&source)?;
    let engineering = super::transfer::portable_mao_fixture()?;
    let rounds: usize = std::env::var("ZIPZI_ROUNDTRIP_ROUNDS")
        .unwrap_or_else(|_| "5".into())
        .parse()?;
    ensure!((1..=20).contains(&rounds), "Use 1–20 rounds");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let output = std::env::var_os("ZIPZI_ROUNDTRIP_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../target/mao-roundtrip-{stamp}"))
        });
    ensure!(
        !output.exists(),
        "Choose a fresh report directory: {}",
        output.display()
    );
    fs::create_dir_all(&output)?;
    let keep = std::env::var_os("ZIPZI_ROUNDTRIP_KEEP_ARTIFACTS").is_some();
    let temporary = tempfile::tempdir()?;
    let work = if keep {
        output.join("artifacts")
    } else {
        temporary.path().to_path_buf()
    };
    fs::create_dir_all(&work)?;
    let mut report = json!({"rounds":rounds,"source":source,"source_sha256":original_files,"tolerances":{"document_absolute":1e-6,"behavior_absolute":1e-5,"vertex_px":0.1},"normalization":["generated UUIDs","asset source locations","PNG encoding; decoded RGBA compared","unordered binding/constraint lists aligned by target and driver"],"inputs":[],"routes":[],"aliases":[]});
    let mut persistence_errors = Vec::new();
    for (seed, entry, initial_format, alias) in [
        (
            "engineering",
            engineering.path().join("mao_pro_t06.cmo3"),
            "C",
            engineering.path().join("mao_pro_t06.can3"),
        ),
        (
            "runtime",
            source.join("runtime/mao_pro.model3.json"),
            "R",
            source.join("runtime/mao_pro.moc3"),
        ),
    ] {
        eprintln!("Mao audit: loading {seed}");
        let (baseline_session, diagnostics) = load_with_diagnostics(&entry, initial_format)?;
        report["inputs"]
            .as_array_mut()
            .unwrap()
            .push(json!({"seed":seed,"diagnostics":diagnostics}));
        let cases = poses(baseline_session.document());
        let baseline = capture(&baseline_session, &cases)?;
        fs::write(
            output.join(format!("{seed}-baseline.json")),
            serde_json::to_vec(&baseline.document)?,
        )?;
        let alias_capture = capture(&load(&alias, initial_format)?, &cases)?;
        let alias_delta = changes(&baseline, &alias_capture);
        report["aliases"]
            .as_array_mut()
            .unwrap()
            .push(json!({"seed":seed,"comparison":alias_delta}));
        fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        drop(alias_capture);
        for (route_name, cycle) in [
            ("zipzi", vec!["K"]),
            ("cubism", vec!["C"]),
            ("runtime", vec!["R"]),
            ("mixed-c-r-k", vec!["C", "R", "K"]),
            ("mixed-r-c-k", vec!["R", "C", "K"]),
        ] {
            let mut current = load(&entry, initial_format)?;
            let mut previous = Some(capture(&current, &cases)?);
            let mut steps = Vec::new();
            'route: for round in 1..=rounds {
                for (index, format) in cycle.iter().enumerate() {
                    let step_name = format!("{seed}-{route_name}-{round}-{index}-{format}");
                    eprintln!("Mao audit: {step_name}");
                    let destination = work.join(&step_name);
                    let before = zipzi_project::encode_project(current.document())
                        .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
                    match transfer(&mut current, format, &destination) {
                        Ok(next) => {
                            // Export operations must not mutate their source document;
                            // saving may relocate asset sources and hashes by design.
                            if *format != "K" {
                                let after = zipzi_project::encode_project(current.document())
                                    .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
                                ensure!(
                                    before == after,
                                    "{step_name}: exporter mutated its source"
                                );
                            }
                            let snapshot =
                                capture(&next, &cases).with_context(|| step_name.clone())?;
                            let delta = changes(previous.as_ref().unwrap_or(&baseline), &snapshot);
                            let cumulative = changes(&baseline, &snapshot);
                            let before_capture = previous.as_ref().unwrap_or(&baseline);
                            let exact = !changed(&delta)
                                && before_capture.document == snapshot.document
                                && before_capture.behavior == snapshot.behavior
                                && before_capture.geometry.metadata == snapshot.geometry.metadata
                                && delta["geometry"]["max_vertex_error_px"] == 0.;
                            if *format == "K" && !exact {
                                persistence_errors.push(step_name.clone());
                            }
                            let step = json!({"step":step_name,"format":format,"round":round,"status":if changed(&delta){"changed"}else{"stable"},"native_exact":if *format=="K" {json!(exact)}else{Value::Null},"previous":delta,"baseline":cumulative});
                            steps.push(step);
                            previous = Some(snapshot);
                            current = next;
                        }
                        Err(error) => {
                            if error.to_string() != "reimport" {
                                ensure!(
                                    !destination.exists(),
                                    "{step_name}: failed publication left partial output"
                                );
                            }
                            steps.push(json!({"step":step_name,"format":format,"round":round,"status":"blocked","error":format!("{error:#}")}));
                            break 'route;
                        }
                    }
                }
            }
            if let Some(final_capture) = previous {
                fs::write(
                    output.join(format!("{seed}-{route_name}-final.json")),
                    serde_json::to_vec(&final_capture.document)?,
                )?;
            }
            drop(current);
            if !keep {
                for step in &steps {
                    let path = work.join(step["step"].as_str().unwrap());
                    if path.is_dir() {
                        fs::remove_dir_all(path)?;
                    }
                }
            }
            report["routes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"seed":seed,"route":route_name,"steps":steps}));
            fs::write(
                output.join("report.json"),
                serde_json::to_vec_pretty(&report)?,
            )?;
        }
    }
    report["source_unchanged"] = json!(fixture_hashes(&source)? == original_files);
    report["persistence_errors"] = json!(persistence_errors);
    report["lossless"] = json!(
        report["aliases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|alias| !changed(&alias["comparison"]))
            && report["routes"].as_array().unwrap().iter().all(|route| {
                route["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|step| step["status"] == "stable")
            })
    );
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(output.join("report.md"), markdown_report(&report))?;
    eprintln!(
        "Mao audit report: {} (lossless={})",
        output.join("report.json").display(),
        report["lossless"]
    );
    ensure!(
        report["source_unchanged"] == true,
        "Licensed fixture changed during audit"
    );
    ensure!(
        persistence_errors.is_empty(),
        "Zipzi persistence changes: {persistence_errors:?}"
    );
    if std::env::var_os("ZIPZI_ROUNDTRIP_ASSERT_LOSSLESS").is_some() {
        ensure!(
            report["lossless"] == true,
            "Conversion loss detected; see {}",
            output.join("report.json").display()
        );
    }
    Ok(())
}

#[test]
fn diagnostic_summary_retains_codes_containing_digits() {
    let error = "publish: CUBISM_TRANSFER: MODEL3_METADATA: detail\nCAN3_FRAME_QUANTIZATION: a\nCAN3_FRAME_QUANTIZATION: b\nDISPLAY_METADATA: c";
    assert_eq!(
        diagnostic_counts(error),
        BTreeMap::from([
            ("MODEL3_METADATA", 1),
            ("CAN3_FRAME_QUANTIZATION", 2),
            ("DISPLAY_METADATA", 1),
        ])
    );
}

#[test]
fn comparison_detects_removed_expressions_blend_changes_and_numeric_drift() {
    let baseline = json!({"expressions":[{"name":"smile","blend":"Add","value":0.25}],"physics":{"gravity":-1.}});
    assert_eq!(difference(&baseline, &baseline, 1e-6).changed, 0);
    assert!(
        difference(
            &baseline,
            &json!({"expressions":[],"physics":{"gravity":-1.}}),
            1e-6
        )
        .changed
            > 0
    );
    assert!(difference(&baseline,&json!({"expressions":[{"name":"smile","blend":"Multiply","value":0.25}],"physics":{"gravity":-1.}}),1e-6).changed>0);
    assert!(difference(&baseline,&json!({"expressions":[{"name":"smile","blend":"Add","value":0.25}],"physics":{"gravity":-1.01}}),1e-6).changed>0);
}
