//! Value-only curve algorithms. Commands install the resulting clip through Builder.
//! No source registry, Session, history or publication state is retained here.
mod retiming;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use zipzi_core::document::MotionCurve;
use zipzi_core::document::MotionTrackEditing;

use zipzi_animation::{
    motion_bezier_parameter, motion_bezier_time_is_monotonic, sample_motion_curve_base,
    CompiledCurve, CompiledSegment,
};
use zipzi_core::document::{
    supported_model_motion_channel, MotionClip, MotionKey, MotionPoint, MotionSegment, MotionTrack,
    MotionTrackTarget,
};
use zipzi_core::state::DocumentBuilder;

use crate::Status;
#[cfg(test)]
use zipzi_animation::sample_motion_curve;
use zipzi_contract::motion_curve::{MotionEasingPreset, MotionFrameStep};

pub use zipzi_contract::kernel_commands::{
    MotionBezierHandle, MotionCurveEdit, MotionInterpolation, MotionKeyMove, MotionKeyRef,
    MotionRetimeFixedEnd,
};

/// One independently owned clip, suitable for immutable preview snapshots.
/// Rebuild from the gesture's original clip for each total displacement. UUIDs
/// come from the caller, so a gesture can reuse its insertion IDs across updates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CurveWorkspace {
    clip: MotionClip,
}

fn error(code: &str, message: impl AsRef<str>) -> Status {
    Status::error(code, message.as_ref())
}

fn collision(track_id: &str, first: &str, second: &str) -> Status {
    error(
        "MOTION_KEY_COLLISION",
        format!("{track_id}: key times for {first} and {second} must stay strictly increasing"),
    )
}

fn validate(document: &DocumentBuilder, clip: &MotionClip) -> Result<(), Status> {
    let status = document.validate_motion(clip);
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}

fn editable(track: &MotionTrack) -> Result<(), Status> {
    match &track.target {
        MotionTrackTarget::Parameter { .. } | MotionTrackTarget::PartOpacity { .. } => Ok(()),
        MotionTrackTarget::Model { runtime_id } if supported_model_motion_channel(runtime_id) => {
            Ok(())
        }
        _ => Err(error("READ_ONLY_MOTION_TRACK", &track.id)),
    }
}

fn track_mut<'a>(clip: &'a mut MotionClip, id: &str) -> Result<&'a mut MotionTrack, Status> {
    let track = clip
        .tracks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| error("MISSING_MOTION_TRACK", id))?;
    editable(track)?;
    Ok(track)
}

fn key_index(track: &MotionTrack, id: &str) -> Result<usize, Status> {
    track
        .keys()
        .position(|k| k.id == id)
        .ok_or_else(|| error("MISSING_MOTION_KEY", id))
}

fn time_in_clip(clip: &MotionClip, time: f32) -> Result<(), Status> {
    if time.is_finite() && time >= 0.0 && time <= clip.duration {
        Ok(())
    } else {
        Err(error(
            "INVALID_MOTION_KEY_TIME",
            "Edited keys must be in [0, duration]",
        ))
    }
}

fn value_in_track(
    document: &DocumentBuilder,
    track: &MotionTrack,
    value: f32,
) -> Result<(), Status> {
    let (min, max) = match &track.target {
        MotionTrackTarget::Parameter { parameter_id } => {
            let parameter = document
                .parameter(parameter_id)
                .ok_or_else(|| error("MISSING_PARAMETER", parameter_id))?;
            (parameter.minimum, parameter.maximum)
        }
        _ => (0.0, 1.0),
    };
    if value.is_finite() && value >= min && value <= max {
        Ok(())
    } else {
        Err(error(
            "MOTION_VALUE_OUT_OF_RANGE",
            format!("{}: {value} outside [{min}, {max}]", track.id),
        ))
    }
}

fn thirds(start: f32, end: f32) -> (f32, f32) {
    let span = f64::from(end) - f64::from(start);
    (
        (f64::from(start) + span / 3.0) as f32,
        (f64::from(end) - span / 3.0) as f32,
    )
}

fn constrain_handles(
    start: f32,
    end: f32,
    first: &mut MotionPoint,
    second: &mut MotionPoint,
    restricted: bool,
) {
    if restricted {
        (first.time, second.time) = thirds(start, end);
    } else {
        first.time = first.time.clamp(start, end);
        second.time = second.time.clamp(start, end);
    }
}

fn is_monotonic(start: f32, segment: &MotionSegment) -> bool {
    match segment {
        MotionSegment {
            end,
            curve: MotionCurve::Bezier {
                control1, control2, ..
            },
            ..
        } => motion_bezier_time_is_monotonic(start, control1.time, control2.time, end.time),
        _ => true,
    }
}

