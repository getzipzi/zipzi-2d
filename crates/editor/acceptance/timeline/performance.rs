use super::super::harness::{Harness, command, update};
use super::common::{clip, drain, elapsed, pointer, pump, select, stats};
use anyhow::{Context as _, ensure};
use editor_core::{CommandId, TimelineDisplayMode, TimelineRowId};
use gpui_kit::{InputEvent as _, ScrollDelta, ScrollWheelEvent, point, px, test::TestWindowExt};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) fn performance(
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
        .read_with(&h.cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .key_position(&key)
        })
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
            v.dock.as_ref().unwrap().timeline.read(cx).scene_builds(),
            v.dock.as_ref().unwrap().timeline.read(cx).scene_paints(),
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
            v.dock.as_ref().unwrap().timeline.read(cx).scene_builds(),
            v.dock.as_ref().unwrap().timeline.read(cx).scene_paints(),
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
