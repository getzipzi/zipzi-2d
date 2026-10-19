//! Native properties input. Query identities only; never force a test frame or wait
//! for a canvas frame between continuous pointer samples.
use super::*;
use editor_core::{ObjectKind, ObjectRef, PropertyKey, PropertyValue, property::mesh};
use gpui_kit::{Bounds, KeyDownEvent, KeyUpEvent, Keystroke, test::TestWindowExt};

const CASES: &[&str] = &[
    "name",
    "runtime_id",
    "part",
    "masks",
    "opacity_text",
    "opacity_drag",
    "multiply_hex",
    "color_blend",
    "enabled",
    "picker_open_close",
    "picker_tabs",
    "picker_hex",
    "picker_invalid_hex",
    "picker_hue",
    "picker_sv",
    "picker_rgb_0_text",
    "picker_rgb_1_text",
    "picker_rgb_2_text",
    "picker_hsv_0_text",
    "picker_hsv_1_text",
    "picker_hsv_2_text",
    "picker_rgb_0_drag",
    "picker_rgb_1_drag",
    "picker_rgb_2_drag",
    "picker_hsv_0_drag",
    "picker_hsv_1_drag",
    "picker_hsv_2_drag",
    "picker_done",
    "picker_cancel",
    "multiply_reset",
    "picker_swatches",
    "idle",
    "picker_idle",
];