fn outgoing_targets(
    document: &DocumentBuilder,
    clip: &MotionClip,
    keys: &[MotionKeyRef],
) -> Result<Vec<(usize, usize)>, Status> {
    let tracks: HashMap<_, _> = clip
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();
    let mut indices = HashMap::new();
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for key in keys {
        let track_index = *tracks
            .get(key.track_id.as_str())
            .ok_or_else(|| error("MISSING_MOTION_TRACK", &key.track_id))?;
        let track = &clip.tracks[track_index];
        let keys = indices.entry(track_index).or_insert_with(|| {
            track
                .keys()
                .enumerate()
                .map(|(i, k)| (k.id.as_str(), i))
                .collect::<HashMap<_, _>>()
        });
        let index = *keys
            .get(key.key_id.as_str())
            .ok_or_else(|| error("MISSING_MOTION_KEY", &key.key_id))?;
        if index < track.segments.len() && seen.insert((track_index, index)) {
            editable(track)?;
            if let Some(id) = zipzi_core::document::motion_interval_lock(
                document.editor_state(),
                &clip.id,
                &track.id,
                &key.key_id,
                &track.segments[index].end.id,
            ) {
                return Err(error("OBJECT_LOCKED", id));
            }
            targets.push((track_index, index));
        }
    }
    Ok(targets)
}

/// In normalized time the cubic differs from u by at most the largest
/// control-time rounding error (convex hull property). Multiplying by a
/// bound on |dy/du| bounds the change at every inverse-time sample.
fn check_unrestricted_precision(
    start: MotionPoint,
    first: MotionPoint,
    second: MotionPoint,
    end: MotionPoint,
) -> Result<(), Status> {
    if start.time.next_up() >= end.time {
        return Ok(());
    }
    let span = f64::from(end.time) - f64::from(start.time);
    let normalized = |time: f32| (f64::from(time) - f64::from(start.time)) / span;
    let time_error = (normalized(first.time) - 1. / 3.)
        .abs()
        .max((normalized(second.time) - 2. / 3.).abs());
    let values = [start.value, first.value, second.value, end.value].map(f64::from);
    let derivative_bound = 3.
        * values
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0., f64::max);
    let tolerance = bezier_value_tolerance(values);
    if derivative_bound * time_error <= tolerance {
        return Ok(());
    }
    // A third rounded by more than the normalized tolerance means the interval
    // spans few f32 ticks relative to its origin. Check its actual samples:
    // even rounded, coincident controls can preserve every representable time.
    let segment = CompiledSegment::Bezier {
        control1: first,
        control2: second,
        end,
    };
    let mut time = start.time.next_up();
    while time < end.time {
        let restricted = zipzi_animation::sample_motion_segment(start, &segment, time, true);
        let unrestricted = zipzi_animation::sample_motion_segment(start, &segment, time, false);
        if (f64::from(restricted) - f64::from(unrestricted)).abs() > tolerance {
            return Err(error("MOTION_BEZIER_CONVERSION_PRECISION",
                "The stored control times cannot preserve this restricted Bezier within numerical tolerance"));
        }
        time = time.next_up();
    }
    Ok(())
}

fn bezier_value_tolerance(values: [f64; 4]) -> f64 {
    let minimum = values.into_iter().fold(f64::INFINITY, f64::min);
    let maximum = values.into_iter().fold(f64::NEG_INFINITY, f64::max);
    0.0001 * (maximum - minimum).max(1.0)
}

