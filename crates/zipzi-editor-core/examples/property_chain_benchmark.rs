//! CPU-side editing benchmark. Run through scripts/property-chain-benchmark.py
//! for paired uninstrumented/instrumented Release binaries and provenance.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{hint::black_box, io::Write, path::PathBuf, time::Instant};
use zipzi_editor_core::{property as schema, *};

struct Samples {
    rows: Vec<Value>,
    warmup: usize,
    samples: usize,
    updates: usize,
    retain_index: bool,
}

impl Samples {
    fn measure<T>(
        &mut self,
        case: &str,
        phase: &str,
        iteration: usize,
        update: usize,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        #[cfg(feature = "bench-instrumentation")]
        let capture = zipzi_core::benchmark::Capture::start();
        let start = Instant::now();
        let result = {
            #[cfg(feature = "bench-instrumentation")]
            let _root = zipzi_core::benchmark::span("benchmark.operation");
            operation()
        };
        let wall_ns = start.elapsed().as_nanos() as u64;
        #[allow(unused_mut)]
        let mut stages = serde_json::Map::new();
        #[cfg(feature = "bench-instrumentation")]
        for (name, stage) in capture.finish() {
            stages.insert(
                name.into(),
                json!({"calls":stage.calls, "inclusive_ns":stage.inclusive_ns, "self_ns":stage.self_ns}),
            );
        }
        let value = result.with_context(|| format!("{case}/{phase}, iteration {iteration}"))?;
        if iteration >= self.warmup {
            self.rows.push(json!({
                "type":"sample", "case":case, "phase":phase,
                "sample":iteration-self.warmup, "update":update,
                "wall_ns":wall_ns, "stages":stages,
            }));
        }
        Ok(value)
    }

    fn count(&self) -> usize {
        self.warmup + self.samples
    }
}

fn patch(
    editor: &Editor,
    object: &ObjectRef,
    spec: PropertySpec,
    value: PropertyValue,
) -> Result<PropertyPatch> {
    let document = editor.document().unwrap();
    let values = document.property_values(object)?;
    Ok(PropertyPatch {
        scope: if spec.keyframed {
            document.property_scope(object)
        } else {
            PropertyScope::Object
        },
        expected: values
            .into_iter()
            .filter(|(key, _)| {
                *key == spec.key || property_spec(*key).is_some_and(|other| other.same_group(&spec))
            })
            .collect(),
        changes: [(spec.key, value)].into(),
    })
}

fn committed(editor: &Editor, object: &ObjectRef, spec: PropertySpec) -> Result<PropertyValue> {
    Ok(editor.document().unwrap().property_values(object)?[&spec.key].clone())
}

fn visible(editor: &Editor, object: &ObjectRef, spec: PropertySpec) -> Result<PropertyValue> {
    if object.kind == ObjectKind::PhysicsGroup {
        let asset = editor.document().unwrap().physics_definition().unwrap();
        let group = schema::physics::group(asset, object)?;
        let id = spec
            .key
            .entity_id()
            .context("missing physics node address")?;
        let node = group
            .vertices
            .iter()
            .find(|node| node.id == id)
            .context("missing physics node")?;
        return Ok(PropertyValue::Float(node.mobility));
    }
    let index = editor.document().unwrap().object_index();
    spec.read(
        &index
            .get(object)
            .context("missing candidate object")?
            .properties,
    )
}

// Called as a separately timed phase. It includes panel index consumption and
// CPU scene/evaluation, but no window layout, GPU rendering or display delivery.
fn preview(editor: &mut Editor, evaluator: &mut AnimationEvaluator) -> Result<SceneSnapshot> {
    black_box(editor.document().unwrap().object_index());
    if editor.workspace() == WorkspaceKind::Modeling {
        let result = editor.preview()?;
        ensure!(
            !result.scene.runtime_frame.drawables.is_empty(),
            "empty modeling frame"
        );
        ensure!(
            editor.present_frame(result.canvas.context("missing modeling canvas")?),
            "modeling frame was rejected"
        );
        Ok(result.scene)
    } else {
        let request = editor
            .animation_request()
            .context("no changed animation request")?;
        let frame = evaluator.evaluate(&request, |_, _| true)?;
        ensure!(
            !frame.scene.runtime_frame.drawables.is_empty(),
            "empty animation frame"
        );
        ensure!(
            editor.present_animation(frame.identity, frame.snapshot),
            "animation frame was rejected"
        );
        Ok(frame.scene)
    }
}

