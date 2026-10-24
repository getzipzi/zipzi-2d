//! Real Physics inspector controls, with candidate values and history verified.
use super::*;
use crate::properties::physics::{NumberField, field_key};
use zipzi_editor_core::{CommandId, PanelKind, property::physics as schema};

async fn reveal(cx: &mut AsyncWindowContext, id: &str) -> anyhow::Result<Bounds<Pixels>> {
    cx.update(|w, cx| w.blur(cx))?;
    for attempt in 0..100 {
        let found = cx.update(|w, cx| -> anyhow::Result<_> {
            let viewport = w
                .try_find("physics-properties")
                .context("Physics inspector missing")?
                .bounds();
            let bounds = w.try_find(id.to_owned()).map(|e| e.bounds());
            if bounds.is_some_and(|b| b.top() >= viewport.top() && b.bottom() <= viewport.bottom())
            {
                return Ok(bounds);
            }
            let dy = if attempt == 0 && bounds.is_none() {
                100_000.
            } else if bounds.is_some_and(|b| b.top() < viewport.top()) {
                40.
            } else {
                -40.
            };
            dispatch_pointer(
                w,
                ScrollWheelEvent {
                    position: viewport.center(),
                    delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            Ok(None)
        })??;
        if let Some(bounds) = found {
            return Ok(bounds);
        }
        pause(cx, 25).await;
    }
    anyhow::bail!("could not reveal Physics control {id}")
}

fn field(case: &str) -> Option<NumberField> {
    Some(match case {
        "physics_input_weight_text" | "physics_input_weight_drag" => NumberField::InputWeight(0),
        "physics_output_weight_text" | "physics_output_weight_drag" => NumberField::OutputWeight(0),
        "physics_output_scale_text" => NumberField::OutputScale(0),
        "physics_node_radius_text" => NumberField::Vertex(1, 0),
        "physics_node_mobility_text" | "physics_node_mobility_drag" => NumberField::Vertex(1, 1),
        "physics_normalization_text" => NumberField::Normalization(false, 1),
        _ => return None,
    })
}

fn observed(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    object: &ObjectRef,
    key: PropertyKey,
) -> anyhow::Result<PropertyValue> {
    if key == schema::NAME.key {
        return value(view, cx, object, key);
    }
    // property_values intentionally reads committed data. Physics definition reads
    // the displayed candidate model, which is what a scrub must actually change.
    view.update_in(cx, |v, _, _| -> anyhow::Result<_> {
        let asset = v.editor.document().unwrap().physics_definition().unwrap();
        let rig = asset
            .data
            .settings
            .iter()
            .find(|g| g.id == object.id)
            .unwrap();
        Ok(match key.name() {
            "input_weight" => PropertyValue::Float(rig.inputs[0].weight),
            "output_weight" => PropertyValue::Float(rig.outputs[0].weight),
            "output_scale" => PropertyValue::Float(rig.outputs[0].scale),
            "radius" => PropertyValue::Float(rig.vertices[1].radius),
            "mobility" => PropertyValue::Float(rig.vertices[1].mobility),
            "position_default" => PropertyValue::Float(rig.normalization.position.default),
            "input_reflect" => PropertyValue::Bool(rig.inputs[0].reflect),
            _ => anyhow::bail!("unknown Physics observation field: {}", key.name()),
        })
    })?
}

pub(super) async fn run(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
    cases: &[(usize, &str)],
) -> anyhow::Result<()> {
    let (index, object) = view.update_in(cx, |v, w, cx| -> anyhow::Result<_> {
        let asset = v.editor.document().unwrap().physics_definition().context("model has no Physics definition")?;
        let (index, rig) = asset.data.settings.iter().enumerate()
            .find(|(_, rig)| !rig.inputs.is_empty() && !rig.outputs.is_empty() && rig.vertices.len() > 1)
            .context("Physics fixture requires input, output and a non-root node")?;
        let object = ObjectRef::new(ObjectKind::PhysicsGroup, rig.id.clone());
        eprintln!("Physics benchmark fixture: {}", serde_json::json!({
            "group": rig.id, "index": index, "input": rig.inputs[0].id,
            "output": rig.outputs[0].id, "node": rig.vertices[1].id,
            "inputs": rig.inputs.len(), "outputs": rig.outputs.len(), "nodes": rig.vertices.len(),
        }));
        v.run_command(CommandId::AnimationWorkspace, w, cx);
        v.dock.as_ref().unwrap().show(PanelKind::AnimationPhysics, None, w, cx);
        Ok((index, object))
    })??;
    pause(cx, 300).await;
    cx.update(|w, cx| -> anyhow::Result<()> {
        let p = w
            .try_find(("physics-group-row", index))
            .context("Physics group row missing")?
            .bounds()
            .center();
        pointer(w, p, 0, cx);
        pointer(w, p, 2, cx);
        Ok(())
    })??;
    pause(cx, 500).await;
    for &(index, case) in cases {
        eprintln!("Physics workload {index}: {case}");
        let (key, low, high) = view.update_in(cx, |v, _, _| -> anyhow::Result<_> {
            let model_read_guard_101 = v.editor.document().unwrap().physics_definition().unwrap();
            let rig = model_read_guard_101
                .data
                .settings
                .iter()
                .find(|g| g.id == object.id)
                .unwrap();
            let key = if case == "physics_name" || case == "physics_idle" {
                schema::NAME.key
            } else if case == "physics_input_reflect" {
                field_key("input_reflect", 0, rig)
            } else {
                field(case)
                    .context("unknown Physics field")?
                    .spec(rig)
                    .unwrap()
                    .key
            };
            let (low, high) = if case == "physics_normalization_text" {
                let range = rig.normalization.position;
                (
                    range.minimum + (range.maximum - range.minimum) * 0.35,
                    range.minimum + (range.maximum - range.minimum) * 0.65,
                )
            } else if case == "physics_node_mobility_text" {
                (0.3, 0.7)
            } else if case == "physics_output_scale_text" {
                (0.8, 1.2)
            } else if case == "physics_node_radius_text" {
                (4., 8.)
            } else {
                (25., 75.)
            };
            Ok((key, low, high))
        })??;
        let id = if let Some(field) = field(case) {
            format!("physics-number-{field:?}")
        } else if case == "physics_input_reflect" {
            format!("property-toggle-{}", key.identity())
        } else {
            "property-input-name".into()
        };
        if case == "physics_normalization_text" {
            reveal(cx, "physics-fold-physics-normalization").await?;
            cx.update(|w, cx| click(w, "physics-fold-physics-normalization", cx))??;
            pause(cx, 60).await;
        }
        let mut bounds = reveal(cx, &id).await?;
        if field(case).is_some() {
            bounds = cx.update(|w, _| w.within(id.clone()).find("slide-number-input").bounds())?;
        }
        pause(cx, 100).await;
        let history = view.update_in(cx, |v, _, _| {
            v.editor.document().unwrap().history_lengths().0
        })?;
        let initial = observed(view, cx, &object, key)?;
        let drag = case.ends_with("_drag");
        let position = |i: usize| bounds.center() + point(px(-((8 + i * 7 % 64) as f32)), px(0.));
        if drag {
            view.update_in(cx, |v, _, cx| {
                v.dock
                    .as_ref()
                    .unwrap()
                    .properties
                    .update(cx, |p, cx| p.benchmark_isolate_pointers(cx))
            })?;
            cx.update(|w, cx| {
                pointer(w, bounds.center(), 3, cx);
                pointer(w, bounds.center(), 0, cx);
            })?;
        }
        let phase = frame_trace::next_id();
        frame_trace::emit(
            phase,
            "properties_phase",
            &[("case", index as u64), ("active", 1)],
        );
        let interval = Duration::from_secs_f64(1. / config.hz as f64);
        let mut deadline = Instant::now() + interval;
        let mut previous = initial.clone();
        let mut changed = 0;
        for sample in 0..config.warmup + config.samples {
            cx.background_executor()
                .timer(deadline.saturating_duration_since(Instant::now()))
                .await;
            let late = Instant::now()
                .saturating_duration_since(deadline)
                .as_nanos() as u64;
            let scope = frame_trace::InputScope::begin(210 + index as u64);
            let tick = scope.id();
            frame_trace::emit(
                tick,
                "properties_tick",
                &[
                    ("case", index as u64),
                    ("sample", sample as u64),
                    ("warmup", u64::from(sample < config.warmup)),
                    ("timer_late_ns", late),
                ],
            );
            cx.update(|w, _| -> anyhow::Result<()> {
                ensure!(
                    w.is_visible() && w.is_window_active(),
                    "Physics benchmark lost focus"
                );
                Ok(())
            })??;
            if drag {
                cx.update(|w, cx| pointer(w, position(sample + 1), 1, cx))?;
            } else if case == "physics_name" {
                fill(cx, &id, &format!("PhysicsBench{sample:05}"), true).await?;
            } else if case == "physics_input_reflect" {
                cx.update(|w, cx| click(w, &id, cx))??;
            } else if case != "physics_idle" {
                fill(
                    cx,
                    &id,
                    &format!("{:.4}", if sample % 2 == 0 { low } else { high }),
                    true,
                )
                .await?;
            }
            drop(scope);
            let next = observed(view, cx, &object, key)?;
            let did_change = next != previous;
            changed += u64::from(sample >= config.warmup && did_change);
            frame_trace::emit(
                tick,
                "properties_checked",
                &[("changed", u64::from(did_change))],
            );
            if !drag && field(case).is_some() {
                let PropertyValue::Float(actual) = next else {
                    anyhow::bail!("Physics number is not numeric");
                };
                let expected = if sample % 2 == 0 { low } else { high };
                ensure!(
                    (actual - expected).abs() < 0.001,
                    "{case}: text did not reach requested value {expected}, got {actual}"
                );
            }
            if drag {
                ensure!(
                    view.update_in(cx, |v, _, _| v
                        .editor
                        .document()
                        .unwrap()
                        .history_lengths()
                        .0)?
                        == history,
                    "Physics drag committed before release"
                );
            }
            cx.update(|w, _| -> anyhow::Result<()> {
                ensure!(
                    w.try_find("physics-error").is_none(),
                    "Physics control reported an error"
                );
                Ok(())
            })??;
            previous = next;
            deadline = (deadline + interval).max(Instant::now());
        }
        if drag {
            cx.update(|w, cx| pointer(w, position(config.samples + config.warmup), 2, cx))?;
        }
        pause(cx, 250).await;
        frame_trace::emit(
            phase,
            "properties_phase_end",
            &[("case", index as u64), ("changed_samples", changed)],
        );
        ensure!(
            case == "physics_idle" || changed > 0,
            "{case}: no value changes"
        );
        view.update_in(cx, |v, _, cx| -> anyhow::Result<()> {
            let final_history = v.editor.document().unwrap().history_lengths().0;
            ensure!(
                !drag || final_history == history + 1,
                "Physics gesture must commit exactly once"
            );
            while v.editor.document().unwrap().history_lengths().0 > history {
                v.editor.execute(EditorCommand::Undo)?;
            }
            v.redraw(cx);
            Ok(())
        })??;
        ensure!(
            observed(view, cx, &object, key)? == initial,
            "{case}: Undo did not restore the initial value"
        );
        if case == "physics_normalization_text" {
            reveal(cx, "physics-fold-physics-normalization").await?;
            cx.update(|w, cx| click(w, "physics-fold-physics-normalization", cx))??;
        }
        pause(cx, 150).await;
    }
    pause(cx, 1500).await;
    Ok(())
}