fn unrestrict_beziers(
    clip: &mut MotionClip,
    document: &DocumentBuilder,
    targets: &[(usize, usize)],
) -> Result<(), Status> {
    let replaced: HashSet<_> = targets.iter().copied().collect();
    for (track_index, track) in clip.tracks.iter_mut().enumerate() {
        if !track.segments.iter().any(|s| {
            matches!(
                s.curve,
                MotionCurve::Bezier { .. } | MotionCurve::AutoSmooth {}
            )
        }) {
            continue;
        }
        let compiled = CompiledCurve::from_track(track, clip.fps);
        for (index, base) in compiled.segments.iter().enumerate() {
            let CompiledSegment::Bezier {
                control1: mut first,
                control2: mut second,
                end,
            } = *base
            else {
                continue;
            };
            if replaced.contains(&(track_index, index)) {
                continue;
            }
            let start = index
                .checked_sub(1)
                .map_or(&track.initial, |i| &track.segments[i].end);
            let segment = &track.segments[index];
            if matches!(segment.curve, MotionCurve::Bezier { .. }) {
                if let Some(id) = zipzi_core::document::motion_interval_lock(
                    document.editor_state(),
                    &clip.id,
                    &track.id,
                    &start.id,
                    &segment.end.id,
                ) {
                    return Err(error("OBJECT_LOCKED", id));
                }
            }
            // A validated compatible preset already stores its rounded thirds.
            // Keep its exact controls and provenance through the mode change.
            let custom = matches!(segment.curve, MotionCurve::Bezier { preset: None, .. });
            if custom {
                (first.time, second.time) = thirds(start.time, segment.end.time);
            }
            check_unrestricted_precision(start.point(), first, second, end)?;
            if custom {
                Arc::make_mut(&mut track.segments)[index].curve = MotionCurve::Bezier {
                    control1: first,
                    control2: second,
                    preset: None,
                };
            }
        }
    }
    clip.restricted_beziers = false;
    Ok(())
}

impl CurveWorkspace {
    pub fn new(document: &DocumentBuilder, clip: MotionClip) -> Result<Self, Status> {
        let original = document
            .motion(&clip.id)
            .ok_or_else(|| error("MISSING_MOTION", &clip.id))?;
        if original.has_extensions() || clip.has_extensions() {
            return Err(error(
                "OPAQUE_EDIT_REQUIRES_IMPORT",
                "Reimport motion3 to edit unknown fields",
            ));
        }
        validate(document, &clip)?;
        Ok(Self { clip })
    }

    #[cfg(test)]
    pub fn clip(&self) -> &MotionClip {
        &self.clip
    }
    pub fn into_clip(self) -> MotionClip {
        self.clip
    }

