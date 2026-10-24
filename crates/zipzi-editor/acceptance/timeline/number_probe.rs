//! Focused release probe for Timeline channel-number scrubbing.
//! Measures GPUI dispatch, CPU stages and current Metal draw, not compositor FPS.
use super::*;
use crate::frame_trace;
use anyhow::{Context as _, ensure};
use gpui_kit::{point, px, test::TestWindowExt};
use serde_json::Value;
use std::time::{Duration, Instant};
use zipzi_editor_core::{
    AnimationEvaluator, AnimationPreviewMode, TimelineCommand, TimelineDisplayMode,
};

pub(super) fn run(h: &mut Harness, samples: usize) -> anyhow::Result<Value> {
    let mut reports = serde_json::Map::new();
    let source_probe = std::env::var("ZIPZI_TIMELINE_NUMBER_PROBE").is_ok_and(|v| v == "source");
    let variants = if source_probe {
        vec!["Source"]
    } else {
        vec!["Natural", "Dense"]
    };
    for variant in variants {
        if source_probe {
            update(h, |view, _, cx| -> anyhow::Result<()> {
                let resource = view
                    .editor
                    .document()
                    .unwrap()
                    .animation_catalog()
                    .motions
                    .iter()
                    .find(|motion| motion.playable())
                    .cloned()
                    .context("no playable source motion")?;
                view.editor
                    .select_motion(view.editor.active_id().unwrap(), resource.selection)?;
                view.redraw(cx);
                Ok(())
            })??;
        } else {
            select(h, &format!("Timeline{variant}"))?;
        }
        update(h, |view, _, cx| {
            let doc = view.editor.active_id().unwrap();
            view.editor
                .set_animation_preview_mode(doc, AnimationPreviewMode::Editing)
                .unwrap();
            view.redraw(cx);
        })?;
        drain(h)?;
        let projection = h
            .view
            .read_with(&h.cx, |v, _| v.editor.timeline_projection().unwrap());
        let channel = projection
            .channels
            .iter()
            .find(|c| c.runtime_id == "ParamAngleX")
            .context("ParamAngleX channel missing")?;
        let track = &projection.tracks[channel.tracks.last().context("channel has no track")?];
        let key = track.keys[0].reference.clone();
        let row = projection
            .rows
            .iter()
            .rev()
            .find(|row| row.channel.as_ref() == Some(&channel.id) && row.keys.contains(&key))
            .context("channel row missing")?;
        let (min, max) = channel.range.context("channel range missing")?;
        let base = track.keys[0].value;
        let direction = if base < (min + max) / 2. { 1. } else { -1. };
        let value_at = |i: usize| {
            (base + direction * (max - min) * (8 + (i * 7) % 64) as f32 / 400.).clamp(min, max)
        };
        let mut report = serde_json::Map::new();

        // Isolate synchronous candidate construction and detached SDK stages.
        let token = update(h, |view, _, _| {
            view.editor
                .begin_timeline_edit(view.editor.active_id().unwrap(), "Number probe")
        })??;
        let mut candidate = Vec::new();
        let mut creation = Vec::new();
        let mut sampling = Vec::new();
        let mut geometry = Vec::new();
        let mut cached_geometry = Vec::new();
        let mut snapshot_copy = Vec::new();
        let mut projection_build = Vec::new();
        let mut evaluator = AnimationEvaluator::default();
        let mut candidate_evaluate = Vec::new();
        for i in 0..samples {
            let start = Instant::now();
            update(h, |view, _, _| {
                view.editor.update_timeline_edit(
                    view.editor.active_id().unwrap(),
                    token,
                    TimelineCommand::WriteValue {
                        channel: channel.id.clone(),
                        track_id: row.track_id.clone(),
                        selected_key: None,
                        value: value_at(i),
                    },
                )
            })??;
            candidate.push(elapsed(start));
            let request = update(h, |view, _, _| view.editor.animation_request().unwrap())?;
            let start = Instant::now();
            evaluator.evaluate(&request, |_, _| true)?;
            candidate_evaluate.push(elapsed(start));
            let current_candidate = h.view.read_with(&h.cx, |v, _| {
                v.editor.timeline_edit().unwrap().candidate.clone()
            });
            let start = Instant::now();
            request
                .source
                .snapshot
                .with_motion_candidate(&current_candidate)?;
            snapshot_copy.push(elapsed(start));
            let start = Instant::now();
            h.view.read_with(&h.cx, |v, _| {
                zipzi_editor_core::TimelineProjection::new(
                    v.editor.document().unwrap(),
                    &projection.clip_id,
                )
            })?;
            projection_build.push(elapsed(start));
            let start = Instant::now();
            let mut preview =
                zipzi_sdk::MotionEditingPreview::new(request.source.snapshot.document());
            creation.push(elapsed(start));
            let start = Instant::now();
            preview.sample_clip(
                request
                    .source
                    .candidate
                    .as_ref()
                    .map(|candidate| candidate.clip()),
                request.identity.time,
            )?;
            sampling.push(elapsed(start));
            let start = Instant::now();
            preview.evaluate_drawables()?;
            geometry.push(elapsed(start));
            let start = Instant::now();
            preview.evaluate_drawables()?;
            cached_geometry.push(elapsed(start));
        }
        update(h, |view, _, cx| {
            view.editor.cancel_timeline_edit();
            view.redraw(cx);
        })?;
        drain(h)?;
        report.insert(
            "cpu_stages".into(),
            json!({
                "candidate_update":stats(&candidate), "editing_preview_new":stats(&creation),
                "motion_sample":stats(&sampling), "first_geometry":stats(&geometry),
                "repeated_geometry_same_preview":stats(&cached_geometry),
                "isolated_snapshot_candidate_copy":stats(&snapshot_copy),
                "committed_projection_rebuild":stats(&projection_build),
                "cached_candidate_evaluate":stats(&candidate_evaluate),
            }),
        );

        for filtered in [false, true] {
            update(h, |view, _, cx| -> anyhow::Result<()> {
                let doc = view.editor.active_id().unwrap();
                view.editor.seek_timeline(doc, track.keys[0].time as f32)?;
                view.editor.reveal_timeline_keys(doc, &[key.clone()])?;
                view.editor.update_timeline_view(doc, |state| {
                    state.search = if filtered {
                        channel.runtime_id.clone()
                    } else {
                        String::new()
                    };
                    state.keys.clear();
                    state.channels = [row.id.clone()].into();
                    state.mode = TimelineDisplayMode::Overview;
                    state.snap = false;
                })?;
                view.redraw(cx);
                Ok(())
            })??;
            drain(h)?;
            let before = h.capture()?;
            let before_canvas = h.canvas_image(&before)?;
            let original = clip(h);
            let field = format!("timeline-number-RowValue({:?})", row.id);
            let origin = h.cx.update_window(h.window, |_, window, cx| {
                window.activate_window();
                window.blur(cx);
                window.render_frame(cx);
                window.within("timeline-rows").find(field).bounds().center()
            })?;
            let before_counts = h.view.read_with(&h.cx, |v, cx| {
                let timeline = v.dock.as_ref().unwrap().timeline.read(cx);
                (
                    timeline.renders(),
                    timeline.scene_builds(),
                    timeline.scene_paints(),
                )
            });
            let mut dispatch = Vec::new();
            let mut total = Vec::new();
            let mut previous_revision = 0;
            pointer(h, origin, 0)?;
            frame_trace::emit(
                frame_trace::next_id(),
                "animation_workload",
                &[("model", u64::from(filtered)), ("phase", 1)],
            );
            for i in 0..samples {
                let start = Instant::now();
                pointer(
                    h,
                    origin + point(px(direction * (8 + (i * 7) % 64) as f32), px(0.)),
                    1,
                )?;
                dispatch.push(elapsed(start));
                drain(h)?;
                total.push(elapsed(start));
                let revision = h.view.read_with(&h.cx, |v, _| {
                    v.editor.timeline_edit().map(|edit| edit.identity.revision)
                });
                ensure!(
                    revision.is_some_and(|revision| revision > previous_revision),
                    "channel-number pointer input did not advance the candidate"
                );
                previous_revision = revision.unwrap();
            }
            let after_counts = h.view.read_with(&h.cx, |v, cx| {
                let timeline = v.dock.as_ref().unwrap().timeline.read(cx);
                (
                    timeline.renders(),
                    timeline.scene_builds(),
                    timeline.scene_paints(),
                )
            });
            frame_trace::emit(
                frame_trace::next_id(),
                "animation_workload",
                &[("model", u64::from(filtered)), ("phase", 0)],
            );
            let after = h.capture()?;
            ensure!(
                h.canvas_image(&after)? != before_canvas,
                "number scrub did not change canvas pixels"
            );
            h.cx.update_window(h.window, |_, window, cx| window.press("escape", cx))?;
            pointer(h, origin, 2)?;
            drain(h)?;
            ensure!(
                clip(h) == original,
                "number scrub changed persistent motion"
            );
            let restored = h.capture()?;
            ensure!(
                h.canvas_image(&restored)? == before_canvas,
                "cancel did not restore canvas"
            );
            let metrics = h.view.read_with(&h.cx, |v, _| v.editor.canvas_metrics());
            report.insert(
                if filtered {
                    "filtered_channel"
                } else {
                    "all_channels"
                }
                .into(),
                json!({
                    "dispatch":stats(&dispatch), "dispatch_to_current_metal_draw":stats(&total),
                    "timeline_renders":after_counts.0-before_counts.0,
                    "timeline_scene_builds":after_counts.1-before_counts.1,
                    "timeline_scene_paints":after_counts.2-before_counts.2,
                    "pixels_changed_and_cancel_restored":true,
                    "canvas":{"width":metrics.width,"height":metrics.height,"scale_factor":metrics.scale_factor},
                }),
            );
        }
        reports.insert(variant.into(), Value::Object(report));
    }
    let parameter = h.view.read_with(&h.cx, |v, _| {
        let document = v.editor.document().unwrap();
        let id = document
            .animation_catalog()
            .parameters
            .iter()
            .find(|parameter| parameter.runtime_id == "ParamAngleX")
            .unwrap()
            .id
            .clone();
        document
            .parameters()
            .into_iter()
            .find(|parameter| parameter.id == id)
            .unwrap()
    });
    command(h, CommandId::ModelingWorkspace)?;
    let before = h.capture()?;
    let before_canvas = h.canvas_image(&before)?;
    let origin = h.cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.blur(cx);
        window.render_frame(cx);
        window
            .find(format!("parameter-value-{}", parameter.id))
            .bounds()
            .center()
    })?;
    let direction = if parameter.value < (parameter.minimum + parameter.maximum) / 2. {
        1.
    } else {
        -1.
    };
    let mut dispatch = Vec::new();
    let mut total = Vec::new();
    let mut previous_value = parameter.value;
    pointer(h, origin, 0)?;
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", 2), ("phase", 1)],
    );
    for i in 0..samples {
        let start = Instant::now();
        pointer(
            h,
            origin + point(px(direction * (8 + (i * 7) % 64) as f32), px(0.)),
            1,
        )?;
        dispatch.push(elapsed(start));
        let value = h.view.read_with(&h.cx, |v, _| {
            v.editor
                .document()
                .unwrap()
                .parameters()
                .into_iter()
                .find(|p| p.id == parameter.id)
                .unwrap()
                .value
        });
        ensure!(
            value != previous_value,
            "authoring input did not change the parameter"
        );
        previous_value = value;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            pump(h)?;
            let ready = h.view.read_with(&h.cx, |v, _| {
                v.modeling_frame()
                    .as_ref()
                    .is_some_and(|frame| v.editor.accepts_preview(frame.canvas.version.request))
            });
            if ready {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "authoring parameter did not reach canvas"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        total.push(elapsed(start));
    }
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", 2), ("phase", 0)],
    );
    let after = h.capture()?;
    ensure!(
        h.canvas_image(&after)? != before_canvas,
        "authoring parameter did not change canvas pixels"
    );
    h.cx.update_window(h.window, |_, window, cx| window.press("escape", cx))?;
    pointer(h, origin, 2)?;
    let restored = h.capture()?;
    ensure!(
        h.canvas_image(&restored)? == before_canvas,
        "authoring cancel did not restore canvas pixels"
    );
    let metrics = h.view.read_with(&h.cx, |v, _| v.editor.canvas_metrics());
    reports.insert(
        "authoring_parameter".into(),
        json!({"dispatch":stats(&dispatch), "dispatch_to_current_metal_draw":stats(&total), "pixels_changed_and_cancel_restored":true,
            "canvas":{"width":metrics.width,"height":metrics.height,"scale_factor":metrics.scale_factor}}),
    );
    Ok(
        json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},
        "measurement":"injected GPUI input to current SDK/Metal canvas draw; excludes compositor presentation",
        "samples":samples,"variants":reports}),
    )
}
