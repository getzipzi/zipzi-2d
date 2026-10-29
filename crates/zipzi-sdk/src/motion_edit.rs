//! Detached, validated curve candidates. Failed operations retain the last legal
//! candidate and do not abort an SDK transaction or publish authoring history.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use zipzi_animation::{
    motion_bezier_parameter, motion_bezier_time_is_monotonic, sample_motion_curve, CompiledCurve,
};
use zipzi_core::document::{
    MotionClip, MotionKey, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
use zipzi_core::Document;

use crate::SdkError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MotionKeyRef {
    pub track_id: String,
    pub key_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionKeyMove {
    pub key: MotionKeyRef,
    /// Absolute seconds and raw channel value, calculated from the gesture origin.
    pub point: MotionPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionInterpolation {
    Linear,
    Bezier,
    Stepped,
    InverseStepped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionBezierHandle {
    Control1,
    Control2,
}

/// One independently owned clip, suitable for immutable preview snapshots.
/// Rebuild from the gesture's original clip for each total displacement. UUIDs
/// come from the caller, so a gesture can reuse its insertion IDs across updates.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionClipCandidate {
    clip: MotionClip,
}

const OP: &str = "motion_curve_candidate";

fn error(code: &str, message: impl AsRef<str>) -> SdkError {
    SdkError::new(code, message.as_ref(), OP)
}

fn collision(track_id: &str, first: &str, second: &str) -> SdkError {
    let mut error = error(
        "MOTION_KEY_COLLISION",
        "Key times must stay strictly increasing",
    );
    error.object_ids = vec![track_id.into(), first.into(), second.into()];
    error
}

fn validate(document: &Document, clip: &MotionClip) -> Result<(), SdkError> {
    let status = document.validate_motion(clip);
    if status.is_ok() {
        Ok(())
    } else {
        Err(SdkError::from_status(status, OP, vec![clip.id.clone()]))
    }
}

fn editable(track: &MotionTrack) -> Result<(), SdkError> {
    match &track.target {
        MotionTrackTarget::Parameter { .. } | MotionTrackTarget::PartOpacity { .. } => Ok(()),
        MotionTrackTarget::Model { runtime_id } if runtime_id == "Opacity" => Ok(()),
        _ => Err(error("READ_ONLY_MOTION_TRACK", &track.id)),
    }
}

fn track_mut<'a>(clip: &'a mut MotionClip, id: &str) -> Result<&'a mut MotionTrack, SdkError> {
    let track = clip
        .tracks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| error("MISSING_MOTION_TRACK", id))?;
    editable(track)?;
    Ok(track)
}

fn key_index(track: &MotionTrack, id: &str) -> Result<usize, SdkError> {
    track
        .keys()
        .position(|k| k.id == id)
        .ok_or_else(|| error("MISSING_MOTION_KEY", id))
}

fn time_in_clip(clip: &MotionClip, time: f32) -> Result<(), SdkError> {
    if time.is_finite() && time >= 0.0 && time <= clip.duration {
        Ok(())
    } else {
        Err(error(
            "INVALID_MOTION_KEY_TIME",
            "Edited keys must be in [0, duration]",
        ))
    }
}