    fn apply(
        &mut self,
        document: &DocumentBuilder,
        change: impl FnOnce(&mut MotionClip) -> Result<(), Status>,
    ) -> Result<(), Status> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("sdk.motion_candidate_apply");
        let mut clip = self.clip.clone();
        change(&mut clip)?;
        validate(document, &clip)?;
        self.clip = clip;
        Ok(())
    }

    /// Apply every absolute key position simultaneously, then validate once.
    /// Does not reorder, merge, overwrite or quantize keys. Adjacent Bezier
    /// controls follow their owning endpoints; unchanged segments stay shared.
    pub fn move_keys(
        &mut self,
        document: &DocumentBuilder,
        moves: &[MotionKeyMove],
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let mut by_track: HashMap<&str, HashMap<&str, MotionPoint>> = HashMap::new();
            for item in moves {
                time_in_clip(clip, item.point.time)?;
                if by_track
                    .entry(&item.key.track_id)
                    .or_default()
                    .insert(&item.key.key_id, item.point)
                    .is_some()
                {
                    return Err(error("DUPLICATE_MOTION_KEY_EDIT", &item.key.key_id));
                }
            }
            let restricted = clip.restricted_beziers;
            for (id, points) in by_track {
                let track = track_mut(clip, id)?;
                let keys: HashMap<_, _> = track.keys().map(|key| (key.id.as_str(), key)).collect();
                for (&key_id, &point) in &points {
                    let key = keys
                        .get(key_id)
                        .ok_or_else(|| error("MISSING_MOTION_KEY", key_id))?;
                    if point.value != key.value {
                        value_in_track(document, track, point.value)?;
                    }
                }
                let position = |key: &MotionKey| {
                    points
                        .get(key.id.as_str())
                        .copied()
                        .unwrap_or_else(|| key.point())
                };
                let original = track.clone();
                let mut start = &original.initial;
                for segment in original.segments.iter() {
                    let end = segment.end();
                    if position(start).time >= position(end).time {
                        return Err(collision(id, &start.id, &end.id));
                    }
                    if !restricted
                        && !is_monotonic(start.time, segment)
                        && (position(start).time != start.time || position(end).time != end.time)
                    {
                        return Err(error(
                            "NON_MONOTONIC_MOTION_BEZIER",
                            "Time edits cannot preserve the runtime inverse-time branch",
                        ));
                    }
                    start = end;
                }
                let initial = position(&original.initial);
                track.initial.time = initial.time;
                track.initial.value = initial.value;
                let mut start = &original.initial;
                // Copy the segment allocation only when an endpoint really changes.
                for (index, segment) in original.segments.iter().enumerate() {
                    let end = segment.end();
                    let new_start = position(start);
                    let new_end = position(end);
                    if new_start != start.point() || new_end != end.point() {
                        let slot = &mut Arc::make_mut(&mut track.segments)[index];
                        slot.end_mut().time = new_end.time;
                        slot.end_mut().value = new_end.value;
                        if let MotionSegment {
                            curve:
                                MotionCurve::Bezier {
                                    control1,
                                    control2,
                                    preset,
                                },
                            ..
                        } = slot
                        {
                            control1.time += new_start.time - start.time;
                            control1.value += new_start.value - start.value;
                            control2.time += new_end.time - end.time;
                            control2.value += new_end.value - end.value;
                            // Keep imported non-monotone time coordinates untouched
                            // during the supported endpoint-value-only operation.
                            if restricted
                                || (new_start.time - start.time != new_end.time - end.time
                                    && is_monotonic(start.time, segment))
                            {
                                constrain_handles(
                                    new_start.time,
                                    new_end.time,
                                    control1,
                                    control2,
                                    restricted,
                                );
                            }
                            if let Some(source) = *preset {
                                let common = new_end.time
                                    == end.time + (new_start.time - start.time)
                                    && new_end.value == end.value + (new_start.value - start.value);
                                if common {
                                    [*control1, *control2] =
                                        source.mapped_controls(new_start, new_end).unwrap();
                                } else {
                                    *preset = None;
                                }
                            }
                        }
                    }
                    start = end;
                }
            }
            Ok(())
        })
    }

    /// Insert a new UUID at exact f32 time. `None` samples the original curve;
    /// `Some` first splits it, then moves the inserted value and its handles.
    /// The first authored interval is Auto Smooth; later exterior connections are
    /// Linear. Existing keys retain their identities.
    pub fn insert_key(
        &mut self,
        document: &DocumentBuilder,
        track_id: &str,
        key_id: &str,
        time: f32,
        value: Option<f32>,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            time_in_clip(clip, time)?;
            let restricted = clip.restricted_beziers;
            let fps = clip.fps;
            let track = track_mut(clip, track_id)?;
            if let Some(value) = value {
                value_in_track(document, track, value)?;
            }
            if let Some(existing) = track.keys().find(|k| k.time == time) {
                return Err(collision(track_id, &existing.id, key_id));
            }
            let sampled = sample_motion_curve_base(
                &CompiledCurve::from_track(&*track, fps),
                time,
                restricted,
            );
            let mut key = MotionKey {
                id: key_id.into(),
                time,
                value: sampled,
            };
            if time < track.initial.time {
                // Sampling an ordinary Bezier before its initial time may follow
                // its inverse branch; endpoint insertion always holds that endpoint.
                key.value = track.initial.value;
                let old = std::mem::replace(&mut track.initial, key);
                let curve = if track.segments.is_empty() {
                    MotionCurve::AutoSmooth {}
                } else {
                    MotionCurve::Linear {}
                };
                Arc::make_mut(&mut track.segments).insert(0, MotionSegment::new(old, curve));
            } else if let Some(index) = track.segments.iter().position(|s| time < s.end().time) {
                let start = if index == 0 {
                    track.initial.point()
                } else {
                    track.segments[index - 1].end().point()
                };
                let (left, right) = split(start, &track.segments[index], key, restricted)?;
                Arc::make_mut(&mut track.segments).splice(index..=index, [left, right]);
            } else {
                let curve = if track.segments.is_empty() {
                    MotionCurve::AutoSmooth {}
                } else {
                    MotionCurve::Linear {}
                };
                Arc::make_mut(&mut track.segments).push(MotionSegment::new(key, curve));
            }
            if let Some(value) = value {
                // Reuse handle-following semantics without publishing an intermediate state.
                let mut candidate = Self { clip: clip.clone() };
                candidate.move_keys(
                    document,
                    &[MotionKeyMove {
                        key: MotionKeyRef {
                            track_id: track_id.into(),
                            key_id: key_id.into(),
                        },
                        point: MotionPoint { time, value },
                    }],
                )?;
                *clip = candidate.into_clip();
            }
            Ok(())
        })
    }

    /// Delete all requested keys using the final surviving neighbours, independent
    /// of request order. Deleting a whole track removes it from the clip.
    pub fn delete_keys(
        &mut self,
        document: &DocumentBuilder,
        keys: &[MotionKeyRef],
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let mut by_track: HashMap<&str, HashSet<&str>> = HashMap::new();
            for key in keys {
                by_track
                    .entry(&key.track_id)
                    .or_default()
                    .insert(&key.key_id);
            }
            let restricted = clip.restricted_beziers;
            let mut empty = HashSet::new();
            for (id, removed) in by_track {
                let track = track_mut(clip, id)?;
                let existing: HashSet<_> = track.keys().map(|key| key.id.as_str()).collect();
                for &id in &removed {
                    if !existing.contains(id) {
                        return Err(error("MISSING_MOTION_KEY", id));
                    }
                }
                if !track.retain_keys(|id| !removed.contains(id), restricted) {
                    empty.insert(id.to_owned());
                }
            }
            clip.tracks.retain(|t| !empty.contains(&t.id));
            clip.track_order.retain(|id| !empty.contains(id));
            Ok(())
        })
    }

    /// Apply to selected outgoing intervals. Endpoint-only keys are ignored.
    /// Auto Smooth to Bezier materializes the pre-operation compiled handles.
    pub fn set_interpolation(
        &mut self,
        document: &DocumentBuilder,
        keys: &[MotionKeyRef],
        kind: MotionInterpolation,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let targets = outgoing_targets(document, clip, keys)?;
            let mut compiled = HashMap::new();
            if kind == MotionInterpolation::Bezier {
                for &(track, _) in &targets {
                    compiled.entry(track).or_insert_with(|| {
                        CompiledCurve::from_track(&clip.tracks[track], clip.fps)
                    });
                }
            }
            for (track_index, index) in targets {
                let track = &mut clip.tracks[track_index];
                let start = track.keys().nth(index).unwrap().point();
                let segment = &track.segments[index];
                let curve = match kind {
                    MotionInterpolation::Linear => MotionCurve::Linear {},
                    MotionInterpolation::AutoSmooth => MotionCurve::AutoSmooth {},
                    MotionInterpolation::Stepped => MotionCurve::Stepped {},
                    MotionInterpolation::InverseStepped => MotionCurve::InverseStepped {},
                    MotionInterpolation::Bezier => {
                        if matches!(segment.curve, MotionCurve::Bezier { .. }) {
                            continue;
                        }
                        if matches!(segment.curve, MotionCurve::AutoSmooth {}) {
                            let CompiledSegment::Bezier {
                                control1, control2, ..
                            } = compiled[&track_index].segments[index]
                            else {
                                unreachable!()
                            };
                            MotionCurve::Bezier {
                                control1,
                                control2,
                                preset: None,
                            }
                        } else {
                            let end = segment.end.point();
                            MotionCurve::Bezier {
                                control1: lerp(start, end, 1. / 3.),
                                control2: lerp(start, end, 2. / 3.),
                                preset: None,
                            }
                        }
                    }
                };
                if segment.curve != curve {
                    Arc::make_mut(&mut track.segments)[index].curve = curve;
                }
            }
            Ok(())
        })
    }

    /// Presets use each interval's own time/value range and retain its N.
    pub fn set_easing_preset(
        &mut self,
        document: &DocumentBuilder,
        keys: &[MotionKeyRef],
        preset: MotionEasingPreset,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let targets = outgoing_targets(document, clip, keys)?;
            if !targets.is_empty() && clip.restricted_beziers && !preset.supports_restricted_time()
            {
                unrestrict_beziers(clip, document, &targets)?;
            }
            for (track_index, index) in targets {
                let track = &mut clip.tracks[track_index];
                let start = track.keys().nth(index).unwrap().point();
                let end = track.segments[index].end.point();
                let [control1, control2] = preset.mapped_controls(start, end).unwrap();
                let curve = MotionCurve::Bezier {
                    control1,
                    control2,
                    preset: Some(preset),
                };
                if track.segments[index].curve != curve {
                    Arc::make_mut(&mut track.segments)[index].curve = curve;
                }
            }
            Ok(())
        })
    }

    pub fn set_frame_step(
        &mut self,
        document: &DocumentBuilder,
        keys: &[MotionKeyRef],
        step: MotionFrameStep,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let step = step.normalized().ok_or_else(|| {
                error("INVALID_MOTION_FRAME_STEP", "Frame count must be positive")
            })?;
            for (track_index, index) in outgoing_targets(document, clip, keys)? {
                let track = &mut clip.tracks[track_index];
                if track.segments[index].frame_step != step {
                    Arc::make_mut(&mut track.segments)[index].frame_step = step;
                }
            }
            Ok(())
        })
    }

    /// A handle is addressed by its segment's endpoint UUID. Restricted Beziers
    /// accept vertical changes at their effective one-third/two-thirds positions.
    pub fn set_handle(
        &mut self,
        document: &DocumentBuilder,
        end_key: &MotionKeyRef,
        handle: MotionBezierHandle,
        point: MotionPoint,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            if !point.time.is_finite() || !point.value.is_finite() {
                return Err(error("INVALID_MOTION_BEZIER", &end_key.key_id));
            }
            let restricted = clip.restricted_beziers;
            let fps = clip.fps;
            let track = track_mut(clip, &end_key.track_id)?;
            let index = key_index(track, &end_key.key_id)?
                .checked_sub(1)
                .ok_or_else(|| error("MISSING_MOTION_SEGMENT", &end_key.key_id))?;
            let start = track.keys().nth(index).unwrap().time;
            if matches!(track.segments[index].curve, MotionCurve::AutoSmooth {}) {
                let curve = CompiledCurve::from_track(track, fps);
                let CompiledSegment::Bezier {
                    control1, control2, ..
                } = curve.segments[index]
                else {
                    unreachable!()
                };
                Arc::make_mut(&mut track.segments)[index].curve = MotionCurve::Bezier {
                    control1,
                    control2,
                    preset: None,
                };
            }
            let segment = &mut Arc::make_mut(&mut track.segments)[index];
            let MotionSegment {
                end,
                curve:
                    MotionCurve::Bezier {
                        control1,
                        control2,
                        preset,
                    },
                ..
            } = segment
            else {
                return Err(error("NOT_MOTION_BEZIER", &end_key.key_id));
            };
            if restricted {
                (control1.time, control2.time) = thirds(start, end.time);
                let expected = match handle {
                    MotionBezierHandle::Control1 => control1.time,
                    MotionBezierHandle::Control2 => control2.time,
                };
                if point.time != expected {
                    return Err(error(
                        "RESTRICTED_BEZIER_TIME",
                        "Restricted Bezier handles only move vertically",
                    ));
                }
            }
            match handle {
                MotionBezierHandle::Control1 => *control1 = point,
                MotionBezierHandle::Control2 => *control2 = point,
            }
            if !(start <= control1.time
                && control1.time <= end.time
                && start <= control2.time
                && control2.time <= end.time)
                || !motion_bezier_time_is_monotonic(start, control1.time, control2.time, end.time)
            {
                return Err(error(
                    "INVALID_MOTION_BEZIER",
                    "Handle times must define a monotone curve inside its interval",
                ));
            }
            *preset = None;
            Ok(())
        })
    }

    /// Align the other handle at the shared key without forcing equal length.
    pub fn set_handle_aligned(
        &mut self,
        document: &DocumentBuilder,
        end_key: &MotionKeyRef,
        handle: MotionBezierHandle,
        point: MotionPoint,
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            let fps = clip.fps;
            let track = track_mut(clip, &end_key.track_id)?;
            let index = key_index(track, &end_key.key_id)?
                .checked_sub(1)
                .ok_or_else(|| error("MISSING_MOTION_SEGMENT", &end_key.key_id))?;
            let anchor_index = match handle {
                MotionBezierHandle::Control1 => index,
                MotionBezierHandle::Control2 => index + 1,
            };
            let anchor = track.keys().nth(anchor_index).unwrap().point();
            let other_index = match handle {
                MotionBezierHandle::Control1 => index.checked_sub(1),
                MotionBezierHandle::Control2 => {
                    (index + 1 < track.segments.len()).then_some(index + 1)
                }
            };
            let compiled = CompiledCurve::from_track(track, fps);
            for i in std::iter::once(index).chain(other_index) {
                if matches!(track.segments[i].curve, MotionCurve::AutoSmooth {}) {
                    let CompiledSegment::Bezier {
                        control1, control2, ..
                    } = compiled.segments[i]
                    else {
                        unreachable!()
                    };
                    Arc::make_mut(&mut track.segments)[i].curve = MotionCurve::Bezier {
                        control1,
                        control2,
                        preset: None,
                    };
                }
            }
            let opposite = other_index.and_then(|i| {
                if let MotionCurve::Bezier {
                    control1, control2, ..
                } = track.segments[i].curve
                {
                    let (old, other_handle) = match handle {
                        MotionBezierHandle::Control1 => (control2, MotionBezierHandle::Control2),
                        MotionBezierHandle::Control2 => (control1, MotionBezierHandle::Control1),
                    };
                    Some((track.segments[i].end.id.clone(), old, other_handle))
                } else {
                    None
                }
            });
            let mut candidate = Self { clip: clip.clone() };
            candidate.set_handle(document, end_key, handle, point)?;
            if let Some((id, old, other_handle)) = opposite {
                let dx = f64::from(point.time) - f64::from(anchor.time);
                let dy = f64::from(point.value) - f64::from(anchor.value);
                let length = dx.hypot(dy);
                if length > 0. {
                    let old_length = (f64::from(old.time) - f64::from(anchor.time))
                        .hypot(f64::from(old.value) - f64::from(anchor.value));
                    let scale = old_length / length;
                    candidate.set_handle(
                        document,
                        &MotionKeyRef {
                            track_id: end_key.track_id.clone(),
                            key_id: id,
                        },
                        other_handle,
                        MotionPoint {
                            time: (f64::from(anchor.time) - dx * scale) as f32,
                            value: (f64::from(anchor.value) - dy * scale) as f32,
                        },
                    )?;
                }
            }
            *clip = candidate.into_clip();
            Ok(())
        })
    }

    /// Tidy restores straight thirds; the explicit user operation clears source
    /// labels and retains frame stepping. All selected intervals commit together.
    pub fn tidy_handles(
        &mut self,
        document: &DocumentBuilder,
        keys: &[MotionKeyRef],
    ) -> Result<(), Status> {
        self.apply(document, |clip| {
            for (track_index, index) in outgoing_targets(document, clip, keys)? {
                let track = &mut clip.tracks[track_index];
                if !matches!(
                    track.segments[index].curve,
                    MotionCurve::Bezier { .. } | MotionCurve::AutoSmooth {}
                ) {
                    return Err(error("NOT_MOTION_BEZIER", &track.segments[index].end.id));
                }
                let start = track.keys().nth(index).unwrap().point();
                let end = track.segments[index].end.point();
                let (first, second) = thirds(start.time, end.time);
                Arc::make_mut(&mut track.segments)[index].curve = MotionCurve::Bezier {
                    control1: MotionPoint {
                        time: first,
                        value: (f64::from(start.value)
                            + (f64::from(end.value) - f64::from(start.value)) / 3.)
                            as f32,
                    },
                    control2: MotionPoint {
                        time: second,
                        value: (f64::from(end.value)
                            - (f64::from(end.value) - f64::from(start.value)) / 3.)
                            as f32,
                    },
                    preset: None,
                };
            }
            Ok(())
        })
    }

    /// Explicit duration scaling transforms keys, control times and events together.
    /// Replacing clip duration/FPS directly does not rescale curves.
    pub fn scale_time(&mut self, document: &DocumentBuilder, duration: f32) -> Result<(), Status> {
        self.apply(document, |clip| {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(error("INVALID_MOTION_DURATION", &clip.id));
            }
            if duration == clip.duration {
                return Ok(());
            }
            // Unknown/unresolved tracks are read-only, including bulk time commands.
            for track in &clip.tracks {
                editable(track)?;
            }
            let ratio = f64::from(duration) / f64::from(clip.duration);
            let scale = |time: f32| (f64::from(time) * ratio) as f32;
            let source_range = clip.source_key_range();
            let restricted = clip.restricted_beziers;
            for track in &mut clip.tracks {
                track.initial.time = scale(track.initial.time);
                let mut start = track.initial.point();
                for segment in Arc::make_mut(&mut track.segments) {
                    segment.end_mut().time = scale(segment.end().time);
                    let end = segment.end().time;
                    let end_point = segment.end.point();
                    zipzi_core::document::validate_motion_interval_times(start.time, end)?;
                    if let MotionSegment {
                        curve:
                            MotionCurve::Bezier {
                                control1,
                                control2,
                                preset,
                            },
                        ..
                    } = segment
                    {
                        control1.time = scale(control1.time);
                        control2.time = scale(control2.time);
                        if restricted {
                            constrain_handles(start.time, end, control1, control2, true);
                        }
                        if let Some(source) = preset {
                            [*control1, *control2] =
                                source.mapped_controls(start, end_point).unwrap();
                        }
                    }
                    start = end_point;
                }
            }
            for event in &mut clip.events {
                event.time = scale(event.time);
            }
            clip.duration = duration;
            if let Some(range) = source_range {
                clip.meta_extensions.insert(
                    "ZipziSourceKeyRange".into(),
                    serde_json::to_value(zipzi_core::document::MotionSourceRange {
                        start: scale(range.start),
                        end: scale(range.end),
                    })
                    .expect("finite source range serializes"),
                );
            }
            Ok(())
        })
    }
}