fn verify_mesh_preview(
    baseline: &SceneSnapshot,
    scene: &SceneSnapshot,
    object: &ObjectRef,
    original: &PropertyValue,
    value: &PropertyValue,
) -> Result<()> {
    let before = baseline
        .runtime_frame
        .drawables
        .iter()
        .find(|d| d.id == object.id)
        .context("missing original drawable")?;
    let after = scene
        .runtime_frame
        .drawables
        .iter()
        .find(|d| d.id == object.id)
        .context("missing candidate drawable")?;
    let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
    match (original, value) {
        (PropertyValue::Float(old), PropertyValue::Float(new)) => {
            ensure!(
                *old > 0. && before.opacity > 0.,
                "opacity target cannot verify visual changes"
            );
            ensure!(
                close(after.opacity, before.opacity * new / old),
                "incorrect evaluated opacity"
            );
        }
        (PropertyValue::Color(old), PropertyValue::Color(new)) => {
            ensure!(
                old.iter().all(|c| *c > 0.),
                "multiply target cannot verify visual changes"
            );
            for c in 0..3 {
                ensure!(
                    close(
                        after.multiply_color[c],
                        before.multiply_color[c] * new[c] / old[c]
                    ),
                    "incorrect evaluated multiply color"
                );
            }
        }
        _ => anyhow::bail!("unsupported mesh preview verification"),
    }
    Ok(())
}

fn commit_case(
    samples: &mut Samples,
    editor: &mut Editor,
    case: &str,
    object: &ObjectRef,
    spec: PropertySpec,
    value: PropertyValue,
) -> Result<()> {
    let original = committed(editor, object, spec)?;
    ensure!(original != value, "{case} would be a no-op");
    let mut evaluator = AnimationEvaluator::default();
    for iteration in 0..samples.count() {
        let history = editor.document().unwrap().history_lengths();
        let changes = patch(editor, object, spec, value.clone())?;
        let context = editor.editing_context_token().unwrap();
        // Panels may still hold the previous immutable projection during edits.
        let reader = samples
            .retain_index
            .then(|| editor.document().unwrap().object_index());
        samples.measure(case, "commit", iteration, 0, || {
            editor.commit_properties(context, object.clone(), changes)
        })?;
        black_box(&reader);
        ensure!(
            committed(editor, object, spec)? == value,
            "{case}: incorrect committed value"
        );
        ensure!(
            editor.document().unwrap().history_lengths() == (history.0 + 1, 0),
            "{case}: expected one history entry"
        );
        samples.measure(case, "after_commit", iteration, 0, || {
            preview(editor, &mut evaluator)
        })?;
        drop(reader);
        // Restoration/undo is verified but deliberately outside the timed edit.
        editor.execute(EditorCommand::Undo)?;
        ensure!(
            committed(editor, object, spec)? == original,
            "{case}: undo failed"
        );
        preview(editor, &mut evaluator)?;
    }
    Ok(())
}

