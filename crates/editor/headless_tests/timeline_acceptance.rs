//! Licensed source assets stay untouched. Stress variants exist only in a temporary directory.
use crate::screenshots::Harness;
use anyhow::{Context as _, ensure};
use editor_core::{
    AnimationEvaluator, AnimationPreviewMode, CommandId, Document, Editor, LoadRequest,
    TimelineChannelId, TimelineCommand, TimelineDisplayMode, TimelineKeyRef, TimelineRowId,
    WorkspaceKind,
};
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent, Window, point, px,
    test::TestWindowExt,
};
use kasane_core::document::{MotionClip, MotionPoint};
use kasane_sdk::{CompiledCurve, CompiledSegment, sample_motion_curve};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

fn update<T>(
    h: &mut Harness,
    f: impl FnOnce(&mut crate::EditorView, &mut Window, &mut gpui_kit::Context<crate::EditorView>) -> T,
) -> anyhow::Result<T> {
    Ok(h.cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| f(view, window, cx))
    })?)
}
fn command(h: &mut Harness, id: CommandId) -> anyhow::Result<()> {
    update(h, |view, window, cx| view.run_command(id, window, cx))
}
fn pump(h: &mut Harness) -> anyhow::Result<()> {
    h.cx.run_until_parked();
    h.cx.update_window(h.window, |_, window, cx| {
        window.draw(cx).clear(cx);
    })?;
    Ok(())
}
fn current(h: &Harness) -> bool {
    h.view.read_with(&h.cx, |view, _| {
        let a = view.editor.animation().unwrap();
        assert!(a.error.is_none(), "{:?}", a.error);
        assert!(view.render_error.is_none(), "{:?}", view.render_error);
        view.animation_frame.as_ref().is_some_and(|frame| {
            let id = frame.state.identity;
            view.editor.can_display_animation_image(id)
                && id.time == a.time
                && id.candidate == a.candidate
                && id.model_generation == a.model_generation
                && id.preview_generation == a.preview_generation
                && id.preview_mode == a.preview_mode
                && a.presented
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(p, &frame.state.snapshot))
        })
    })
}
fn drain(h: &mut Harness) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        pump(h)?;
        if current(h) {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "current Timeline revision never reached the Metal canvas"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn stats(values: &[f64]) -> Value {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let percentile = |p: f64| v[((v.len() - 1) as f64 * p).ceil() as usize];
    json!({"samples":v.len(),"p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"max_ms":v.last()})
}
fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn numeric(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}
fn curve(v: &Value) -> CompiledCurve {
    let numbers = v["Segments"].as_array().unwrap();
    let point_at = |i| MotionPoint {
        time: numeric(&numbers[i]),
        value: numeric(&numbers[i + 1]),
    };
    let mut segments = Vec::new();
    let mut i = 2;
    while i < numbers.len() {
        let kind = numbers[i].as_u64().unwrap();
        segments.push(match kind {
            0 => CompiledSegment::Linear {
                end: point_at(i + 1),
            },
            1 => CompiledSegment::Bezier {
                control1: point_at(i + 1),
                control2: point_at(i + 3),
                end: point_at(i + 5),
            },
            2 => CompiledSegment::Stepped {
                end: point_at(i + 1),
            },
            3 => CompiledSegment::InverseStepped {
                end: point_at(i + 1),
            },
            _ => panic!("unexpected segment kind"),
        });
        i += if kind == 1 { 7 } else { 3 };
    }
    CompiledCurve {
        initial: point_at(0),
        segments: Arc::new(segments),
    }
}
fn recount(motion: &mut Value) {
    let curves = motion["Curves"].as_array().unwrap();
    let compiled: Vec<_> = curves.iter().map(curve).collect();
    let count = compiled.iter().map(|c| c.segments.len()).sum::<usize>();
    let points = compiled
        .iter()
        .map(|c| {
            1 + c
                .segments
                .iter()
                .map(|s| {
                    if matches!(s, CompiledSegment::Bezier { .. }) {
                        3
                    } else {
                        1
                    }
                })
                .sum::<usize>()
        })
        .sum::<usize>();
    motion["Meta"]["CurveCount"] = json!(curves.len());
    motion["Meta"]["TotalSegmentCount"] = json!(count);
    motion["Meta"]["TotalPointCount"] = json!(points);
}
fn variants(source: &Path) -> anyhow::Result<(tempfile::TempDir, PathBuf, Value)> {
    let model: Value = serde_json::from_slice(&std::fs::read(source)?)?;
    let base = source.parent().unwrap();
    let motions = model["FileReferences"]["Motions"]
        .as_object()
        .context("model has no motions")?;
    let mut candidates = Vec::new();
    for entries in motions.values() {
        for entry in entries.as_array().unwrap() {
            let path = base.join(entry["File"].as_str().unwrap());
            let motion: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            candidates.push((
                motion["Meta"]["TotalSegmentCount"].as_u64().unwrap_or(0),
                path,
                motion,
            ));
        }
    }
    let (_, motion_path, original) = candidates
        .into_iter()
        .max_by_key(|(n, _, _)| *n)
        .context("no motion files")?;
    let temp = tempfile::tempdir()?;
    // The importer deliberately rejects symlinks escaping the model directory.
    // Copy into a disposable directory; source assets and tracked fixtures stay untouched.
    fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            let destination = target.join(entry.file_name());
            if path.is_dir() {
                copy_tree(&path, &destination)?;
            } else {
                std::fs::copy(path, destination)?;
            }
        }
        Ok(())
    }
    copy_tree(base, temp.path())?;
    let mut natural = original.clone();
    let first = natural["Curves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["Target"] == "Parameter")
        .context("no parameter curves")?
        .clone();
    let mut unresolved = first.clone();
    unresolved["Id"] = json!("TimelineMissingParameter");
    natural["Curves"]
        .as_array_mut()
        .unwrap()
        .extend([first, unresolved]);
    recount(&mut natural);
    let mut dense = natural.clone();
    let duration = numeric(&dense["Meta"]["Duration"]);
    let restricted = dense["Meta"]["AreBeziersRestricted"].as_bool().unwrap();
    for c in dense["Curves"].as_array_mut().unwrap() {
        let raw = curve(c);
        let end = raw
            .segments
            .last()
            .map_or(duration, |s| s.end().time)
            .max(duration);
        let mut numbers = vec![json!(0.), json!(sample_motion_curve(&raw, 0., restricted))];
        for i in 1..=256 {
            let time = end * i as f32 / 256.;
            numbers.extend([
                json!(0),
                json!(time),
                json!(sample_motion_curve(&raw, time, restricted)),
            ]);
        }
        c["Segments"] = json!(numbers);
    }
    recount(&mut dense);
    let mut opaque = original.clone();
    opaque["TimelineVendorExtension"] = json!({"version":7,"payload":["preserve",0.1137]});
    let mut model = model;
    let mut registrations = serde_json::Map::new();
    for (name, motion) in [("Natural", natural), ("Dense", dense), ("Opaque", opaque)] {
        let file = format!("timeline-{name}.motion3.json");
        std::fs::write(temp.path().join(&file), serde_json::to_vec(&motion)?)?;
        registrations.insert(format!("Timeline{name}"), json!([{"File":file}]));
    }
    model["FileReferences"]["Motions"] = Value::Object(registrations);
    let path = temp.path().join("timeline-acceptance.model3.json");
    std::fs::write(&path, serde_json::to_vec(&model)?)?;
    Ok((
        temp,
        path,
        json!({"model":source,"motion":motion_path,"original_curves":original["Meta"]["CurveCount"],"original_segments":original["Meta"]["TotalSegmentCount"],"dense_points_per_track":257}),
    ))
}
fn select(h: &mut Harness, group: &str) -> anyhow::Result<()> {
    update(h, |view, _, cx| -> anyhow::Result<()> {
        let selection = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .find(|m| m.selection.entry.as_ref().is_some_and(|(g, _)| g == group))
            .context("missing variant")?
            .selection
            .clone();
        view.editor
            .select_motion(view.editor.active_id().unwrap(), selection)?;
        view.redraw(cx);
        Ok(())
    })??;
    drain(h)
}
fn clip(h: &Harness) -> MotionClip {
    h.view.read_with(&h.cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .motion_definition(&view.editor.timeline_projection().unwrap().clip_id)
            .unwrap()
            .clone()
    })
}
fn edit(h: &mut Harness, c: TimelineCommand) -> anyhow::Result<()> {
    update(h, |view, _, cx| -> anyhow::Result<()> {
        view.editor
            .execute_timeline(view.editor.active_id().unwrap(), c)?;
        view.redraw(cx);
        Ok(())
    })??;
    drain(h)
}
fn edges(h: &mut Harness) -> anyhow::Result<Value> {
    select(h, "TimelineOpaque")?;
    let opaque = clip(h);
    update(h, |view, _, _| {
        assert!(!view.editor.timeline_projection().unwrap().editable);
        assert!(
            view.editor
                .begin_timeline_edit(view.editor.active_id().unwrap(), "Forbidden")
                .is_err()
        );
    })?;
    ensure!(clip(h) == opaque, "opaque clip changed");
    select(h, "TimelineNatural")?;
    let original = clip(h);
    let projection = h
        .view
        .read_with(&h.cx, |v, _| v.editor.timeline_projection().unwrap());
    let unresolved = projection
        .tracks
        .values()
        .find(|t| matches!(t.channel, TimelineChannelId::Unresolved { .. }))
        .context("unresolved track missing")?;
    ensure!(!unresolved.editable, "unresolved track writable");
    let duplicate = projection
        .channels
        .iter()
        .find(|c| c.tracks.len() == 2)
        .context("duplicate channel collapsed")?;
    let track = &projection.tracks[&duplicate.tracks[0]];
    let other = projection
        .tracks
        .values()
        .find(|t| t.editable && t.id != track.id && t.keys.len() > 1)
        .unwrap();
    let before = update(h, |view, _, _| -> anyhow::Result<()> {
        let doc = view.editor.active_id().unwrap();
        view.editor.update_timeline_view(doc, |v| v.snap = false)?;
        let token = view.editor.begin_timeline_edit(doc, "Atomic conflict")?;
        let key = track.keys[0].reference.clone();
        assert!(
            view.editor
                .update_timeline_edit(
                    doc,
                    token,
                    TimelineCommand::MoveKeys {
                        keys: vec![key.clone(), other.keys[0].reference.clone()],
                        anchor: key,
                        time_delta: track.keys[1].time - track.keys[0].time,
                        value_delta: 0.
                    }
                )
                .is_err()
        );
        assert_eq!(
            view.editor.timeline_edit().unwrap().candidate.clip(),
            &original
        );
        view.editor.cancel_timeline_edit();
        Ok(())
    })?;
    before?;
    let (index, _) = original
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(i, t)| {
            t.segments.iter().enumerate().filter_map(move |(s, seg)| {
                matches!(seg, kasane_core::document::MotionSegment::Bezier { .. }).then_some((i, s))
            })
        })
        .next()
        .context("no imported Bezier")?;
    let t = &original.tracks[index];
    let segment = t
        .segments
        .iter()
        .position(|s| matches!(s, kasane_core::document::MotionSegment::Bezier { .. }))
        .unwrap();
    let start = if segment == 0 {
        t.initial.time
    } else {
        t.segments[segment - 1].end().time
    };
    let time = (start + t.segments[segment].end().time) / 2.;
    let channel = TimelineChannelId::from_target(&t.target);
    update(h, |view, _, _| {
        view.editor
            .seek_timeline(view.editor.active_id().unwrap(), time)
    })??;
    edit(
        h,
        TimelineCommand::InsertKey {
            channel,
            track_id: Some(t.id.clone()),
            selected_key: None,
        },
    )?;
    let split = clip(h);
    let a = CompiledCurve::from(t);
    let b = CompiledCurve::from(&split.tracks[index]);
    let error = (0..=512)
        .map(|i| {
            let time = original.duration * i as f32 / 512.;
            (sample_motion_curve(&a, time, original.restricted_beziers)
                - sample_motion_curve(&b, time, original.restricted_beziers))
            .abs()
        })
        .fold(0f32, f32::max);
    ensure!(error < 0.0001, "real Bezier split changed shape: {error}");
    command(h, CommandId::Undo)?;
    drain(h)?;
    ensure!(
        clip(h) == original,
        "split Undo did not restore UUIDs/segments"
    );
    let subframes = projection
        .tracks
        .values()
        .flat_map(|t| &t.keys)
        .filter(|k| {
            (k.time * projection.fps as f64 - (k.time * projection.fps as f64).round()).abs()
                > 0.001
        })
        .count();
    let tail = projection
        .tracks
        .values()
        .flat_map(|t| &t.keys)
        .map(|k| k.time)
        .fold(projection.duration, f64::max);
    ensure!(subframes > 0, "real motion lacks subframe coverage");
    ensure!(
        tail > projection.duration,
        "real motion lacks tolerated tail coverage"
    );
    Ok(
        json!({"subframe_keys":subframes,"nominal_duration":projection.duration,"last_key_time":tail,"duplicate_tracks":duplicate.tracks.len(),"unresolved_tracks":projection.tracks.values().filter(|t| matches!(t.channel, TimelineChannelId::Unresolved { .. })).count(),"opaque_preserved":true,"atomic_conflict":true,"split_max_error":error}),
    )
}