fn value_in_track(document: &Document, track: &MotionTrack, value: f32) -> Result<(), SdkError> {
    let (min, max) = match &track.target {
        MotionTrackTarget::Parameter { parameter_id } => {
            let parameter = document
                .get_parameter(parameter_id)
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
        second.time = second.time.clamp(first.time, end);
    }
}

fn is_monotonic(start: f32, segment: &MotionSegment) -> bool {
    match segment {
        MotionSegment::Bezier {
            control1,
            control2,
            end,
        } => motion_bezier_time_is_monotonic(start, control1.time, control2.time, end.time),
        _ => true,
    }
}

impl MotionClipCandidate {
    pub fn new(document: &Document, clip: MotionClip) -> Result<Self, SdkError> {
        let original = document
            .get_motion(&clip.id)
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

    pub fn clip(&self) -> &MotionClip {
        &self.clip
    }
    pub fn into_clip(self) -> MotionClip {
        self.clip
    }

    fn apply(
        &mut self,
        document: &Document,
        change: impl FnOnce(&mut MotionClip) -> Result<(), SdkError>,
    ) -> Result<(), SdkError> {
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
        document: &Document,
        moves: &[MotionKeyMove],
    ) -> Result<(), SdkError> {
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
                        if let MotionSegment::Bezier {
                            control1, control2, ..
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
    /// At either end, the new connection is Linear and existing keys retain IDs.
    pub fn insert_key(
        &mut self,
        document: &Document,
        track_id: &str,
        key_id: &str,
        time: f32,
        value: Option<f32>,
    ) -> Result<(), SdkError> {
        self.apply(document, |clip| {
            time_in_clip(clip, time)?;
            let restricted = clip.restricted_beziers;
            let track = track_mut(clip, track_id)?;
            if let Some(value) = value {
                value_in_track(document, track, value)?;
            }
            if let Some(existing) = track.keys().find(|k| k.time == time) {
                return Err(collision(track_id, &existing.id, key_id));
            }
            let sampled = sample_motion_curve(&CompiledCurve::from(&*track), time, restricted);
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
                Arc::make_mut(&mut track.segments).insert(0, MotionSegment::Linear { end: old });
            } else if let Some(index) = track.segments.iter().position(|s| time < s.end().time) {
                let start = if index == 0 {
                    track.initial.point()
                } else {
                    track.segments[index - 1].end().point()
                };
                let (left, right) = split(start, &track.segments[index], key, restricted)?;
                Arc::make_mut(&mut track.segments).splice(index..=index, [left, right]);
            } else {
                Arc::make_mut(&mut track.segments).push(MotionSegment::Linear { end: key });
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
        document: &Document,
        keys: &[MotionKeyRef],
    ) -> Result<(), SdkError> {
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

    /// Interpolation belongs to the outgoing segment. Last keys are ignored.
    /// Switching to Bezier starts with straight one-third/two-thirds handles.
    pub fn set_interpolation(
        &mut self,
        document: &Document,
        keys: &[MotionKeyRef],
        kind: MotionInterpolation,
    ) -> Result<(), SdkError> {
        self.apply(document, |clip| {
            for key in keys {
                let track = track_mut(clip, &key.track_id)?;
                let index = key_index(track, &key.key_id)?;
                if index == track.segments.len() {
                    continue;
                }
                let start = track.keys().nth(index).unwrap().point();
                let segment = &track.segments[index];
                if matches!(
                    (kind, segment),
                    (MotionInterpolation::Linear, MotionSegment::Linear { .. })
                        | (MotionInterpolation::Bezier, MotionSegment::Bezier { .. })
                        | (MotionInterpolation::Stepped, MotionSegment::Stepped { .. })
                        | (
                            MotionInterpolation::InverseStepped,
                            MotionSegment::InverseStepped { .. }
                        )
                ) {
                    continue;
                }
                let end = segment.end().clone();
                let segment = match kind {
                    MotionInterpolation::Linear => MotionSegment::Linear { end },
                    MotionInterpolation::Stepped => MotionSegment::Stepped { end },
                    MotionInterpolation::InverseStepped => MotionSegment::InverseStepped { end },
                    MotionInterpolation::Bezier => {
                        let (first, second) = thirds(start.time, end.time);
                        let delta = f64::from(end.value) - f64::from(start.value);
                        MotionSegment::Bezier {
                            control1: MotionPoint {
                                time: first,
                                value: (f64::from(start.value) + delta / 3.0) as f32,
                            },
                            control2: MotionPoint {
                                time: second,
                                value: (f64::from(end.value) - delta / 3.0) as f32,
                            },
                            end,
                        }
                    }
                };
                Arc::make_mut(&mut track.segments)[index] = segment;
            }
            Ok(())
        })
    }

    /// A handle is addressed by its segment's endpoint UUID. Restricted Beziers
    /// accept vertical changes at their effective one-third/two-thirds positions.
    pub fn set_handle(
        &mut self,
        document: &Document,
        end_key: &MotionKeyRef,
        handle: MotionBezierHandle,
        point: MotionPoint,
    ) -> Result<(), SdkError> {
        self.apply(document, |clip| {
            if !point.time.is_finite() || !point.value.is_finite() {
                return Err(error("INVALID_MOTION_BEZIER", &end_key.key_id));
            }
            let restricted = clip.restricted_beziers;
            let track = track_mut(clip, &end_key.track_id)?;
            let index = key_index(track, &end_key.key_id)?
                .checked_sub(1)
                .ok_or_else(|| error("MISSING_MOTION_SEGMENT", &end_key.key_id))?;
            let start = track.keys().nth(index).unwrap().time;
            let segment = &mut Arc::make_mut(&mut track.segments)[index];
            let MotionSegment::Bezier {
                control1,
                control2,
                end,
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
                && control1.time <= control2.time
                && control2.time <= end.time)
            {
                return Err(error(
                    "INVALID_MOTION_BEZIER",
                    "Handle time must stay inside its segment and preserve handle order",
                ));
            }
            Ok(())
        })
    }

    /// Explicit duration scaling transforms keys, control times and events together.
    /// Merely changing clip duration/FPS continues to use set_motion_timing.
    pub fn scale_time(&mut self, document: &Document, duration: f32) -> Result<(), SdkError> {
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
            let restricted = clip.restricted_beziers;
            for track in &mut clip.tracks {
                track.initial.time = scale(track.initial.time);
                let mut start = track.initial.time;
                for segment in Arc::make_mut(&mut track.segments) {
                    segment.end_mut().time = scale(segment.end().time);
                    let end = segment.end().time;
                    if let MotionSegment::Bezier {
                        control1, control2, ..
                    } = segment
                    {
                        control1.time = scale(control1.time);
                        control2.time = scale(control2.time);
                        if restricted {
                            constrain_handles(start, end, control1, control2, true);
                        }
                    }
                    start = end;
                }
            }
            for event in &mut clip.events {
                event.time = scale(event.time);
            }
            clip.duration = duration;
            Ok(())
        })
    }
}

fn lerp(a: MotionPoint, b: MotionPoint, t: f32) -> MotionPoint {
    MotionPoint {
        time: a.time + (b.time - a.time) * t,
        value: a.value + (b.value - a.value) * t,
    }
}

fn split(
    start: MotionPoint,
    segment: &MotionSegment,
    mut key: MotionKey,
    restricted: bool,
) -> Result<(MotionSegment, MotionSegment), SdkError> {
    let end = segment.end().clone();
    Ok(match segment {
        MotionSegment::Linear { .. } => (
            MotionSegment::Linear { end: key },
            MotionSegment::Linear { end },
        ),
        MotionSegment::Stepped { .. } => (
            MotionSegment::Stepped { end: key },
            MotionSegment::Stepped { end },
        ),
        MotionSegment::InverseStepped { .. } => (
            MotionSegment::InverseStepped { end: key },
            MotionSegment::InverseStepped { end },
        ),
        MotionSegment::Bezier {
            control1, control2, ..
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
                MotionSegment::Bezier {
                    control1: a,
                    control2: d,
                    end: key,
                },
                MotionSegment::Bezier {
                    control1: e,
                    control2: c,
                    end,
                },
            );
            verify_bezier_split(start, segment, &result, restricted)?;
            result
        }
    })
}

fn verify_bezier_split(
    start: MotionPoint,
    source: &MotionSegment,
    split: &(MotionSegment, MotionSegment),
    restricted: bool,
) -> Result<(), SdkError> {
    // Large absolute times and short segments can make the existing f32 Cardano
    // branch ill-conditioned even though the mathematical curve is monotone.
    // Reject precision loss rather than silently changing that runtime curve.
    let original = CompiledCurve {
        initial: start,
        segments: Arc::new(vec![source.into()]),
    };
    let candidate = CompiledCurve {
        initial: start,
        segments: Arc::new(vec![(&split.0).into(), (&split.1).into()]),
    };
    let MotionSegment::Bezier {
        control1,
        control2,
        end,
    } = source
    else {
        unreachable!()
    };
    let values = [start.value, control1.value, control2.value, end.value].map(f64::from);
    let minimum = values.into_iter().fold(f64::INFINITY, f64::min);
    let maximum = values.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let tolerance = 0.0001 * (maximum - minimum).max(1.0);
    let span = f64::from(end.time) - f64::from(start.time);
    let times = (0..=128)
        .map(|i| (f64::from(start.time) + span * f64::from(i) / 128.0) as f32)
        .chain(std::iter::once(split.0.end().time));
    for time in times {
        let a = sample_motion_curve(&original, time, restricted);
        let b = sample_motion_curve(&candidate, time, restricted);
        if !a.is_finite() || !b.is_finite() || (f64::from(a) - f64::from(b)).abs() > tolerance {
            return Err(error(
                "MOTION_BEZIER_SPLIT_PRECISION",
                "The f32 runtime cannot preserve this Bezier split within numerical tolerance",
            ));
        }
    }
    Ok(())
}