fn drag_case(
    samples: &mut Samples,
    editor: &mut Editor,
    case: &str,
    object: &ObjectRef,
    spec: PropertySpec,
    values: [PropertyValue; 2],
) -> Result<()> {
    let original = committed(editor, object, spec)?;
    ensure!(
        values[0] != values[1] && values.iter().all(|v| *v != original),
        "{case}: no-op inputs"
    );
    let initial = patch(editor, object, spec, values[0].clone())?;
    let mut evaluator = AnimationEvaluator::default();
    let mesh_baseline = if object.kind == ObjectKind::Mesh {
        Some(editor.preview()?.scene)
    } else {
        None
    };
    for iteration in 0..samples.count() {
        let history = editor.document().unwrap().history_lengths();
        let context = editor.editing_context_token().unwrap();
        let token = samples.measure(case, "begin", iteration, 0, || {
            editor.begin_property_edit(context, object.clone(), &initial)
        })?;
        for update in 0..samples.updates {
            let value = values[update % 2].clone();
            let mut changes = initial.clone();
            changes.changes.insert(spec.key, value.clone());
            let reader = samples
                .retain_index
                .then(|| editor.document().unwrap().object_index());
            samples.measure(case, "update", iteration, update, || {
                editor.update_property_edit(&token, &changes)
            })?;
            black_box(&reader);
            if object.kind != ObjectKind::Mesh {
                ensure!(
                    visible(editor, object, spec)? == value,
                    "{case}: candidate value not applied"
                );
            }
            ensure!(
                editor.document().unwrap().history_lengths() == history,
                "{case}: preview published history"
            );
            let scene = samples.measure(case, "preview", iteration, update, || {
                preview(editor, &mut evaluator)
            })?;
            if let Some(baseline) = &mesh_baseline {
                verify_mesh_preview(baseline, &scene, object, &original, &value)?;
            }
            drop(reader);
        }
        if iteration % 2 == 0 {
            samples.measure(case, "finish", iteration, 0, || {
                editor.finish_property_edit(&token)
            })?;
            ensure!(
                committed(editor, object, spec)? == values[(samples.updates - 1) % 2],
                "{case}: finish lost candidate"
            );
            ensure!(
                editor.document().unwrap().history_lengths() == (history.0 + 1, 0),
                "{case}: finish history"
            );
            samples.measure(case, "after_finish", iteration, 0, || {
                preview(editor, &mut evaluator)
            })?;
            editor.execute(EditorCommand::Undo)?;
            ensure!(
                committed(editor, object, spec)? == original,
                "{case}: undo failed"
            );
            preview(editor, &mut evaluator)?;
        } else {
            samples.measure(case, "cancel", iteration, 0, || {
                editor.cancel_property_edit(&token);
                Ok(())
            })?;
            ensure!(
                committed(editor, object, spec)? == original,
                "{case}: cancel failed"
            );
            ensure!(
                editor.document().unwrap().history_lengths() == history,
                "{case}: cancel changed history"
            );
            samples.measure(case, "after_cancel", iteration, 0, || {
                preview(editor, &mut evaluator)
            })?;
        }
        ensure!(
            !editor.property_edit_owned(&token),
            "{case}: gesture remained active"
        );
    }
    Ok(())
}

fn read_case(samples: &mut Samples, editor: &Editor, case: &str, object: &ObjectRef) -> Result<()> {
    let expected = editor.document().unwrap().property_values(object)?;
    for iteration in 0..samples.count() {
        let values = samples.measure(case, "read", iteration, 0, || {
            editor.document().unwrap().property_values(object)
        })?;
        ensure!(black_box(values) == expected, "unstable property read");
    }
    Ok(())
}