fn result_parity(h: &mut Harness) -> anyhow::Result<()> {
    select(h, "TimelineNatural")?;
    update(h, |view, _, cx| -> anyhow::Result<()> {
        let doc = view.editor.active_id().unwrap();
        view.editor
            .set_animation_preview_mode(doc, AnimationPreviewMode::Result)?;
        let expression = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .expressions
            .first()
            .map(|e| e.id.clone());
        if let Some(expression) = expression {
            view.editor.select_expression(doc, expression)?;
        }
        view.redraw(cx);
        Ok(())
    })??;
    for time in [0.1137, 1.125, 9.23, 0.25] {
        let request = update(h, |view, _, cx| -> anyhow::Result<_> {
            view.editor
                .seek_timeline(view.editor.active_id().unwrap(), time)?;
            let request = view
                .editor
                .animation_request()
                .context("missing parity request")?;
            // The observer owns this request. Submit a fresh identity to the real worker too.
            view.editor.invalidate_animation();
            view.redraw(cx);
            Ok(request)
        })??;
        let mut sdk = kasane_sdk::MotionPreview::new(request.source.snapshot.document());
        let motion = request.motion.as_ref().unwrap();
        if let Some((group, index)) = &motion.entry {
            sdk.schedule_motion_entry(group, *index, 0.)?;
        } else {
            sdk.schedule_motion(&motion.clip_id, 0.)?;
        }
        sdk.replace_expression_schedule_with_transitions(
            request
                .expressions
                .iter()
                .map(|e| (e.expression_id.as_str(), e.time, e.transition)),
        )?;
        sdk.seek(request.identity.time)?;
        let mut evaluator = AnimationEvaluator::default();
        let evaluated = evaluator.evaluate(&request, |_, _| true)?;
        ensure!(
            *evaluated.snapshot == *sdk.snapshot(),
            "real result snapshot diverged from SDK"
        );
        ensure!(
            *evaluated.scene.viewport_frame == sdk.evaluate_drawables()?,
            "real result geometry diverged from SDK"
        );
        drain(h)?;
        ensure!(
            h.view.read_with(&h.cx, |v, _| v
                .editor
                .animation()
                .unwrap()
                .presented
                .as_deref()
                == Some(sdk.snapshot())),
            "presented result diverged from SDK"
        );
    }
    Ok(())
}
fn pointer(h: &mut Harness, position: Point<Pixels>, kind: u8) -> anyhow::Result<()> {
    h.cx.update_window(h.window, |_, window, cx| match kind {
        0 => {
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
        }
        1 => {
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
        }
        _ => {
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        }
    })?;
    Ok(())
}
fn performance(
    h: &mut Harness,
    output: &Path,
    samples: usize,
    variant: &str,
) -> anyhow::Result<Value> {
    select(h, &format!("Timeline{variant}"))?;
    let original = clip(h);
    let projection = h
        .view
        .read_with(&h.cx, |v, _| v.editor.timeline_projection().unwrap());
    let row_cache = h
        .view
        .read_with(&h.cx, |v, _| v.editor.timeline_visible_rows().unwrap());
    let mut scroll = Vec::new();
    for i in 0..samples {
        let start = Instant::now();
        h.cx.update_window(h.window, |_, window, cx| {
            let position = window.find("timeline-graph").bounds().center();
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Lines(point(0., if i % 40 < 20 { -2. } else { 2. })),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        })?;
        pump(h)?;
        scroll.push(elapsed(start));
        ensure!(
            h.view.read_with(&h.cx, |v, _| Arc::ptr_eq(
                &row_cache,
                &v.editor.timeline_visible_rows().unwrap()
            )),
            "scroll rebuilt the row/key tree"
        );
    }
    let chosen = projection
        .channels
        .iter()
        .find(|c| c.runtime_id == "ParamAngleX")
        .or_else(|| {
            projection
                .channels
                .iter()
                .find(|c| c.editable && c.tracks.len() == 1)
        })
        .context("no editable channel")?;
    // Later duplicate tracks overwrite earlier tracks in SDK order.
    let track = &projection.tracks[chosen.tracks.last().unwrap()];
    let key = track.keys[0].reference.clone();
    update(h, |view, _, cx| -> anyhow::Result<()> {
        let doc = view.editor.active_id().unwrap();
        view.editor.update_timeline_view(doc, |v| {
            v.scroll = 0.;
            v.search = chosen.runtime_id.clone();
            v.mode = TimelineDisplayMode::Curves;
            v.channels = [
                TimelineRowId::Track(track.id.clone()),
                TimelineRowId::Channel(chosen.id.clone()),
            ]
            .into();
            v.time_range = (0., 0.1);
            v.snap = false;
        })?;
        view.editor.seek_timeline(doc, track.keys[0].time as f32)?;
        view.redraw(cx);
        Ok(())
    })??;
    let before = h.capture()?;
    let before_canvas = h.canvas_image(&before)?;
    before.save(output.join(format!("{variant}-before.png")))?;
    let origin = h
        .view
        .read_with(&h.cx, |v, cx| v.timeline.read(cx).key_position(&key))
        .context("dense key not rendered")?
        + point(px(0.5), px(0.));
    let history = h
        .view
        .read_with(&h.cx, |v, _| v.editor.document().unwrap().history_lengths());
    pointer(h, origin, 0)?;
    let mut drag = Vec::new();
    let mut drag_dispatch = Vec::new();
    for i in 0..32 {
        let start = Instant::now();
        pointer(h, origin + point(px(0.), px(-8. - i as f32 * 0.5)), 1)?;
        drag_dispatch.push(elapsed(start));
        drain(h)?;
        drag.push(elapsed(start));
        ensure!(
            h.view
                .read_with(&h.cx, |v, _| v.editor.timeline_edit().is_some()),
            "native key drag did not create a candidate"
        );
        ensure!(clip(h) == original, "candidate mutated the asset");
    }
    let changed = h.capture()?;
    changed.save(output.join(format!("{variant}-candidate.png")))?;
    ensure!(
        h.canvas_image(&changed)? != before_canvas,
        "native dense drag did not change presented pixels"
    );
    h.cx.update_window(h.window, |_, w, cx| w.press("escape", cx))?;
    pointer(h, origin, 2)?;
    let restored = h.capture()?;
    restored.save(output.join(format!("{variant}-cancel.png")))?;
    ensure!(
        h.canvas_image(&restored)? == before_canvas,
        "cancel did not restore real model pixels"
    );
    ensure!(clip(h) == original, "cancel changed real imported curves");
    // Cancel while candidate CPU/GPU work is still in flight, then deliver a late move.
    pointer(h, origin, 0)?;
    for i in 0..8 {
        pointer(h, origin + point(px(0.), px(-9. - i as f32)), 1)?;
    }
    let cancel_start = Instant::now();
    h.cx.update_window(h.window, |_, w, cx| w.press("escape", cx))?;
    pointer(h, origin + point(px(0.), px(-30.)), 1)?;
    pointer(h, origin, 2)?;
    drain(h)?;
    let cancel_ms = elapsed(cancel_start);
    let restored = h.capture()?;
    ensure!(
        h.canvas_image(&restored)? == before_canvas,
        "late candidate replaced cancelled image"
    );
    ensure!(
        h.view
            .read_with(&h.cx, |v, _| v.editor.timeline_edit().is_none()
                && v.editor.document().unwrap().history_lengths() == history),
        "late move restarted cancelled candidate"
    );
    // Release commits exactly once; Undo restores both persistent identity and curves.
    pointer(h, origin, 0)?;
    pointer(h, origin + point(px(0.), px(-20.)), 1)?;
    pump(h)?;
    pointer(h, origin + point(px(0.), px(-20.)), 2)?;
    drain(h)?;
    ensure!(
        h.view.read_with(&h.cx, |v, _| v
            .editor
            .document()
            .unwrap()
            .history_lengths()
            .0)
            == history.0 + 1,
        "drag committed more than one history entry"
    );
    command(h, CommandId::Undo)?;
    drain(h)?;
    ensure!(clip(h) == original, "dense drag Undo changed UUIDs/curves");
    let mut seeks = Vec::new();
    for i in 0..32 {
        let start = Instant::now();
        update(h, |view, _, cx| {
            let doc = view.editor.active_id().unwrap();
            view.editor
                .seek_timeline(doc, original.duration * ((i * 17) % 31) as f32 / 31.)
                .unwrap();
            view.redraw(cx);
        })?;
        drain(h)?;
        seeks.push(elapsed(start));
    }
    // A burst must merge pending requests and drain only the final current identity.
    let start = Instant::now();
    for i in 0..samples {
        update(h, |view, _, cx| {
            let doc = view.editor.active_id().unwrap();
            view.editor
                .scrub_timeline(doc, original.duration * ((i * 13) % 97) as f32 / 97.)
                .unwrap();
            view.redraw(cx);
        })?;
    }
    drain(h)?;
    let burst_ms = elapsed(start);
    let final_time = h
        .view
        .read_with(&h.cx, |v, _| v.editor.animation().unwrap().time);
    let (builds, paints) = h.view.read_with(&h.cx, |v, cx| {
        (
            v.timeline.read(cx).scene_builds(),
            v.timeline.read(cx).scene_paints(),
        )
    });
    update(h, |view, _, cx| {
        let doc = view.editor.active_id().unwrap();
        view.editor.seek_timeline(doc, 0.).unwrap();
        view.editor.set_animation_preview_loop(doc, true).unwrap();
        view.editor.toggle_animation().unwrap();
        view.redraw(cx);
    })?;
    drain(h)?;
    let start = Instant::now();
    let mut observed = 0;
    let mut last = None;
    let mut max_lag = 0f32;
    for tick in 0..samples {
        let deadline = start + Duration::from_secs_f64(tick as f64 / 60.);
        while Instant::now() < deadline {
            pump(h)?;
            std::thread::sleep(Duration::from_millis(1));
        }
        update(h, |view, _, cx| {
            view.editor.advance_animation_clock(tick as f32 / 60.);
            view.request_preview();
            view.notify_animation(cx);
        })?;
        pump(h)?;
        h.view.read_with(&h.cx, |view, _| {
            let a = view.editor.animation().unwrap();
            if let Some(s) = view.displayed_animation() {
                if last != Some(s.time) {
                    observed += 1;
                    last = Some(s.time);
                }
                max_lag = max_lag.max(a.time - s.time);
            }
        });
    }
    update(h, |view, _, cx| {
        view.editor.pause_animation();
        view.redraw(cx);
    })?;
    drain(h)?;
    let (after_builds, after_paints) = h.view.read_with(&h.cx, |v, cx| {
        (
            v.timeline.read(cx).scene_builds(),
            v.timeline.read(cx).scene_paints(),
        )
    });
    ensure!(
        after_builds == builds && after_paints == paints,
        "clock/seek invalidated static curves"
    );
    ensure!(observed > 0, "playback starved");
    ensure!(
        clip(h) == original,
        "navigation/playback mutated dense clip"
    );
    Ok(
        json!({"tracks":projection.tracks.len(),"keys":projection.tracks.values().map(|t|t.keys.len()).sum::<usize>(),"scroll_dispatch_draw":stats(&scroll),"drag_dispatch":stats(&drag_dispatch),"drag_dispatch_to_current_metal_draw":stats(&drag),"seek_to_current_metal_draw":stats(&seeks),"burst":{"requests":samples,"dispatch_and_drain_ms":burst_ms,"final_time":final_time},"playback":{"requests":samples,"observed_frames":observed,"wall_seconds":start.elapsed().as_secs_f64(),"max_lag_seconds":max_lag,"static_scene_rebuilds":after_builds-builds,"static_scene_repaints":after_paints-paints},"candidate_cancel_pixels_restored":true,"inflight_cancel_to_current_metal_draw_ms":cancel_ms,"one_drag_one_undo":true}),
    )
}
fn persistence(h: &mut Harness) -> anyhow::Result<()> {
    let original = h.view.read_with(&h.cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .map(|m| {
                v.editor
                    .document()
                    .unwrap()
                    .motion_definition(&m.selection.clip_id)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>()
    });
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("timeline.kasane");
    let save = update(h, |v, _, _| {
        v.editor
            .prepare_save(v.editor.active_id().unwrap(), &path, false)
    })??;
    let mut completion = save.run();
    update(h, |v, _, _| v.editor.finish_save(&mut completion))??;
    let reopened = Document::open(&path.join("project.kasane.json"))?;
    for c in original {
        ensure!(
            reopened.motion_definition(&c.id) == Some(&c),
            "save/reopen changed imported keys, UUIDs, tails or extensions"
        );
    }
    Ok(())
}
pub(super) fn run() -> anyhow::Result<()> {
    ensure!(
        std::env::var_os("KASANE_DEV_SESSION_DIR").is_none(),
        "unset KASANE_DEV_SESSION_DIR"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::env::var_os("KASANE_TIMELINE_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("models/local/mao/runtime/mao_pro.model3.json"));
    if !source.exists() {
        ensure!(
            std::env::var_os("KASANE_TIMELINE_REQUIRE_ASSET").is_none(),
            "required licensed model missing"
        );
        println!("SKIP Timeline real-asset acceptance: {}", source.display());
        return Ok(());
    }
    let samples = std::env::var("KASANE_TIMELINE_SAMPLES")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(60);
    ensure!((2..=3600).contains(&samples), "invalid sample count");
    let output = std::env::var_os("KASANE_TIMELINE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/timeline-acceptance"));
    std::fs::create_dir_all(&output)?;
    let (_assets, path, source_info) = variants(&source.canonicalize()?)?;
    eprintln!("Timeline acceptance: importing real curves and dense variants");
    let (document, notices) = Document::import_model3(&path)?;
    let mut h = Harness::new()?;
    update(&mut h, |view, window, cx| {
        view.editor = Editor::from_document(document);
        view.animation_clock_running = true;
        view.refresh_document(window, cx);
    })?;
    command(&mut h, CommandId::AnimationWorkspace)?;
    command(&mut h, CommandId::FitView)?;
    drain(&mut h)?;
    eprintln!("Timeline acceptance: imported edge cases");
    let edges = edges(&mut h)?;
    eprintln!("Timeline acceptance: real motion input, cancellation and playback");
    let real_performance = performance(&mut h, &output, samples, "Natural")?;
    eprintln!("Timeline acceptance: dense input, cancellation and playback");
    let performance = performance(&mut h, &output, samples, "Dense")?;
    eprintln!("Timeline acceptance: SDK result parity and project round trip");
    result_parity(&mut h)?;
    persistence(&mut h)?;
    let report = json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"logical_window":[1100,740],"measurement":"injected GPUI input through current SDK/Metal canvas acceptance and GPUI draw; excludes OS input delivery, screenshot readback and compositor presentation","source":source_info,"import_notices":notices,"edge_cases":edges,"real_motion_performance":real_performance,"performance":performance,"save_reopen_unchanged":true,"result_sdk_snapshot_and_geometry_parity":true});
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Timeline real-asset acceptance passed: {}",
        output.display()
    );
    Ok(())
}
