//! Bounded, read-only lowering of authored intervals to motion3's four kinds.
use super::{error, MotionExportLimits, MotionExportSummary, MotionProjectError};
use std::io::Write;
use zipzi_animation::{
    motion_bezier_parameter, motion_bezier_time_is_monotonic, motion_frame_step_boundary,
    sample_motion_curve, sample_motion_segment, CompiledCurve, CompiledSegment,
};
use zipzi_core::document::{MotionClip, MotionCurve, MotionFrameStep, MotionPoint, MotionTrack};

const MAX_GRID_INDEX: u64 = (1 << 53) - 1;
const FIXED_WORKSPACE: usize = 16 << 10;
const SOURCE_SEGMENT_BYTES: usize = 96;
const OUTPUT_SEGMENT_BYTES: usize = 768;

struct EncodedSize {
    bytes: usize,
    limit: usize,
}
impl Write for EncodedSize {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limit)
            .ok_or_else(|| std::io::Error::other("Motion metadata exceeds the export workspace"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) struct Estimate {
    pub summary: MotionExportSummary,
    pub tracks: Vec<usize>,
}

fn limit(message: impl Into<String>) -> MotionProjectError {
    error("MOTION_EXPORT_LIMIT", "$.Curves", message)
}

fn count_add(a: usize, b: usize) -> Result<usize, MotionProjectError> {
    a.checked_add(b)
        .ok_or_else(|| limit("Motion export size overflow"))
}

fn stepped_frames(step: MotionFrameStep, curve: &MotionCurve) -> Option<u32> {
    match (step, curve) {
        (
            MotionFrameStep::EveryFrames { frames: 2.. },
            MotionCurve::Stepped {} | MotionCurve::InverseStepped {},
        ) => None,
        (MotionFrameStep::EveryFrames { frames: 2.. }, _) => match step {
            MotionFrameStep::EveryFrames { frames } => Some(frames),
            _ => unreachable!(),
        },
        _ => None,
    }
}

/// A pair of lower bounds on the canonical represented grid. Binary search
/// handles subframe origins and coalescing f32 ticks without walking pre-roll.
fn ticks(
    start: f32,
    left: f32,
    right: f32,
    fps: f32,
    frames: u32,
) -> Result<std::ops::Range<u64>, MotionProjectError> {
    let boundary = |index| -> Result<f32, MotionProjectError> {
        match motion_frame_step_boundary(start, fps, frames, index) {
            Ok(value) => Ok(value),
            // An out-of-time boundary is an upper sentinel, never emitted.
            Err(zipzi_animation::MotionCurveError::FrameStepPrecision) => Ok(f32::INFINITY),
            Err(e) => Err(error(
                "INVALID_MOTION_FRAME_STEP",
                "$.Curves",
                e.to_string(),
            )),
        }
    };
    if boundary(MAX_GRID_INDEX)? < right {
        return Err(limit(
            "Frame-step indices exceed the exactly enumerable range",
        ));
    }
    let lower_bound = |time: f32, inclusive: bool| -> Result<u64, MotionProjectError> {
        let (mut low, mut high) = (0, MAX_GRID_INDEX);
        while low < high {
            let middle = low + (high - low) / 2;
            let value = boundary(middle)?;
            if value < time || (!inclusive && value == time) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        Ok(low)
    };
    Ok(lower_bound(left, false)?..lower_bound(right, true)?)
}

fn playback_range(clip: &MotionClip, start: f32, end: f32) -> (f32, f32) {
    if clip.source_key_range().is_some() {
        (start.max(0.), end.min(clip.duration))
    } else {
        (start, end)
    }
}

pub(super) fn estimate(
    clip: &MotionClip,
    limits: &MotionExportLimits,
) -> Result<Estimate, MotionProjectError> {
    let source_segments = clip
        .tracks
        .iter()
        .try_fold(0usize, |n, track| count_add(n, track.segments.len()))?;
    let source_bytes = source_segments
        .checked_mul(SOURCE_SEGMENT_BYTES)
        .ok_or_else(|| limit("Motion compilation size overflow"))?;
    let headers = clip
        .tracks
        .len()
        .checked_mul(1024)
        .ok_or_else(|| limit("Motion track count overflow"))?;
    let mut workspace = count_add(FIXED_WORKSPACE, count_add(source_bytes, headers)?)?;
    if workspace > limits.max_workspace_bytes {
        return Err(limit(
            "Motion compilation exceeds the export workspace budget",
        ));
    }
    let mut summary = MotionExportSummary {
        source_segments,
        ..Default::default()
    };
    let mut tracks = Vec::with_capacity(clip.tracks.len());
    for track in &clip.tracks {
        let mut count = 0;
        let mut start = track.initial.time;
        for segment in track.segments.iter() {
            let end = segment.end.time;
            if matches!(segment.curve, MotionCurve::AutoSmooth {}) {
                summary.auto_smooth_segments += 1;
            }
            let (left, right) = playback_range(clip, start, end);
            if left < right {
                let output = if let Some(frames) =
                    stepped_frames(segment.frame_step, &segment.curve)
                {
                    summary.frame_step_intervals += 1;
                    let grid = ticks(start, left, right, clip.fps, frames)?;
                    let size = usize::try_from(grid.end - grid.start)
                        .map_err(|_| limit("Frame-step expansion overflows the output range"))?;
                    // One final hold, and one possible discontinuity bridge at
                    // its start. Normal monotone curves do not need the latter.
                    let bridge = usize::from(
                        !clip.restricted_beziers
                            && matches!(&segment.curve,
                        MotionCurve::Bezier { control1, control2, .. }
                        if !motion_bezier_time_is_monotonic(start, control1.time, control2.time, end)),
                    );
                    count_add(size, 1 + bridge)?
                } else {
                    1
                };
                count = count_add(count, output)?;
            }
            start = end;
        }
        // A crop may end exactly on an outgoing jump, and looping stepped
        // continuous curves retain a final continuous-kind seam marker.
        if clip.source_key_range().is_some()
            || (clip.looping
                && track
                    .segments
                    .last()
                    .is_some_and(|s| stepped_frames(s.frame_step, &s.curve).is_some()))
        {
            count = count_add(count, 1)?;
        }
        summary.estimated_output_segments = count_add(summary.estimated_output_segments, count)?;
        if summary.estimated_output_segments > limits.max_output_segments {
            return Err(limit(format!(
                "Estimated {} segments exceed the {} segment export budget",
                summary.estimated_output_segments, limits.max_output_segments
            )));
        }
        tracks.push(count);
    }
    workspace = count_add(
        workspace,
        summary
            .estimated_output_segments
            .checked_mul(OUTPUT_SEGMENT_BYTES)
            .ok_or_else(|| limit("Motion output size overflow"))?,
    )?;
    // User data and opaque metadata are not quantized. Include their encoded
    // representations in the same estimate rather than exempting them from it.
    let mut metadata = EncodedSize {
        bytes: 0,
        limit: limits.max_workspace_bytes.saturating_sub(workspace) / 128,
    };
    serde_json::to_writer(
        &mut metadata,
        &(
            &clip.name,
            &clip.extensions,
            &clip.meta_extensions,
            &clip.events,
        ),
    )
    .map_err(|_| limit("Motion metadata exceeds the export workspace"))?;
    for track in &clip.tracks {
        serde_json::to_writer(&mut metadata, &track.extensions)
            .map_err(|_| limit("Motion track metadata exceeds the export workspace"))?;
    }
    workspace = count_add(
        workspace,
        metadata
            .bytes
            .checked_mul(128)
            .ok_or_else(|| limit("Motion metadata size overflow"))?,
    )?;
    if workspace > limits.max_workspace_bytes {
        return Err(limit(format!(
            "Estimated {workspace} bytes exceed the {} byte motion export workspace",
            limits.max_workspace_bytes
        )));
    }
    summary.estimated_workspace_bytes = workspace;
    Ok(Estimate { summary, tracks })
}

fn lerp(a: MotionPoint, b: MotionPoint, t: f32) -> MotionPoint {
    let coordinate =
        |a: f32, b: f32| (f64::from(a) + (f64::from(b) - f64::from(a)) * f64::from(t)) as f32;
    MotionPoint {
        time: coordinate(a.time, b.time),
        value: coordinate(a.value, b.value),
    }
}

fn split(p: [MotionPoint; 4], t: f32) -> ([MotionPoint; 4], [MotionPoint; 4]) {
    let a = lerp(p[0], p[1], t);
    let b = lerp(p[1], p[2], t);
    let c = lerp(p[2], p[3], t);
    let d = lerp(a, b, t);
    let e = lerp(b, c, t);
    let middle = lerp(d, e, t);
    ([p[0], a, d, middle], [middle, e, c, p[3]])
}

fn clip_segment(
    start: MotionPoint,
    segment: CompiledSegment,
    left: f32,
    right: f32,
    restricted: bool,
) -> Result<(MotionPoint, CompiledSegment), MotionProjectError> {
    let end = segment.end();
    if left == start.time && right == end.time {
        return Ok((start, segment));
    }
    Ok(match segment {
        CompiledSegment::Bezier {
            control1, control2, ..
        } => {
            if !restricted
                && !motion_bezier_time_is_monotonic(
                    start.time,
                    control1.time,
                    control2.time,
                    end.time,
                )
            {
                return Err(error(
                    "NON_MONOTONIC_MOTION_BEZIER",
                    "$.Curves",
                    "Clipping this imported Bezier cannot preserve its inverse-time branch",
                ));
            }
            let mut p = [start, control1, control2, end];
            if restricted {
                let third = (f64::from(end.time) - f64::from(start.time)) / 3.;
                p[1].time = (f64::from(start.time) + third) as f32;
                p[2].time = (f64::from(end.time) - third) as f32;
            }
            if right < end.time {
                let t = motion_bezier_parameter(p[0], p[1], p[2], p[3], right, restricted);
                p = split(p, t).0;
                p[3].time = right;
                p[3].value = sample_motion_segment(start, &segment, right, restricted);
            }
            if left > start.time {
                let t = motion_bezier_parameter(p[0], p[1], p[2], p[3], left, restricted);
                p = split(p, t).1;
                p[0].time = left;
                p[0].value = sample_motion_segment(start, &segment, left, restricted);
            }
            (
                p[0],
                CompiledSegment::Bezier {
                    control1: p[1],
                    control2: p[2],
                    end: p[3],
                },
            )
        }
        CompiledSegment::Linear { .. } => {
            let value = |time| sample_motion_segment(start, &segment, time, restricted);
            (
                MotionPoint {
                    time: left,
                    value: value(left),
                },
                CompiledSegment::Linear {
                    end: MotionPoint {
                        time: right,
                        value: if right == end.time {
                            end.value
                        } else {
                            value(right)
                        },
                    },
                },
            )
        }
        CompiledSegment::Stepped { .. } => (
            MotionPoint {
                time: left,
                value: start.value,
            },
            CompiledSegment::Stepped {
                end: MotionPoint {
                    time: right,
                    value: if right == end.time {
                        end.value
                    } else {
                        start.value
                    },
                },
            },
        ),
        CompiledSegment::InverseStepped { .. } => (
            MotionPoint {
                time: left,
                value: if left == start.time {
                    start.value
                } else {
                    end.value
                },
            },
            CompiledSegment::InverseStepped {
                end: MotionPoint {
                    time: right,
                    value: end.value,
                },
            },
        ),
    })
}

/// A discontinuity at a stored boundary needs two values: the incoming limit
/// and the outgoing sample. A one-ULP linear segment preserves every f32 query,
/// with no representable time inside the bridge. It is export-only geometry.
fn set_endpoint_value(
    initial: &mut MotionPoint,
    output: &mut Vec<CompiledSegment>,
    value: f32,
    source: &CompiledCurve,
    restricted: bool,
) -> Result<usize, MotionProjectError> {
    let mut value = value;
    let mut pending = Vec::new();
    loop {
        let Some(&last) = output.last() else {
            initial.value = value;
            break;
        };
        if last.end().value == value {
            break;
        }
        let end = last.end();
        let start = if output.len() == 1 {
            *initial
        } else {
            output[output.len() - 2].end()
        };
        let before = end.time.next_down();
        output.pop();
        pending.push(CompiledSegment::Linear {
            end: MotionPoint {
                time: end.time,
                value,
            },
        });
        if before > start.time {
            let (_, clipped) = clip_segment(start, last, start.time, before, restricted)?;
            output.push(clipped);
            break;
        }
        value = sample_motion_curve(source, start.time, restricted);
    }
    let bridges = pending.len();
    output.extend(pending.into_iter().rev());
    Ok(bridges)
}

pub(super) fn track(
    track: &MotionTrack,
    clip: &MotionClip,
    capacity: usize,
    summary: &mut MotionExportSummary,
) -> Result<(MotionPoint, Vec<CompiledSegment>), MotionProjectError> {
    let source = CompiledCurve::from_track(track, clip.fps);
    let cropped = clip.source_key_range().is_some();
    if cropped && source.initial.time >= clip.duration {
        if let Some(CompiledSegment::Bezier {
            control1,
            control2,
            end,
        }) = source.segments.first()
        {
            if stepped_frames(track.segments[0].frame_step, &track.segments[0].curve).is_none()
                && !clip.restricted_beziers
                && !motion_bezier_time_is_monotonic(
                    source.initial.time,
                    control1.time,
                    control2.time,
                    end.time,
                )
            {
                return Err(error(
                    "NON_MONOTONIC_MOTION_BEZIER",
                    "$.Curves",
                    "Cannot crop a non-monotonic Bezier time branch",
                ));
            }
        }
    }
    let mut initial = if cropped {
        MotionPoint {
            time: 0.,
            value: sample_motion_curve(&source, 0., clip.restricted_beziers),
        }
    } else {
        source.initial
    };
    let mut output = Vec::with_capacity(capacity);
    let mut start = source.initial;
    for (index, &segment) in source.segments.iter().enumerate() {
        let end = segment.end();
        let (left, right) = playback_range(clip, start.time, end.time);
        if left >= right {
            start = end;
            continue;
        }
        if let Some(frames) = stepped_frames(
            track.segments[index].frame_step,
            &track.segments[index].curve,
        ) {
            let value = sample_motion_curve(&source, left, clip.restricted_beziers);
            if output.is_empty() {
                initial = MotionPoint { time: left, value };
            } else {
                summary.boundary_segments += set_endpoint_value(
                    &mut initial,
                    &mut output,
                    value,
                    &source,
                    clip.restricted_beziers,
                )?;
            }
            let mut previous_time = left;
            for tick in ticks(start.time, left, right, clip.fps, frames)? {
                let time = motion_frame_step_boundary(start.time, clip.fps, frames, tick)
                    .map_err(|e| error("MOTION_FRAME_STEP_PRECISION", "$.Curves", e.to_string()))?;
                if time <= previous_time {
                    continue;
                }
                let value = sample_motion_curve(&source, time, clip.restricted_beziers);
                output.push(CompiledSegment::Stepped {
                    end: MotionPoint { time, value },
                });
                summary.frame_step_segments += 1;
                previous_time = time;
            }
            let value = if right == end.time {
                end.value
            } else {
                sample_motion_curve(&source, right, clip.restricted_beziers)
            };
            output.push(CompiledSegment::Stepped {
                end: MotionPoint { time: right, value },
            });
            summary.frame_step_segments += 1;
        } else {
            let (point, segment) =
                clip_segment(start, segment, left, right, clip.restricted_beziers)?;
            if output.is_empty() {
                initial = point;
            }
            output.push(segment);
        }
        start = end;
    }
    let final_index = source.segment_index(clip.duration);
    let final_start = if final_index == 0 {
        source.initial
    } else {
        source.segments[final_index - 1].end()
    };
    if cropped
        && final_index < source.segments.len()
        && final_start.time == clip.duration
        && output.last().is_some_and(|s| s.end().time == clip.duration)
    {
        summary.boundary_segments += set_endpoint_value(
            &mut initial,
            &mut output,
            sample_motion_curve(&source, clip.duration, clip.restricted_beziers),
            &source,
            clip.restricted_beziers,
        )?;
    }
    // The extra loop frame is handled by the existing playback layer using the
    // last segment's kind. Preserve that continuous seam after Step expansion.
    if clip.looping
        && track
            .segments
            .last()
            .is_some_and(|s| stepped_frames(s.frame_step, &s.curve).is_some())
        && output
            .last()
            .is_some_and(|s| s.end().time == source.segments.last().unwrap().end().time)
    {
        if let Some(last @ CompiledSegment::Stepped { .. }) = output.last().copied() {
            let end = last.end();
            let start = if output.len() == 1 {
                initial
            } else {
                output[output.len() - 2].end()
            };
            let before = end.time.next_down();
            output.pop();
            if before > start.time {
                output.push(CompiledSegment::Stepped {
                    end: MotionPoint {
                        time: before,
                        value: start.value,
                    },
                });
            }
            output.push(CompiledSegment::Linear { end });
            summary.boundary_segments += 1;
        }
    }
    summary.output_segments = count_add(summary.output_segments, output.len())?;
    Ok((initial, output))
}
