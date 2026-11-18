//! Clip-local affine time transforms for explicit key and event selections.
use super::*;
use zipzi_core::document::{motion_interval_lock, validate_motion_interval_times};

struct Transform {
    fixed: f32,
    moving: f32,
    target: f64,
    ratio: f64,
}

impl Transform {
    fn time(&self, time: f32) -> f32 {
        if time == self.fixed {
            time
        } else if time == self.moving {
            self.target as f32
        } else {
            let fixed = f64::from(self.fixed);
            (fixed + (f64::from(time) - fixed) * self.ratio) as f32
        }
    }
}

impl CurveWorkspace {
    pub fn retime_selection(
        &mut self,
        document: &DocumentBuilder,
        keys: &[MotionKeyRef],
        events: &[String],
        fixed_end: MotionRetimeFixedEnd,
        moving_time: f64,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let mut first = f32::INFINITY;
            let mut last = f32::NEG_INFINITY;
            let mut include = |time: f32| {
                first = first.min(time);
                last = last.max(time);
            };

            let tracks: HashMap<_, _> = clip
                .tracks
                .iter()
                .enumerate()
                .map(|(index, track)| (track.id.as_str(), index))
                .collect();
            let mut selected_keys = HashMap::<usize, HashSet<&str>>::new();
            for key in keys {
                let index = *tracks
                    .get(key.track_id.as_str())
                    .ok_or_else(|| error("MISSING_MOTION_TRACK", &key.track_id))?;
                if !selected_keys.entry(index).or_default().insert(&key.key_id) {
                    return Err(error("DUPLICATE_MOTION_KEY_EDIT", &key.key_id));
                }
            }
            for (&index, selected) in &selected_keys {
                let track = &clip.tracks[index];
                editable(track)?;
                let by_id: HashMap<_, _> = track
                    .keys()
                    .map(|key| (key.id.as_str(), key.time))
                    .collect();
                for &id in selected {
                    include(
                        *by_id
                            .get(id)
                            .ok_or_else(|| error("MISSING_MOTION_KEY", id))?,
                    );
                }
            }
            let by_id: HashMap<_, _> = clip
                .events
                .iter()
                .enumerate()
                .map(|(index, event)| (event.id.as_str(), index))
                .collect();
            let mut selected_events = HashSet::new();
            for id in events {
                let index = *by_id
                    .get(id.as_str())
                    .ok_or_else(|| error("MISSING_MOTION_EVENT", id))?;
                if !selected_events.insert(index) {
                    return Err(error("DUPLICATE_MOTION_EVENT_EDIT", id));
                }
                include(clip.events[index].time);
            }
            if first >= last {
                return Err(error(
                    "MOTION_RETIME_SPAN",
                    "Select at least two distinct times",
                ));
            }
            let (fixed, moving) = match fixed_end {
                MotionRetimeFixedEnd::Earliest => (first, last),
                MotionRetimeFixedEnd::Latest => (last, first),
            };
            // Round-trip an unchanged exposed endpoint, including imported subframes.
            if moving_time as f32 == moving && moving_time.is_finite() {
                return Ok(());
            }
            let ratio = (moving_time - f64::from(fixed)) / (f64::from(moving) - f64::from(fixed));
            if !ratio.is_finite()
                || ratio <= 0.
                || moving_time as f32 == fixed
                || moving_time < 0.
                || moving_time > f64::from(clip.duration)
            {
                return Err(error("INVALID_MOTION_RETIME_TARGET", &clip.id));
            }
            let transform = Transform {
                fixed,
                moving,
                target: moving_time,
                ratio,
            };
            let duration = clip.duration;
            let restricted = clip.restricted_beziers;
            let fps = clip.fps;
            for (index, selected) in selected_keys {
                retime_track(
                    document,
                    &clip.id,
                    &mut clip.tracks[index],
                    &selected,
                    &transform,
                    duration,
                    fps,
                    restricted,
                )?;
            }
            let mut events_moved = false;
            for index in selected_events {
                let event = &clip.events[index];
                let time = transform.time(event.time);
                if time == event.time {
                    continue;
                }
                check_time(duration, time, &event.id)?;
                check_lock(document, [&clip.id, &event.id])?;
                clip.events[index].time = time;
                events_moved = true;
            }
            if events_moved {
                clip.events
                    .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
            }
            Ok(())
        })
    }
}