fn key(window: &mut Window, value: &str, cx: &mut App) {
    let keystroke = Keystroke::parse(value).unwrap();
    window.dispatch_event(
        KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
}

fn text(window: &mut Window, value: &str, cx: &mut App) {
    key(window, "cmd-a", cx);
    // Use the GPUI simulated IME, without TestWindowExt::input's per-character draws.
    for character in value.chars() {
        let _input = frame_trace::InputScope::begin(6);
        let value = character.to_string();
        let mut keystroke = Keystroke::parse(&value).unwrap();
        keystroke.key_char = Some(value);
        window.dispatch_keystroke(keystroke, cx);
    }
}

fn pointer(window: &mut Window, p: Point<Pixels>, kind: u8, cx: &mut App) {
    let event = match kind {
        0 => MouseDownEvent {
            position: p,
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
            modifiers: Default::default(),
        }
        .to_platform_input(),
        1 => MouseMoveEvent {
            position: p,
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        }
        .to_platform_input(),
        2 => MouseUpEvent {
            position: p,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: Default::default(),
        }
        .to_platform_input(),
        _ => MouseMoveEvent {
            position: p,
            pressed_button: None,
            modifiers: Default::default(),
        }
        .to_platform_input(),
    };
    dispatch_pointer(window, event, cx);
}

fn click(window: &mut Window, id: &str, cx: &mut App) -> anyhow::Result<()> {
    let p = window
        .try_find(id.to_owned())
        .with_context(|| format!("missing {id}"))?
        .bounds()
        .center();
    pointer(window, p, 3, cx);
    pointer(window, p, 0, cx);
    pointer(window, p, 2, cx);
    Ok(())
}

async fn pause(cx: &mut AsyncWindowContext, ms: u64) {
    cx.background_executor()
        .timer(Duration::from_millis(ms))
        .await;
}

async fn show(cx: &mut AsyncWindowContext, id: &str) -> anyhow::Result<Bounds<Pixels>> {
    // The same virtual property list and scroll path as screenshot acceptance.
    for attempt in 0..85 {
        let found = cx.update(|window, cx| -> anyhow::Result<_> {
            let viewport = window
                .try_find("object-properties")
                .context("properties list missing")?
                .bounds();
            let bounds = window.try_find(id.to_owned()).map(|e| e.bounds());
            if bounds.is_some_and(|b| b.top() >= viewport.top() && b.bottom() <= viewport.bottom())
            {
                return Ok(bounds);
            }
            let delta = if attempt == 0 && bounds.is_none() {
                100_000.
            } else if bounds.is_some_and(|b| b.top() < viewport.top()) {
                40.
            } else {
                -40.
            };
            dispatch_pointer(
                window,
                ScrollWheelEvent {
                    position: viewport.center(),
                    delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
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
    anyhow::bail!("could not reveal {id}")
}

async fn fill(
    cx: &mut AsyncWindowContext,
    id: &str,
    value: &str,
    submit: bool,
) -> anyhow::Result<()> {
    cx.update(|window, cx| click(window, id, cx))??;
    cx.update(|window, cx| text(window, value, cx))?;
    if submit {
        cx.update(|window, cx| key(window, "enter", cx))?;
    }
    Ok(())
}

fn value(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    object: &ObjectRef,
    field: PropertyKey,
) -> anyhow::Result<PropertyValue> {
    view.update_in(cx, |view, _, _| {
        view.editor
            .document()
            .unwrap()
            .property_values(object)
            .map(|v| v[&field].clone())
    })?
}

fn live(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    field: PropertyKey,
) -> anyhow::Result<String> {
    view.update_in(cx, |view, _, cx| {
        view.dock
            .as_ref()
            .unwrap()
            .properties
            .read(cx)
            .benchmark_live_value(field, cx)
    })
    .map_err(Into::into)
}

async fn open(cx: &mut AsyncWindowContext) -> anyhow::Result<()> {
    show(cx, "property-color-multiply").await?;
    cx.update(|window, cx| click(window, "property-color-multiply", cx))??;
    pause(cx, 60).await;
    cx.update(|window, _| {
        ensure!(
            window.try_find("color-picker-panel").is_some(),
            "picker did not open"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    Ok(())
}

async fn close(cx: &mut AsyncWindowContext) -> anyhow::Result<()> {
    cx.update(|window, cx| key(window, "escape", cx))?;
    pause(cx, 30).await;
    cx.update(|window, _| {
        ensure!(
            window.try_find("color-picker-panel").is_none(),
            "picker did not close"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    Ok(())
}

pub(super) async fn run(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
) -> anyhow::Result<()> {
    let object = view.update_in(cx, |view, window, cx| -> anyhow::Result<_> {
        let layer = view
            .editor
            .layers()
            .into_iter()
            .filter(|l| l.visible && l.opacity > 0. && l.vertices > 0)
            .max_by_key(|l| l.vertices)
            .context("model has no visible mesh")?;
        let object = ObjectRef::new(ObjectKind::Mesh, &layer.id);
        view.editor.execute(EditorCommand::Select(layer.id))?;
        view.refresh_document(window, cx);
        view.run_command(editor_core::CommandId::FitView, window, cx);
        frame_trace::emit(
            frame_trace::next_id(),
            "properties_fixture",
            &[
                ("meshes", view.editor.layers().len() as u64),
                ("selected_vertices", layer.vertices as u64),
                ("selected_opacity_milli", (layer.opacity * 1000.) as u64),
            ],
        );
        eprintln!("Properties benchmark mesh: {} ({})", layer.name, object.id);
        Ok(object)
    })??;
    pause(cx, 500).await;
    let requested = std::env::var("KASANE_BENCH_PROPERTY_CASE").ok();
    ensure!(
        requested
            .as_ref()
            .is_none_or(|r| CASES.contains(&r.as_str())),
        "unknown property case"
    );
    for (index, &case) in CASES
        .iter()
        .enumerate()
        .filter(|(_, c)| requested.as_ref().is_none_or(|r| r == **c))
    {
        eprintln!("Properties workload {index}: {case}");
        let history = view.update_in(cx, |v, _, _| {
            v.editor.document().unwrap().history_lengths().0
        })?;
        // A non-grey baseline keeps hue and every color channel meaningful.
        show(cx, "property-input-multiply").await?;
        fill(cx, "property-input-multiply", "#804020", true).await?;
        pause(cx, 40).await;
        let picker = case.starts_with("picker_") && case != "picker_open_close";
        if picker {
            open(cx).await?;
        }
        let field = match case {
            "name" => mesh::NAME.key,
            "runtime_id" => mesh::RUNTIME_ID.key,
            "part" => mesh::PART.key,
            "masks" => mesh::MASKS.key,
            "color_blend" => mesh::COLOR_BLEND.key,
            "enabled" => mesh::ENABLED.key,
            "opacity_text" | "opacity_drag" => mesh::OPACITY.key,
            _ => mesh::MULTIPLY.key,
        };
        let input_id = match case {
            "name" => "property-input-name",
            "runtime_id" => "property-input-runtime_id",
            "part" => "property-select-part_id",
            "masks" => "property-select-masks",
            "opacity_text" | "opacity_drag" => "property-input-opacity",
            "color_blend" => "property-select-color_blend",
            "enabled" => "property-toggle-enabled",
            _ => "property-input-multiply",
        };
        if !picker {
            show(cx, input_id).await?;
        }
        if case.contains("_hsv_") || case.contains("_rgb_") {
            cx.update(|w, cx| click(w, if case.contains("_hsv_") { "HSV" } else { "RGB" }, cx))??;
            pause(cx, 40).await;
        }
        let channel = case.split('_').find_map(|s| s.parse::<usize>().ok());
        let drag = case.ends_with("_drag") || matches!(case, "picker_hue" | "picker_sv");
        let drag_id = if case == "picker_hue" {
            "color-picker-hue".to_owned()
        } else if case == "picker_sv" {
            "color-picker-sv".to_owned()
        } else if let Some(channel) = channel {
            format!("color-picker-channel-{channel}")
        } else {
            input_id.to_owned()
        };
        let bounds = if drag {
            Some(cx.update(|w, _| w.find(drag_id.clone()).bounds())?)
        } else {
            None
        };
        let position = |i: usize| {
            let b = bounds.unwrap();
            if case == "picker_hue" {
                let angle = i as f32 * 0.13;
                b.center()
                    + point(
                        b.size.width * 0.44 * angle.cos(),
                        b.size.height * 0.44 * angle.sin(),
                    )
            } else if case == "picker_sv" {
                // A small circle around the wheel centre stays inside the SV triangle.
                let angle = i as f32 * 0.13;
                b.center()
                    + point(
                        b.size.width * 0.09 * angle.cos(),
                        b.size.height * 0.09 * angle.sin(),
                    )
            } else {
                b.center() + point(px(-((8 + (i * 7) % 64) as f32)), px(0.))
            }
        };
        if drag {
            view.update_in(cx, |v, _, cx| {
                v.dock
                    .as_ref()
                    .unwrap()
                    .properties
                    .clone()
                    .update(cx, |panel, cx| panel.benchmark_isolate_pointers(cx));
            })?;
            cx.update(|w, cx| {
                w.blur(cx);
                pointer(
                    w,
                    if case.starts_with("picker_") && channel.is_none() {
                        position(0)
                    } else {
                        bounds.unwrap().center()
                    },
                    0,
                    cx,
                );
            })?;
        }
        if matches!(case, "idle" | "picker_idle") {
            cx.update(|w, cx| w.blur(cx))?;
            pause(cx, 250).await;
        }
        let mut changed = 0u64;
        let mut previous = value(view, cx, &object, field)?;
        let mut previous_live = live(view, cx, field)?;
        let interval = Duration::from_secs_f64(1. / config.hz as f64);
        let mut deadline = Instant::now() + interval;
        let phase = frame_trace::next_id();
        frame_trace::emit(
            phase,
            "properties_phase",
            &[("case", index as u64), ("active", 1)],
        );
        for sample in 0..config.samples + config.warmup {
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
            cx.update(|w, _| {
                ensure!(
                    w.is_visible() && w.is_window_active(),
                    "properties window lost visibility or focus"
                );
                Ok::<_, anyhow::Error>(())
            })??;
            if drag {
                cx.update(|w, cx| pointer(w, position(sample + 1), 1, cx))?;
            } else {
                match case {
                    "name" | "runtime_id" => {
                        fill(cx, input_id, &format!("Bench{sample:05}"), true).await?
                    }
                    "opacity_text" => {
                        fill(
                            cx,
                            input_id,
                            if sample % 2 == 0 { "35" } else { "75" },
                            true,
                        )
                        .await?
                    }
                    "multiply_hex" => {
                        fill(
                            cx,
                            input_id,
                            if sample % 2 == 0 {
                                "#4080C0"
                            } else {
                                "#C06030"
                            },
                            true,
                        )
                        .await?
                    }
                    "enabled" => {
                        cx.update(|w, cx| click(w, input_id, cx))??;
                    }
                    "part" | "color_blend" | "masks" => {
                        if case == "masks" && sample % 2 == 1 {
                            show(cx, "property-remove-masks-0").await?;
                            cx.update(|w, cx| click(w, "property-remove-masks-0", cx))??;
                        } else {
                            show(cx, input_id).await?;
                            let choices = view.update_in(cx, |v, _, cx| {
                                v.dock
                                    .as_ref()
                                    .unwrap()
                                    .properties
                                    .read(cx)
                                    .benchmark_choices(field)
                            })?;
                            let selected = value(view, cx, &object, field)?;
                            let target = choices
                                .iter()
                                .position(|(_, id)| match &selected {
                                    PropertyValue::Text(value) => value != id,
                                    PropertyValue::References(ids) => {
                                        id != &object.id && !ids.contains(id)
                                    }
                                    _ => false,
                                })
                                .context("no alternative select choice")?;
                            // Visit every blending mode, rather than alternating only two modes.
                            let target = if case == "color_blend" {
                                (sample + 1) % choices.len()
                            } else {
                                target
                            };
                            cx.update(|w, cx| click(w, input_id, cx))??;
                            pause(cx, 35).await;
                            if case == "color_blend" {
                                let current = choices
                                    .iter()
                                    .position(|(_, id)| selected == PropertyValue::Text(id.clone()))
                                    .context("missing selected mode")?;
                                cx.update(|w, cx| {
                                    for _ in 0..(target + choices.len() - current) % choices.len() {
                                        key(w, "down", cx);
                                    }
                                    key(w, "enter", cx);
                                })?;
                            } else {
                                cx.update(|w, cx| text(w, &choices[target].0, cx))?;
                                pause(cx, 180).await;
                                cx.update(|w, cx| key(w, "enter", cx))?;
                            }
                            let actual = value(view, cx, &object, field)?;
                            ensure!(
                                match actual {
                                    PropertyValue::Text(id) => id == choices[target].1,
                                    PropertyValue::References(ids) =>
                                        ids.contains(&choices[target].1),
                                    _ => false,
                                },
                                "{case}: selection did not reach the requested value"
                            );
                        }
                    }
                    "picker_hex" | "picker_invalid_hex" => {
                        fill(
                            cx,
                            "color-picker-hex",
                            if case == "picker_invalid_hex" && sample % 2 == 0 {
                                "#12"
                            } else if sample % 2 == 0 {
                                "#4080C0"
                            } else {
                                "#C06030"
                            },
                            false,
                        )
                        .await?
                    }
                    c if c.ends_with("_text") && channel.is_some() => {
                        let high = if c.contains("_hsv_") && channel != Some(0) {
                            "75"
                        } else {
                            "180"
                        };
                        fill(
                            cx,
                            &format!("color-picker-channel-{}", channel.unwrap()),
                            if sample % 2 == 0 { "30" } else { high },
                            false,
                        )
                        .await?;
                    }
                    "picker_tabs" => {
                        cx.update(|w, cx| {
                            click(w, if sample % 2 == 0 { "HSV" } else { "RGB" }, cx)
                        })??;
                    }
                    "picker_open_close" => {
                        open(cx).await?;
                        close(cx).await?;
                    }
                    "picker_done" | "picker_cancel" => {
                        fill(
                            cx,
                            "color-picker-hex",
                            if sample % 2 == 0 {
                                "#4080C0"
                            } else {
                                "#C06030"
                            },
                            false,
                        )
                        .await?;
                        cx.update(|w, cx| {
                            click(
                                w,
                                if case == "picker_done" {
                                    "color-picker-done"
                                } else {
                                    "color-picker-cancel"
                                },
                                cx,
                            )
                        })??;
                        pause(cx, 30).await;
                        open(cx).await?;
                    }
                    "multiply_reset" => {
                        fill(cx, input_id, "#804020", true).await?;
                        cx.update(|w, cx| click(w, "property-color-multiply-reset", cx))??;
                    }
                    "picker_swatches" => {
                        cx.update(|w, cx| -> anyhow::Result<()> {
                            let slot = if config.samples < 90 {
                                sample
                            } else {
                                sample / 90
                            };
                            let label = if slot % 2 == 0 {
                                t!("color_picker.original")
                            } else {
                                t!("color_picker.current")
                            };
                            let id = format!("color-picker-swatch-{label}");
                            let b = w.try_find(id).context("swatch missing")?.bounds();
                            pointer(w, b.center(), 3, cx);
                            Ok(())
                        })??;
                        if config.samples < 90 {
                            pause(cx, 600).await;
                        }
                    }
                    "idle" | "picker_idle" => {}
                    _ => anyhow::bail!("unimplemented workload {case}"),
                }
            }
            drop(scope); // Include effect flushing and SDK subscriptions in dispatch time.
            let next = value(view, cx, &object, field)?;
            let next_live = live(view, cx, field)?;
            let did_change = next != previous || next_live != previous_live;
            if sample >= config.warmup {
                changed += u64::from(did_change);
            }
            frame_trace::emit(
                tick,
                "properties_checked",
                &[("changed", u64::from(did_change))],
            );
            previous = next;
            previous_live = next_live;
            view.update_in(cx, |v, _, _| {
                ensure!(
                    v.edit_error.is_none(),
                    "property edit failed: {:?}",
                    v.edit_error
                );
                ensure!(v.render_error().is_none(), "property render failed");
                Ok::<_, anyhow::Error>(())
            })??;
            deadline += interval;
            if deadline < Instant::now() {
                deadline = Instant::now() + interval;
            }
        }
        if drag {
            cx.update(|w, cx| pointer(w, position(config.samples + config.warmup), 2, cx))?;
        }
        pause(cx, if case == "picker_swatches" { 650 } else { 250 }).await;
        frame_trace::emit(
            phase,
            "properties_phase_end",
            &[("case", index as u64), ("changed_samples", changed)],
        );
        let changing = !matches!(
            case,
            "idle"
                | "picker_idle"
                | "picker_tabs"
                | "picker_open_close"
                | "picker_cancel"
                | "picker_swatches"
                | "multiply_reset"
                | "picker_invalid_hex"
        );
        ensure!(
            !changing || changed > 0,
            "{case} never changed its property or live preview"
        );
        if picker {
            if case == "picker_idle" {
                cx.update(|w, cx| click(w, "color-picker-cancel", cx))??;
                pause(cx, 40).await;
            } else {
                close(cx).await?;
            }
        }
        // Restore committed changes outside the measured phase. Never save the source model.
        view.update_in(cx, |v, _, cx| -> anyhow::Result<()> {
            while v.editor.document().unwrap().history_lengths().0 > history {
                v.editor.execute(EditorCommand::Undo)?;
            }
            v.redraw(cx);
            Ok(())
        })??;
        pause(cx, 150).await;
    }
    pause(cx, 1500).await;
    Ok(())
}
