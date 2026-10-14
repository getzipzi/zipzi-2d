//! Stateless motion curve sampling. Queue state and fades are separate.
use kasane_core::document::{MotionPoint, MotionSegment, MotionTrack};

/// Local seeks for canonical replay of one clip scheduled at absolute zero.
/// Activation occurs on the first 60 Hz step; use the SDK's returned loop index.
pub fn canonical_motion_seek_time(
    clip: &kasane_core::document::MotionClip,
    loop_index: u64,
    local: f32,
) -> Result<f32, crate::AnimationError> {
    let period = if clip.looping {
        clip.duration + 1. / clip.fps
    } else {
        clip.duration
    };
    if !local.is_finite() || local < 0. || local > period || (!clip.looping && loop_index != 0) {
        return Err(crate::AnimationError::InvalidTime);
    }
    if loop_index == 0 && local == 0. {
        return Ok(0.);
    }
    let mut time = (f64::from(period) * loop_index as f64 + f64::from(local) + 1. / 60.) as f32;
    // An exact seam belongs to the preceding SDK cycle. A preview seek to the
    // next cycle's zero uses the next representable clock value, never a key edit.
    if loop_index > 0 && local == 0. {
        time = time.next_up();
    }
    if !time.is_finite() || time * 60. > 1_000_000. {
        return Err(crate::AnimationError::SeekLimit);
    }
    Ok(time)
}

/// Numeric segments contain no authoring UUIDs or other persistent metadata.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompiledSegment {
    Linear {
        end: MotionPoint,
    },
    Bezier {
        control1: MotionPoint,
        control2: MotionPoint,
        end: MotionPoint,
    },
    Stepped {
        end: MotionPoint,
    },
    InverseStepped {
        end: MotionPoint,
    },
}

impl CompiledSegment {
    pub fn end(&self) -> MotionPoint {
        match self {
            Self::Linear { end }
            | Self::Bezier { end, .. }
            | Self::Stepped { end }
            | Self::InverseStepped { end } => *end,
        }
    }
}

impl From<&MotionSegment> for CompiledSegment {
    fn from(segment: &MotionSegment) -> Self {
        let end = segment.end().point();
        match segment {
            MotionSegment::Linear { .. } => Self::Linear { end },
            MotionSegment::Bezier {
                control1, control2, ..
            } => Self::Bezier {
                control1: *control1,
                control2: *control2,
                end,
            },
            MotionSegment::Stepped { .. } => Self::Stepped { end },
            MotionSegment::InverseStepped { .. } => Self::InverseStepped { end },
        }
    }
}

/// Immutable runtime curve, independent of JSON representation and metadata.
#[derive(Debug, Clone)]
pub struct CompiledCurve {
    /// Initial point, with time in seconds and an unweighted channel value.
    pub initial: MotionPoint,
    /// Shared ordered curve segments; source tracks must satisfy document validation.
    pub segments: std::sync::Arc<Vec<CompiledSegment>>,
}
impl From<&MotionTrack> for CompiledCurve {
    fn from(track: &MotionTrack) -> Self {
        Self {
            initial: track.initial.point(),
            segments: std::sync::Arc::new(
                track.segments.iter().map(CompiledSegment::from).collect(),
            ),
        }
    }
}

/// Sample the same four segment kinds used by Cubism MotionBehavior V2.
/// `restricted_beziers` selects the file's time-linear or Cardano path.
/// `time` is a finite clip-local time in seconds. This low-level sampler assumes
/// validated points; it does not apply looping, fades, parameter clamping or events.
pub fn sample_motion_curve(curve: &CompiledCurve, time: f32, restricted_beziers: bool) -> f32 {
    // End times are strictly increasing. Exact endpoints belong to the next
    // segment, which matters for an outgoing InverseStepped segment.
    let index = curve
        .segments
        .partition_point(|segment| segment.end().time <= time);
    let start = index
        .checked_sub(1)
        .map_or(curve.initial, |i| curve.segments[i].end());
    if let Some(segment) = curve.segments.get(index) {
        let end = segment.end();
        match segment {
            CompiledSegment::Linear { .. } => {
                let t = ((time - start.time) / (end.time - start.time)).max(0.0);
                lerp(start.value, end.value, t)
            }
            CompiledSegment::Bezier {
                control1, control2, ..
            } => {
                let t = motion_bezier_parameter(
                    start,
                    *control1,
                    *control2,
                    end,
                    time,
                    restricted_beziers,
                );
                bezier_value(start, *control1, *control2, end, t)
            }
            CompiledSegment::Stepped { .. } => start.value,
            CompiledSegment::InverseStepped { .. } => end.value,
        }
    } else {
        start.value
    }
}