fn check_time(duration: f32, time: f32, id: &str) -> Result<(), Status> {
    if time.is_finite() && (0. ..=duration).contains(&time) {
        return Ok(());
    }
    Err(error("INVALID_MOTION_RETIME_TARGET", id))
}

fn check_lock<const N: usize>(document: &DocumentBuilder, ids: [&String; N]) -> Result<(), Status> {
    if let Some(id) = ids
        .into_iter()
        .find(|id| document.editor_state().get(id).locked)
    {
        return Err(error("OBJECT_LOCKED", id));
    }
    Ok(())
}

fn retime_track(
    document: &DocumentBuilder,
    clip_id: &String,
    track: &mut MotionTrack,
    selected: &HashSet<&str>,
    transform: &Transform,
    duration: f32,
    fps: f32,
    restricted: bool,
) -> Result<(), Status> {
    let original = track.clone();
    let time = |key: &MotionKey| {
        if selected.contains(key.id.as_str()) {
            transform.time(key.time)
        } else {
            key.time
        }
    };
    let mut start = &original.initial;
    let mut new_start = time(start);
    if new_start != start.time {
        check_time(duration, new_start, &start.id)?;
        check_lock(document, [clip_id, &track.id, &start.id])?;
        track.initial.time = new_start;
    }
    let mut solve_auto = false;
    for (index, segment) in original.segments.iter().enumerate() {
        let end = segment.end();
        let new_end = time(end);
        validate_motion_interval_times(new_start, new_end)
            .map_err(|_| collision(&track.id, &start.id, &end.id))?;
        if new_start != start.time || new_end != end.time {
            if new_end != end.time {
                check_time(duration, new_end, &end.id)?;
            }
            let state = document.editor_state();
            if let Some(id) = motion_interval_lock(state, clip_id, &track.id, &start.id, &end.id) {
                return Err(error("OBJECT_LOCKED", id));
            }
            if !restricted && !is_monotonic(start.time, segment) {
                return Err(error(
                    "NON_MONOTONIC_MOTION_BEZIER",
                    format!("{}: {}", track.id, end.id),
                ));
            }
            let slot = &mut Arc::make_mut(&mut track.segments)[index];
            slot.end_mut().time = new_end;
            if let MotionCurve::Bezier {
                control1,
                control2,
                preset,
            } = &mut slot.curve
            {
                if let Some(preset) = preset {
                    [*control1, *control2] = preset
                        .mapped_controls(
                            MotionPoint {
                                time: new_start,
                                value: start.value,
                            },
                            MotionPoint {
                                time: new_end,
                                value: end.value,
                            },
                        )
                        .ok_or_else(|| error("INVALID_MOTION_EASING_PRESET", &end.id))?;
                } else if restricted {
                    (control1.time, control2.time) = thirds(new_start, new_end);
                } else {
                    let old_width = f64::from(end.time) - f64::from(start.time);
                    let new_width = f64::from(new_end) - f64::from(new_start);
                    let map = |time: f32| {
                        let normalized = (f64::from(time) - f64::from(start.time)) / old_width;
                        (f64::from(new_start) + normalized * new_width) as f32
                    };
                    control1.time = map(control1.time);
                    control2.time = map(control2.time);
                    if !motion_bezier_time_is_monotonic(
                        new_start,
                        control1.time,
                        control2.time,
                        new_end,
                    ) {
                        return Err(error(
                            "NON_MONOTONIC_MOTION_BEZIER",
                            format!("{}: {}", track.id, end.id),
                        ));
                    }
                }
            }
            solve_auto |= matches!(slot.curve, MotionCurve::AutoSmooth {});
        }
        start = end;
        new_start = new_end;
    }
    // An automatic tangent can change an interval whose stored endpoints did
    // not move. Compare the resolved curves before accepting protected edits.
    if solve_auto {
        let before = CompiledCurve::from_track(&original, fps);
        let after = CompiledCurve::from_track(track, fps);
        for ((start, end), (before, after)) in original
            .keys()
            .zip(original.keys().skip(1))
            .zip(before.segments.iter().zip(after.segments.iter()))
        {
            if before == after {
                continue;
            }
            if let Some(id) = motion_interval_lock(
                document.editor_state(),
                clip_id,
                &track.id,
                &start.id,
                &end.id,
            ) {
                return Err(error("OBJECT_LOCKED", id));
            }
        }
    }
    Ok(())
}