fn lerp(a: MotionPoint, b: MotionPoint, t: f32) -> MotionPoint {
    MotionPoint {
        time: (f64::from(a.time) + (f64::from(b.time) - f64::from(a.time)) * f64::from(t)) as f32,
        value: (f64::from(a.value) + (f64::from(b.value) - f64::from(a.value)) * f64::from(t))
            as f32,
    }
}

fn split(
    start: MotionPoint,
    segment: &MotionSegment,
    mut key: MotionKey,
    restricted: bool,
) -> Result<(MotionSegment, MotionSegment), Status> {
    let end = segment.end().clone();
    let mut result = match segment {
        MotionSegment {
            curve: MotionCurve::AutoSmooth {},
            ..
        } => (
            MotionSegment::new(key, MotionCurve::AutoSmooth {}),
            MotionSegment::new(end, MotionCurve::AutoSmooth {}),
        ),
        MotionSegment {
            curve: MotionCurve::Linear {},
            ..
        } => (
            MotionSegment::new(key, MotionCurve::Linear {}),
            MotionSegment::new(end, MotionCurve::Linear {}),
        ),
        MotionSegment {
            curve: MotionCurve::Stepped {},
            ..
        } => (
            MotionSegment::new(key, MotionCurve::Stepped {}),
            MotionSegment::new(end, MotionCurve::Stepped {}),
        ),
        MotionSegment {
            curve: MotionCurve::InverseStepped {},
            ..
        } => (
            MotionSegment::new(key, MotionCurve::InverseStepped {}),
            MotionSegment::new(end, MotionCurve::InverseStepped {}),
        ),
        MotionSegment {
            curve: MotionCurve::Bezier {
                control1, control2, ..
            },
            ..
        } => {
            if !restricted && !is_monotonic(start.time, segment) {
                return Err(error(
                    "NON_MONOTONIC_MOTION_BEZIER",
                    "Cannot split while preserving the runtime inverse-time branch",
                ));
            }
            let (mut first, mut second) = (*control1, *control2);
            if restricted {
                (first.time, second.time) = thirds(start.time, end.time);
            }
            let t =
                motion_bezier_parameter(start, first, second, end.point(), key.time, restricted);
            if !t.is_finite() || t <= 0.0 || t >= 1.0 {
                return Err(error("MOTION_BEZIER_SPLIT_PRECISION", &key.id));
            }
            let a = lerp(start, first, t);
            let b = lerp(first, second, t);
            let c = lerp(second, end.point(), t);
            let d = lerp(a, b, t);
            let e = lerp(b, c, t);
            key.value = lerp(d, e, t).value;
            let (mut a, mut d, mut e, mut c) = (a, d, e, c);
            if restricted {
                (a.time, d.time) = thirds(start.time, key.time);
                (e.time, c.time) = thirds(key.time, end.time);
            }
            let result = (
                MotionSegment::new(
                    key,
                    MotionCurve::Bezier {
                        control1: a,
                        control2: d,
                        preset: None,
                    },
                ),
                MotionSegment::new(
                    end,
                    MotionCurve::Bezier {
                        control1: e,
                        control2: c,
                        preset: None,
                    },
                ),
            );
            verify_bezier_split(start, segment, &result, restricted)?;
            result
        }
    };
    result.0.frame_step = segment.frame_step;
    result.1.frame_step = segment.frame_step;
    Ok(result)
}

