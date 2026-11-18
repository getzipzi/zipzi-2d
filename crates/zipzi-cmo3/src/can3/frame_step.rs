//! Native common Frame Step schedules, with per-channel Step lowering otherwise.
use super::*;
use zipzi_animation::{motion_frame_step_boundary, sample_motion_curve, CompiledCurve};
use zipzi_core::document::MotionFrameStep;

pub(super) fn enabled(track: &MotionTrack) -> bool {
    track
        .segments
        .iter()
        .any(|s| matches!(s.frame_step, MotionFrameStep::EveryFrames { frames: 2.. }))
}

fn grid_frame(time: f32, fps: f32) -> Option<i32> {
    let frame = (f64::from(time) * f64::from(fps)).round();
    (frame >= f64::from(i32::MIN)
        && frame <= f64::from(i32::MAX)
        && (frame / f64::from(fps)) as f32 == time)
        .then_some(frame as i32)
}

/// A model-wide property must never retime an independently animated channel.
/// A shared fully-stepped schedule maps directly; all other selections lower
/// independently. Empty/static channels are unaffected by the common grid.
pub(super) fn native_schedule(clip: &MotionClip, fps: f32) -> Vec<(i32, i32)> {
    if fps != clip.fps {
        return Vec::new();
    }
    let mut common = None;
    for track in clip
        .tracks
        .iter()
        .filter(|track| !track.segments.is_empty())
    {
        let mut schedule = Vec::new();
        let mut start = &track.initial;
        for segment in track.segments.iter() {
            let MotionFrameStep::EveryFrames {
                frames: frames @ 2..,
            } = segment.frame_step
            else {
                return Vec::new();
            };
            // CAN3 uses signed integer counts and its first inverse-step anchor
            // has a different exact-boundary convention. Lower those locally.
            if frames > i32::MAX as u32 || matches!(segment.curve, MotionCurve::InverseStepped {}) {
                return Vec::new();
            }
            if let MotionCurve::Bezier {
                control1, control2, ..
            } = &segment.curve
            {
                if !clip.restricted_beziers
                    && !zipzi_animation::motion_bezier_time_is_monotonic(
                        start.time,
                        control1.time,
                        control2.time,
                        segment.end.time,
                    )
                {
                    return Vec::new();
                }
            }
            let Some(frame) = grid_frame(start.time, fps) else {
                return Vec::new();
            };
            if frame < 0 {
                return Vec::new();
            }
            schedule.push((frame, frames as i32));
            start = &segment.end;
        }
        let Some(end) = grid_frame(start.time, fps) else {
            return Vec::new();
        };
        schedule.push((end, 1));
        if common
            .as_ref()
            .is_some_and(|previous| previous != &schedule)
        {
            return Vec::new();
        }
        common = Some(schedule);
    }
    common.unwrap_or_default()
}

fn reserve(count: usize, id: &str, budget: &mut usize) -> Result<(), Cmo3Error> {
    if count > 1_000_000 {
        return Err(Cmo3Error::InvalidDocument(format!(
            "{id}: CAN3 Frame Step expansion exceeds one million keys"
        )));
    }
    *budget = budget.checked_sub(count).ok_or_else(|| {
        Cmo3Error::InvalidDocument("CAN3 animation baking exceeds ten million keys".into())
    })?;
    Ok(())
}