/// The exact runtime inverse-time branch, shared by sampling and authoring splits.
/// Inputs must be finite and the endpoint times strictly increasing.
pub fn motion_bezier_parameter(
    start: MotionPoint,
    control1: MotionPoint,
    control2: MotionPoint,
    end: MotionPoint,
    time: f32,
    restricted_beziers: bool,
) -> f32 {
    if restricted_beziers {
        ((time - start.time) / (end.time - start.time)).max(0.0)
    } else {
        let a = end.time - 3.0 * control2.time + 3.0 * control1.time - start.time;
        let b = 3.0 * control2.time - 6.0 * control1.time + 3.0 * start.time;
        let c = 3.0 * control1.time - 3.0 * start.time;
        cardano_bezier(a, b, c, start.time - time)
    }
}

/// Whether the cubic time coordinate is monotone over its whole parameter range.
/// Ordered control times are sufficient but not necessary for this condition.
pub fn motion_bezier_time_is_monotonic(start: f32, first: f32, second: f32, end: f32) -> bool {
    if ![start, first, second, end].into_iter().all(f32::is_finite) || start >= end {
        return false;
    }
    let [s, p, q, e] = [start, first, second, end].map(f64::from);
    let a = -s + 3.0 * p - 3.0 * q + e;
    let b = 2.0 * (s - 2.0 * p + q);
    let c = p - s;
    let derivative = |t: f64| (a * t + b) * t + c;
    let mut minimum = derivative(0.0).min(derivative(1.0));
    if a > 0.0 {
        let t = -b / (2.0 * a);
        if (0.0..1.0).contains(&t) {
            minimum = minimum.min(derivative(t));
        }
    }
    minimum >= 0.0
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

fn bezier_value(
    start: MotionPoint,
    first: MotionPoint,
    second: MotionPoint,
    end: MotionPoint,
    t: f32,
) -> f32 {
    let a = lerp(start.value, first.value, t);
    let b = lerp(first.value, second.value, t);
    let c = lerp(second.value, end.value, t);
    lerp(lerp(a, b, t), lerp(b, c, t), t)
}

fn quadratic(a: f32, b: f32, c: f32) -> f32 {
    if a.abs() < 0.00001 {
        if b.abs() < 0.00001 {
            -c
        } else {
            -c / b
        }
    } else {
        -(b + (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
    }
}

fn cardano_bezier(a: f32, b: f32, c: f32, d: f32) -> f32 {
    if a.abs() < 0.00001 {
        return quadratic(b, c, d).clamp(0.0, 1.0);
    }
    let ba = b / a;
    let ca = c / a;
    let da = d / a;
    let p = (3.0 * ca - ba * ba) / 3.0;
    let p3 = p / 3.0;
    let q = (2.0 * ba * ba * ba - 9.0 * ba * ca + 27.0 * da) / 27.0;
    let q2 = q / 2.0;
    let discriminant = q2 * q2 + p3 * p3 * p3;
    let near_center = |root: f32| (root - 0.5).abs() < 0.51;
    if discriminant < 0.0 {
        let r = (-p3 * -p3 * -p3).sqrt();
        let angle = (-q / (2.0 * r)).clamp(-1.0, 1.0).acos();
        let scale = 2.0 * r.cbrt();
        let root1 = scale * (angle / 3.0).cos() - ba / 3.0;
        if near_center(root1) {
            return root1.clamp(0.0, 1.0);
        }
        let root2 = scale * ((angle + 2.0 * std::f32::consts::PI) / 3.0).cos() - ba / 3.0;
        if near_center(root2) {
            return root2.clamp(0.0, 1.0);
        }
        return (scale * ((angle + 4.0 * std::f32::consts::PI) / 3.0).cos() - ba / 3.0)
            .clamp(0.0, 1.0);
    }
    if discriminant == 0.0 {
        let u = if q2 < 0.0 { (-q2).cbrt() } else { -q2.cbrt() };
        let root = 2.0 * u - ba / 3.0;
        return if near_center(root) {
            root
        } else {
            -u - ba / 3.0
        }
        .clamp(0.0, 1.0);
    }
    let sd = discriminant.sqrt();
    ((sd - q2).cbrt() - (sd + q2).cbrt() - ba / 3.0).clamp(0.0, 1.0)
}
