//! Numerical building blocks for authored curves. Persistent modes and editing
//! transactions belong to Contract/Core/SDK; these operations own no history.

use std::sync::Arc;
use zipzi_contract::motion_curve::{MotionEasingPreset, MotionFrameStep};
use zipzi_core::document::MotionPoint;

use crate::{CompiledCurve, CompiledSegment};

/// Invalid numerical input to curve compilation or frame-step sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MotionCurveError {
    /// A group needs an initial key, even when it has no outgoing segments.
    #[error("a curve requires at least one key")]
    EmptyCurve,
    /// Key times, values and sample times must be finite.
    #[error("curve coordinates and sample times must be finite")]
    NonFinitePoint,
    /// Adjacent keys must have strictly increasing times.
    #[error("curve key times must be strictly increasing")]
    UnorderedKeys,
    /// Clip FPS must be finite and greater than zero.
    #[error("frame-step sampling requires finite, positive FPS")]
    InvalidFps,
    /// Frame counts must be positive integers; one means Off.
    #[error("frame-step count must be greater than zero")]
    InvalidFrameStep,
}

fn validate_point(point: MotionPoint) -> Result<(), MotionCurveError> {
    if point.time.is_finite() && point.value.is_finite() {
        Ok(())
    } else {
        Err(MotionCurveError::NonFinitePoint)
    }
}

/// Map the shared normalized preset onto an ascending, descending or flat
/// interval. Compute in f64 so finite f32 endpoints of opposite signs cannot
/// overflow while taking their difference. Endpoints are never changed.
pub fn motion_easing_controls(
    preset: MotionEasingPreset,
    start: MotionPoint,
    end: MotionPoint,
) -> Result<[MotionPoint; 2], MotionCurveError> {
    validate_point(start)?;
    validate_point(end)?;
    if end.time <= start.time {
        return Err(MotionCurveError::UnorderedKeys);
    }
    Ok(preset.definition().controls.map(|[x, y]| MotionPoint {
        time: (f64::from(start.time) + f64::from(x) * (f64::from(end.time) - f64::from(start.time)))
            as f32,
        value: (f64::from(start.value)
            + f64::from(y) * (f64::from(end.value) - f64::from(start.value))) as f32,
    }))
}

/// Compile one uninterrupted Auto Smooth group into explicit cubic Beziers.
///
/// Group endpoints have zero slope. Interior slopes use the time-weighted
/// harmonic mean (PCHIP); a flat interval or change of direction gives a zero
/// slope. The result is monotone on each monotone interval without overshoot.
/// Callers split groups at every non-automatic segment, so manual controls never
/// influence or get rewritten by this operation. A single key has no segments.
pub fn compile_motion_auto_smooth(
    points: &[MotionPoint],
) -> Result<CompiledCurve, MotionCurveError> {
    let Some(&initial) = points.first() else {
        return Err(MotionCurveError::EmptyCurve);
    };
    for &point in points {
        validate_point(point)?;
    }
    for pair in points.windows(2) {
        if pair[0].time >= pair[1].time {
            return Err(MotionCurveError::UnorderedKeys);
        }
    }

    let mut slope = 0.0;
    let mut segments = Vec::with_capacity(points.len() - 1);
    for (index, pair) in points.windows(2).enumerate() {
        let [start, end] = [pair[0], pair[1]];
        let next_slope = points.get(index + 2).map_or(0.0, |&next| {
            let left_width = f64::from(end.time) - f64::from(start.time);
            let right_width = f64::from(next.time) - f64::from(end.time);
            let left = (f64::from(end.value) - f64::from(start.value)) / left_width;
            let right = (f64::from(next.value) - f64::from(end.value)) / right_width;
            if left == 0.0 || right == 0.0 || left.is_sign_positive() != right.is_sign_positive() {
                0.0
            } else {
                let w1 = 2.0 * right_width + left_width;
                let w2 = right_width + 2.0 * left_width;
                (w1 + w2) / (w1 / left + w2 / right)
            }
        });
        let third = (f64::from(end.time) - f64::from(start.time)) / 3.0;
        segments.push(CompiledSegment::Bezier {
            control1: MotionPoint {
                time: (f64::from(start.time) + third) as f32,
                value: (f64::from(start.value) + slope * third) as f32,
            },
            control2: MotionPoint {
                time: (f64::from(end.time) - third) as f32,
                value: (f64::from(end.value) - next_slope * third) as f32,
            },
            end,
        });
        slope = next_slope;
    }
    Ok(CompiledCurve {
        initial,
        segments: Arc::new(segments),
    })
}

/// Quantize a selected segment's sample time, anchored to that segment's start.
///
/// Select the outgoing segment using the original time *before* calling this
/// function; at its endpoint the following segment takes over immediately.
/// Off and a raw one-frame setting preserve subframe time exactly. No playback
/// state, event clock or fade/physics time is changed.
///
/// Grid boundaries are calculated in f64 and rounded once into the curve's f32
/// time domain. Comparing the neighboring represented boundary makes a tick and
/// its `next_down()` distinct even when the ideal rational tick rounds downward.
/// No epsilon advances a sample that still precedes the boundary.
pub fn motion_frame_step_time(
    start: f32,
    time: f32,
    fps: f32,
    step: MotionFrameStep,
) -> Result<f32, MotionCurveError> {
    if !start.is_finite() || !time.is_finite() {
        return Err(MotionCurveError::NonFinitePoint);
    }
    if !fps.is_finite() || fps <= 0.0 {
        return Err(MotionCurveError::InvalidFps);
    }
    let step = step
        .normalized()
        .ok_or(MotionCurveError::InvalidFrameStep)?;
    let MotionFrameStep::EveryFrames { frames } = step else {
        return Ok(time);
    };
    if time <= start {
        return Ok(start);
    }
    let boundary =
        |index: f64| (f64::from(start) + index * f64::from(frames) / f64::from(fps)) as f32;
    let mut index =
        ((f64::from(time) - f64::from(start)) * f64::from(fps) / f64::from(frames)).floor();
    if boundary(index) > time {
        index = (index - 1.0).max(0.0);
    }
    let current = boundary(index);
    let next = boundary(index + 1.0);
    Ok(if next <= time { next } else { current })
}