fn timeline_case(samples: &mut Samples, editor: &mut Editor, clip_id: &str) -> Result<()> {
    let case = "timeline_value_drag";
    let document = editor.active_id().unwrap();
    let clip = editor
        .document()
        .unwrap()
        .motion_definition(clip_id)
        .unwrap();
    let (track, parameter) = clip
        .tracks
        .iter()
        .find_map(|track| {
            let zipzi_core::document::MotionTrackTarget::Parameter { parameter_id } = &track.target
            else {
                return None;
            };
            editor
                .document()
                .unwrap()
                .parameters()
                .into_iter()
                .find(|p| p.id == *parameter_id && p.minimum < p.maximum)
                .map(|p| (track, p))
        })
        .context("no writable timeline parameter track")?;
    let key = TimelineKeyRef {
        clip_id: clip_id.into(),
        track_id: track.id.clone(),
        key_id: track.initial.id.clone(),
    };
    let original = track.initial.value;
    let values = [0.37, 0.63]
        .map(|fraction| parameter.minimum + (parameter.maximum - parameter.minimum) * fraction);
    ensure!(
        values.iter().all(|v| *v != original),
        "no-op timeline input"
    );
    let channel = TimelineChannelId::Parameter(parameter.id);
    editor.seek_timeline(document, track.initial.time)?;
    editor.update_timeline_view(document, |view| view.snap = false)?;
    let mut evaluator = AnimationEvaluator::default();
    for iteration in 0..samples.count() {
        let history = editor.document().unwrap().history_lengths();
        let token = samples.measure(case, "begin", iteration, 0, || {
            editor.begin_timeline_edit(document, "Benchmark channel value")
        })?;
        for update in 0..samples.updates {
            let reader = editor.timeline_projection().unwrap();
            let value = values[update % 2];
            let command = TimelineCommand::WriteValue {
                channel: channel.clone(),
                track_id: Some(key.track_id.clone()),
                selected_key: Some(key.clone()),
                value,
            };
            samples.measure(case, "update", iteration, update, || {
                editor.update_timeline_edit(document, token, command)
            })?;
            black_box(&reader);
            ensure!(
                editor
                    .timeline_projection()
                    .unwrap()
                    .key(&key)
                    .unwrap()
                    .value
                    == value,
                "timeline candidate mismatch"
            );
            ensure!(
                editor.document().unwrap().history_lengths() == history,
                "timeline update published history"
            );
            samples.measure(case, "preview", iteration, update, || {
                preview(editor, &mut evaluator)
            })?;
        }
        if iteration % 2 == 0 {
            samples.measure(case, "finish", iteration, 0, || {
                editor.finish_timeline_edit(document, token)
            })?;
            ensure!(
                editor
                    .document()
                    .unwrap()
                    .motion_definition(clip_id)
                    .unwrap()
                    .tracks
                    .iter()
                    .find(|t| t.id == key.track_id)
                    .unwrap()
                    .initial
                    .value
                    == values[(samples.updates - 1) % 2],
                "timeline commit mismatch"
            );
            ensure!(
                editor.document().unwrap().history_lengths() == (history.0 + 1, 0),
                "timeline finish history"
            );
            samples.measure(case, "after_finish", iteration, 0, || {
                preview(editor, &mut evaluator)
            })?;
            editor.execute(EditorCommand::Undo)?;
            preview(editor, &mut evaluator)?;
        } else {
            samples.measure(case, "cancel", iteration, 0, || {
                editor.cancel_timeline_edit();
                Ok(())
            })?;
            ensure!(
                editor.document().unwrap().history_lengths() == history,
                "timeline cancel history"
            );
            samples.measure(case, "after_cancel", iteration, 0, || {
                preview(editor, &mut evaluator)
            })?;
        }
        ensure!(
            editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value
                == original,
            "timeline undo/cancel failed"
        );
        ensure!(
            editor.timeline_edit().is_none(),
            "timeline edit remained active"
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "use --release for this benchmark");
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .context("MODEL [SAMPLES] [WARMUP] [UPDATES] [FILTER]")?,
    )
    .canonicalize()?;
    let mut samples = Samples {
        rows: vec![],
        samples: args.next().unwrap_or_else(|| "60".into()).parse()?,
        warmup: args.next().unwrap_or_else(|| "10".into()).parse()?,
        updates: args.next().unwrap_or_else(|| "12".into()).parse()?,
        retain_index: true,
    };
    let filter = args.next().unwrap_or_else(|| "all".into());
    let reader_policy = args.next().unwrap_or_else(|| "retained".into());
    ensure!(
        matches!(reader_policy.as_str(), "retained" | "unique"),
        "reader policy must be retained or unique"
    );
    samples.retain_index = reader_policy == "retained";
    ensure!(
        samples.samples >= 2 && samples.updates >= 2 && samples.warmup >= 1,
        "need >=2 samples/updates and >=1 warmup"
    );
    let load = if path.to_string_lossy().ends_with(".model3.json") {
        LoadRequest::Model3(path.clone())
    } else {
        LoadRequest::Open(path.clone())
    };
    let (document, diagnostics) = load.load()?;
    let mut editor = Editor::from_document(document);
    for kind in [ViewportKind::Modeling, ViewportKind::Animation] {
        editor.register_primary_viewport(ViewportId::new(), kind)?;
    }
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1100.,
        height: 740.,
        scale_factor: 1.,
    });
    let mut layers = editor.document().unwrap().layers();
    layers.sort_by_key(|l| l.vertices);
    let candidates: Vec<_> = layers
        .iter()
        .filter(|l| {
            editor
                .document()
                .unwrap()
                .property_scope(&ObjectRef::new(ObjectKind::Mesh, &l.id))
                != PropertyScope::InterpolatedMesh
        })
        .collect();
    let index = editor.document().unwrap().object_index();
    let mesh = candidates
        .iter()
        .find(|l| {
            index
                .get(&ObjectRef::new(ObjectKind::Mesh, &l.id))
                .is_some_and(|info| info.properties["runtime_id"] == "ArtMesh215")
        })
        .copied()
        .or_else(|| candidates.get(candidates.len() / 2).copied())
        .context("no editable mesh at default pose")?;
    let object = ObjectRef::new(ObjectKind::Mesh, &mesh.id);
    let parameter = editor
        .document()
        .unwrap()
        .parameters()
        .into_iter()
        .find(|p| p.minimum < p.maximum)
        .context("no parameter")?;
    let motions = editor
        .document()
        .unwrap()
        .animation_catalog()
        .motions
        .clone();
    let motion = motions.iter().max_by_key(|m| m.tracks);
    let expressions = editor
        .document()
        .unwrap()
        .animation_catalog()
        .expressions
        .clone();
    let expression = expressions.iter().max_by_key(|e| e.parameters);
    let physics = editor
        .document()
        .unwrap()
        .physics_definition()
        .and_then(|a| {
            a.data
                .settings
                .iter()
                .find(|g| g.vertices.len() > 1)
                .map(|g| ObjectRef::new(ObjectKind::PhysicsGroup, &g.id))
        });
    samples.rows.push(json!({
        "type":"metadata", "schema_version":1, "model":path,
        "instrumented":cfg!(feature="bench-instrumentation"),
        "samples":samples.samples, "warmup":samples.warmup, "updates":samples.updates,
        "meshes":layers.len(), "vertices":layers.iter().map(|l| l.vertices).sum::<usize>(),
        "triangles":layers.iter().map(|l| l.triangles).sum::<usize>(),
        "parameters":editor.document().unwrap().parameters().len(),
        "motions":motions.len(), "expressions":expressions.len(),
        "motion_tracks":motion.map(|m| m.tracks),
        "motion_keys":motion.and_then(|m| editor.document().unwrap().motion_definition(&m.selection.clip_id)).map(|c| c.tracks.iter().map(|t| t.keys().count()).sum::<usize>()),
        "target_mesh":{"id":object.id,"vertices":mesh.vertices,"scope":format!("{:?}",editor.document().unwrap().property_scope(&object))},
        "retained_previous_index":samples.retain_index, "preview_mode":"Editing", "diagnostics":diagnostics,
    }));
    drop(index);
    let wanted = |case: &str| filter == "all" || case.contains(&filter);
    macro_rules! run {
        ($name:literal, $operation:expr) => {
            if wanted($name) {
                eprintln!("{}", $name);
                $operation.with_context(|| $name)?;
            }
        };
    }
    editor.execute(EditorCommand::Navigate(object.clone()))?;
    preview(&mut editor, &mut AnimationEvaluator::default())?;
    run!(
        "mesh_read",
        read_case(&mut samples, &editor, "mesh_read", &object)
    );
    run!(
        "mesh_name_commit",
        commit_case(
            &mut samples,
            &mut editor,
            "mesh_name_commit",
            &object,
            schema::mesh::NAME,
            PropertyValue::Text("Benchmark renamed mesh".into())
        )
    );
    run!(
        "mesh_opacity_commit",
        commit_case(
            &mut samples,
            &mut editor,
            "mesh_opacity_commit",
            &object,
            schema::mesh::OPACITY,
            PropertyValue::Float(0.37)
        )
    );
    run!(
        "mesh_multiply_commit",
        commit_case(
            &mut samples,
            &mut editor,
            "mesh_multiply_commit",
            &object,
            schema::mesh::MULTIPLY,
            PropertyValue::Color([0.37, 0.62, 0.81])
        )
    );
    run!(
        "mesh_opacity_drag",
        drag_case(
            &mut samples,
            &mut editor,
            "mesh_opacity_drag",
            &object,
            schema::mesh::OPACITY,
            [PropertyValue::Float(0.37), PropertyValue::Float(0.63)]
        )
    );
    run!(
        "mesh_multiply_drag",
        drag_case(
            &mut samples,
            &mut editor,
            "mesh_multiply_drag",
            &object,
            schema::mesh::MULTIPLY,
            [
                PropertyValue::Color([0.37, 0.62, 0.81]),
                PropertyValue::Color([0.63, 0.48, 0.19])
            ]
        )
    );
    let target = ObjectRef::new(ObjectKind::Parameter, &parameter.id);
    let a = parameter.minimum + (parameter.maximum - parameter.minimum) * 0.37;
    let b = parameter.minimum + (parameter.maximum - parameter.minimum) * 0.63;
    run!(
        "parameter_default_commit",
        commit_case(
            &mut samples,
            &mut editor,
            "parameter_default_commit",
            &target,
            schema::parameter::DEFAULT_VALUE,
            PropertyValue::Float(a)
        )
    );
    run!(
        "parameter_default_drag",
        drag_case(
            &mut samples,
            &mut editor,
            "parameter_default_drag",
            &target,
            schema::parameter::DEFAULT_VALUE,
            [PropertyValue::Float(a), PropertyValue::Float(b)]
        )
    );
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1100.,
        height: 740.,
        scale_factor: 1.,
    });
    let document = editor.active_id().unwrap();
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Editing)?;
    if let Some(motion) = motion {
        editor.select_motion(document, motion.selection.clone())?;
        let target = ObjectRef::new(ObjectKind::Motion, &motion.selection.clip_id);
        run!(
            "motion_read",
            read_case(&mut samples, &editor, "motion_read", &target)
        );
        run!(
            "motion_fade_commit",
            commit_case(
                &mut samples,
                &mut editor,
                "motion_fade_commit",
                &target,
                schema::motion::FADE_IN,
                PropertyValue::OptionalFloat(Some(0.37))
            )
        );
        run!(
            "motion_fade_drag",
            drag_case(
                &mut samples,
                &mut editor,
                "motion_fade_drag",
                &target,
                schema::motion::FADE_IN,
                [
                    PropertyValue::OptionalFloat(Some(0.37)),
                    PropertyValue::OptionalFloat(Some(0.63))
                ]
            )
        );
        run!(
            "timeline_value_drag",
            timeline_case(&mut samples, &mut editor, &motion.selection.clip_id)
        );
    } else {
        samples
            .rows
            .push(json!({"type":"skip","case":"motion","reason":"no motion resource"}));
    }
    if let Some(expression) = expression {
        let target = ObjectRef::new(ObjectKind::Expression, &expression.id);
        run!(
            "expression_read",
            read_case(&mut samples, &editor, "expression_read", &target)
        );
        run!(
            "expression_fade_commit",
            commit_case(
                &mut samples,
                &mut editor,
                "expression_fade_commit",
                &target,
                schema::expression::FADE_IN,
                PropertyValue::OptionalFloat(Some(0.37))
            )
        );
        run!(
            "expression_fade_drag",
            drag_case(
                &mut samples,
                &mut editor,
                "expression_fade_drag",
                &target,
                schema::expression::FADE_IN,
                [
                    PropertyValue::OptionalFloat(Some(0.37)),
                    PropertyValue::OptionalFloat(Some(0.63))
                ]
            )
        );
    } else {
        samples
            .rows
            .push(json!({"type":"skip","case":"expression","reason":"no expression resource"}));
    }
    if let Some(target) = physics {
        let mut spec = schema::physics::MOBILITY;
        let asset = editor.document().unwrap().physics_definition().unwrap();
        let node = &schema::physics::group(asset, &target)?.vertices[1];
        spec.key = spec.key.for_entity(&node.id);
        run!(
            "physics_mobility_commit",
            commit_case(
                &mut samples,
                &mut editor,
                "physics_mobility_commit",
                &target,
                spec,
                PropertyValue::Float(0.37)
            )
        );
        run!(
            "physics_mobility_drag",
            drag_case(
                &mut samples,
                &mut editor,
                "physics_mobility_drag",
                &target,
                spec,
                [PropertyValue::Float(0.37), PropertyValue::Float(0.63)]
            )
        );
    } else {
        samples.rows.push(
            json!({"type":"skip","case":"physics","reason":"no physics group with two nodes"}),
        );
    }
    ensure!(
        samples.rows.iter().any(|r| r["type"] == "sample"),
        "no matching workload"
    );
    samples
        .rows
        .push(json!({"type":"complete","validated":true}));
    // All output is outside measurements. Failed runs never emit a complete file.
    let mut output = std::io::BufWriter::new(std::io::stdout().lock());
    for row in &samples.rows {
        serde_json::to_writer(&mut output, row)?;
        writeln!(output)?;
    }
    Ok(())
}