pub(super) fn keys(
    track: &MotionTrack,
    clip: &MotionClip,
    fps: f32,
    inferred: bool,
    report: &mut Cmo3Report,
    budget: &mut usize,
) -> Result<Vec<Key>, Cmo3Error> {
    let compiled = CompiledCurve::from_track(track, clip.fps);
    let ratio = f64::from(fps) / f64::from(clip.fps);
    let grid: Option<Vec<_>> = track.keys().map(|k| grid_frame(k.time, fps)).collect();
    let nonmonotonic = !clip.restricted_beziers
        && compiled.segments.iter().enumerate().any(|(i, segment)| {
            let zipzi_animation::CompiledSegment::Bezier {
                control1,
                control2,
                end,
            } = segment
            else {
                return false;
            };
            let start = if i == 0 {
                track.initial.time
            } else {
                track.segments[i - 1].end.time
            };
            !zipzi_animation::motion_bezier_time_is_monotonic(
                start,
                control1.time,
                control2.time,
                end.time,
            )
        });
    let counts: Option<Vec<_>> = track
        .segments
        .iter()
        .map(|s| match s.frame_step {
            MotionFrameStep::EveryFrames {
                frames: frames @ 2..,
            } => {
                let n = f64::from(frames) * ratio;
                (n.fract() == 0. && n >= 1. && n <= u32::MAX as f64).then_some(n as u32)
            }
            _ => Some(0),
        })
        .collect();
    let (Some(grid), Some(counts)) = (grid, counts) else {
        // Integer anchors cannot retain fractional origins. Evaluate before
        // quantizing, at the actual scene FPS, and retain held spans as Step.
        // This is a reported conversion, never a reason to omit Frame Step.
        return bake::keys_at_fps(track, clip, fps, report, budget);
    };
    if nonmonotonic || grid.windows(2).any(|p| p[0] >= p[1]) {
        return bake::keys_at_fps(track, clip, fps, report, budget);
    }
    let mut size = track.segments.len() + 1;
    for (i, segment) in track.segments.iter().enumerate() {
        if counts[i] > 0
            && !matches!(
                segment.curve,
                MotionCurve::Stepped {} | MotionCurve::InverseStepped {}
            )
        {
            size = size
                .checked_add(
                    ((i64::from(grid[i + 1]) - i64::from(grid[i]) - 1) / i64::from(counts[i]))
                        as usize,
                )
                .ok_or_else(|| {
                    Cmo3Error::InvalidDocument("CAN3 Frame Step key count overflow".into())
                })?;
        }
        // Inverse repair can insert a final-frame guard into an unstepped
        // continuous prefix. Include those keys in the same preflight budget.
        if counts[i] == 0
            && matches!(
                segment.curve,
                MotionCurve::Linear {} | MotionCurve::AutoSmooth {} | MotionCurve::Bezier { .. }
            )
            && i64::from(grid[i + 1]) - i64::from(grid[i]) > 1
            && track.segments.get(i + 1).is_some_and(|next| {
                matches!(next.curve, MotionCurve::InverseStepped {})
                    && segment.end.value != next.end.value
            })
        {
            size = size.checked_add(1).ok_or_else(|| {
                Cmo3Error::InvalidDocument("CAN3 Frame Step key count overflow".into())
            })?;
        }
    }
    reserve(size, &track.id, budget)?;
    let source = super::keys(track, fps, report, inferred, clip.restricted_beziers)?;
    let mut output = Vec::with_capacity(size);
    output.push(source[0].clone());
    let mut baked = 0;
    for (i, segment) in track.segments.iter().enumerate() {
        if counts[i] > 0
            && !matches!(
                segment.curve,
                MotionCurve::Stepped {} | MotionCurve::InverseStepped {}
            )
        {
            output.last_mut().unwrap().curve = "STEP";
            let MotionFrameStep::EveryFrames { frames } = segment.frame_step else {
                unreachable!()
            };
            let origin = if i == 0 {
                track.initial.time
            } else {
                track.segments[i - 1].end.time
            };
            let mut q = 1_u64;
            let end = i64::from(grid[i + 1]);
            let mut frame = i64::from(grid[i]) + i64::from(counts[i]);
            while frame < end {
                let time = motion_frame_step_boundary(origin, clip.fps, frames, q)
                    .map_err(|e| Cmo3Error::InvalidDocument(e.to_string()))?;
                let value = f64::from(sample_motion_curve(
                    &compiled,
                    time,
                    clip.restricted_beziers,
                ));
                output.push(Key {
                    frame: frame as i32,
                    value,
                    curve: "STEP",
                    prev: (frame as f64, value),
                    next: (frame as f64, value),
                });
                frame += i64::from(counts[i]);
                q += 1;
            }
            baked += 1;
        } else {
            output.last_mut().unwrap().curve = source[i].curve;
            output.last_mut().unwrap().next = source[i].next;
        }
        output.push(source[i + 1].clone());
    }
    bake::info(
        report,
        &track.id,
        "CAN3_FRAME_STEP_BAKED",
        &format!("Converted {baked} interval Frame Step settings to {} editable keys at {fps} FPS; unstepped curves retain their anchors and handles", output.len()),
    );
    // This conversion also needs Animator's exact inverse-step boundary repair.
    Ok(inverse::repair(output, &track.id, report))
}