fn verify_bezier_split(
    start: MotionPoint,
    source: &MotionSegment,
    split: &(MotionSegment, MotionSegment),
    restricted: bool,
) -> Result<(), Status> {
    // Rounding split controls to f32 can lose shape on short segments at large
    // absolute times. Check the shared runtime sampler before accepting a split.
    let original = CompiledCurve::compile(start, std::slice::from_ref(source), 1.);
    let candidate = CompiledCurve::compile(start, &[split.0.clone(), split.1.clone()], 1.);
    let MotionSegment {
        end,
        curve: MotionCurve::Bezier {
            control1, control2, ..
        },
        ..
    } = source
    else {
        unreachable!()
    };
    let values = [start.value, control1.value, control2.value, end.value].map(f64::from);
    let tolerance = bezier_value_tolerance(values);
    let span = f64::from(end.time) - f64::from(start.time);
    let times = (0..=128)
        .map(|i| (f64::from(start.time) + span * f64::from(i) / 128.0) as f32)
        .chain(std::iter::once(split.0.end().time));
    for time in times {
        let a = sample_motion_curve_base(&original, time, restricted);
        let b = sample_motion_curve_base(&candidate, time, restricted);
        if !a.is_finite() || !b.is_finite() || (f64::from(a) - f64::from(b)).abs() > tolerance {
            return Err(error(
                "MOTION_BEZIER_SPLIT_PRECISION",
                "The f32 runtime cannot preserve this Bezier split within numerical tolerance",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
