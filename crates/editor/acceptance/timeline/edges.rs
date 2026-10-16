use super::super::harness::{Harness, command, update};
use super::common::{clip, drain, edit, select};
use anyhow::{Context as _, ensure};
use editor_core::{CommandId, TimelineChannelId, TimelineCommand};
use kasane_sdk::{CompiledCurve, sample_motion_curve};
use serde_json::{Value, json};

pub(crate) fn edges(h: &mut Harness) -> anyhow::Result<Value> {
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
